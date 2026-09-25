//! Re-roll 批次執行層（openspec/specs/reroll T3）
//!
//! 職責只有「批次迴圈 + 覆寫保護 + 圖譜同步」：搜尋與 stale 判定在 `reroll.rs`
//! （T2 計畫層），單支錄製一律經 `dispatcher::execute_script`（T1 共用入口，
//! REQ-2.1 零旁路）。批次核心以 `ScriptExecutor` 注入隔離測試（design §7），
//! 不依賴真實錄製。

use crate::cli::RerollArgs;
use crate::db::AssetTracker;
use crate::engine::dispatcher::{execute_script, resolve_engine, RunOptions};
use crate::engine::reroll::{find_rolls, plan_item, RerollItem};
use crate::engine::roll_parser::{parse_roll_script, Engine};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// 暫存副檔名（REQ-2.4）：錄製寫 `<target>.reroll-tmp`，成功後原子置換
const REROLL_TMP_EXT: &str = ".reroll-tmp";

// ─────────────────────────────────────────────
// executor 抽象（design §7 注入式）
// ─────────────────────────────────────────────

/// 單支執行：輸入 .roll 與輸出覆寫（reroll 唯一合法用途＝指向暫存路徑），
/// 回傳實際輸出路徑。production 實作一律走 `dispatcher::execute_script`。
#[async_trait]
pub trait ScriptExecutor: Send + Sync {
    async fn execute(&self, roll: &Path, output_override: Option<&Path>) -> Result<PathBuf>;
}

/// production executor：與 `tapedeck run` 同一條路徑（腳本預設，無旁路）
struct RealExecutor;

#[async_trait]
impl ScriptExecutor for RealExecutor {
    async fn execute(&self, roll: &Path, output_override: Option<&Path>) -> Result<PathBuf> {
        let script = parse_roll_script(roll)?;
        let opts = RunOptions {
            output: output_override.map(|p| p.to_path_buf()),
            ..RunOptions::default()
        };
        execute_script(roll, script, &opts).await
    }
}

// ─────────────────────────────────────────────
// 批次迴圈
// ─────────────────────────────────────────────

/// 彙總統計（REQ-3.2）
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Stats {
    success: usize,
    failed: usize,
    skipped: usize,
}

/// CLI 批次入口：計畫 → 提示 → 批次 → 彙總。
/// 回傳是否需 exit 非零（有失敗）；`--stale-only` 全 skip → exit 0。
pub async fn run(args: &RerollArgs) -> Result<bool> {
    let root = match &args.path {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };

    // REQ-4.1：計畫含 Native 支 → 批次開始前列印真實桌面錄製提示
    if batch_needs_desktop(&root)? {
        eprintln!("⚠️ 批次含 Native 支：部分重錄為真實桌面錄製，期間請勿干擾目標視窗。");
    }

    // db 開不起來才整批 bail（design §6：圖譜同步是核心承諾）
    let tracker = AssetTracker::open()?;
    let cwd = std::env::current_dir()?;
    let executor = RealExecutor;

    let (lines, has_failure) = run_batch(
        &root,
        args.stale_only,
        args.clean,
        &tracker,
        &executor,
        &cwd,
    )
    .await?;

    for line in &lines {
        println!("{line}");
    }
    Ok(has_failure)
}

/// 批次核心（design §7）：順序執行（REQ-1.3）、單支失敗不中斷、成功登錄圖譜，
/// `--clean` 顯式才清孤兒（REQ-3.1）。回傳 (逐行訊息, 有無失敗)。
async fn run_batch(
    root: &Path,
    stale_only: bool,
    clean: bool,
    tracker: &AssetTracker,
    executor: &dyn ScriptExecutor,
    clean_root: &Path,
) -> Result<(Vec<String>, bool)> {
    let mut lines = Vec::new();
    let mut stats = Stats::default();

    for roll in find_rolls(root) {
        match plan_item(&roll, stale_only, tracker) {
            Ok(item) => {
                if item.stale {
                    match record_and_swap(executor, tracker, &item).await {
                        Ok(()) => {
                            stats.success += 1;
                            lines.push(format!(
                                "✅ {} → {}",
                                item.roll.display(),
                                item.output.display()
                            ));
                        }
                        Err(e) => {
                            stats.failed += 1;
                            lines.push(format!("❌ {} — {e:#}", item.roll.display()));
                        }
                    }
                } else {
                    stats.skipped += 1;
                    lines.push(format!("⏭️ {}", item.roll.display()));
                }
            }
            // 計畫層容錯（REQ-1.3＋design §6）：單支 parse／輸出解析失敗 → ❌ 繼續
            Err(e) => {
                stats.failed += 1;
                lines.push(format!("❌ {} — 計畫失敗：{e:#}", roll.display()));
            }
        }
    }

    // --clean（REQ-3.1）：僅顯式帶 flag 時清孤兒（與 `tapedeck clean` 同款掃描）
    if clean {
        match tracker.orphans(clean_root) {
            Ok(orphans) if orphans.is_empty() => lines.push("🧹 無孤兒資產".to_owned()),
            Ok(orphans) => {
                for asset in &orphans {
                    match tracker.remove(asset, false) {
                        Ok(msg) => lines.push(format!("🧹 {msg}")),
                        Err(e) => {
                            stats.failed += 1;
                            lines.push(format!("❌ 孤兒清理失敗 — {e:#}"));
                        }
                    }
                }
            }
            Err(e) => {
                stats.failed += 1;
                lines.push(format!("❌ 孤兒掃描失敗 — {e:#}"));
            }
        }
    }

    // --stale-only 全 skip 且無任何錄製 → nothing to do（REQ-3.2／design §6）
    if stats.success == 0 && stats.failed == 0 && stats.skipped > 0 {
        lines.push("nothing to do".to_owned());
    }

    lines.push(format!(
        "彙總：{} 成功、{} 失敗、{} skip",
        stats.success, stats.failed, stats.skipped
    ));
    Ok((lines, stats.failed > 0))
}

/// 單支執行：temp 原子置換（REQ-2.4）＋成功登錄目標路徑（REQ-2.2）。
/// 置換失敗時該支 Err（計入 fail），暫存保留供除錯（design §6）。
async fn record_and_swap(
    executor: &dyn ScriptExecutor,
    tracker: &AssetTracker,
    item: &RerollItem,
) -> Result<()> {
    let mut os = item.output.clone().into_os_string();
    os.push(REROLL_TMP_EXT);
    let tmp_path = PathBuf::from(os);

    // 錄製寫暫存（executor 經 RunOptions.output 覆寫），回傳實際輸出
    let actual = executor.execute(&item.roll, Some(&tmp_path)).await?;

    if actual != item.output {
        // 原子置換：rename 優先；跨裝置 fallback copy + delete（design §3）
        if std::fs::rename(&actual, &item.output).is_err() {
            std::fs::copy(&actual, &item.output).with_context(|| {
                format!(
                    "暫存置換失敗（rename 與 copy 皆失敗），暫存保留於 {}",
                    actual.display()
                )
            })?;
            std::fs::remove_file(&actual)
                .with_context(|| format!("暫存檔刪除失敗: {}", actual.display()))?;
        }
    }

    // 成功支自動登錄「置換後的目標」，不是暫存（REQ-2.2）
    tracker
        .register(&item.output, Some(&item.roll.to_string_lossy()))
        .with_context(|| format!("資產登錄失敗: {}", item.output.display()))
}

/// REQ-4.1：任一支解析為 Native 引擎 → 需要真實桌面（解析失敗支不在此提示，
/// 批次時會個別計入 fail）
fn batch_needs_desktop(root: &Path) -> Result<bool> {
    for roll in find_rolls(root) {
        if let Ok(script) = parse_roll_script(&roll) {
            if resolve_engine(&script) == Engine::Native {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    /// mock executor：依 roll 路徑子字串決定成功（寫出覆寫路徑檔）或失敗；
    /// 未命中的 roll 預設成功。不碰真引擎、不碰 XDG 全域 env
    /// （測試腳本一律絕對輸出路徑，resolve_output_path 原樣保留）。
    struct MockExecutor {
        /// (roll 路徑含此子字串, 是否成功)
        outcomes: Vec<(String, bool)>,
    }

    #[async_trait]
    impl ScriptExecutor for MockExecutor {
        async fn execute(&self, roll: &Path, output_override: Option<&Path>) -> Result<PathBuf> {
            let ok = self
                .outcomes
                .iter()
                .find(|(s, _)| roll.to_string_lossy().contains(s))
                .map(|(_, ok)| *ok)
                .unwrap_or(true);
            let out = output_override
                .unwrap_or_else(|| Path::new("/nonexistent/default.webm"))
                .to_path_buf();
            if ok {
                // 模擬真實錄製：實際落在覆寫路徑（= 暫存）
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&out, format!("recording-of-{}", roll.display())).unwrap();
                Ok(out)
            } else {
                anyhow::bail!("模擬錄製失敗: {}", roll.display())
            }
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-reroll-batch-{tag}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 寫一支測試 .roll；`output` 為完整絕對輸出路徑（避免 XDG 全域 env）
    fn write_roll(dir: &Path, rel: &str, output: &Path) -> PathBuf {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            format!(
                "Set Engine VHS\nSet Output \"{}\"\nType \"hi\"\n",
                output.display()
            ),
        )
        .unwrap();
        p
    }

    fn tmp_db(tag: &str) -> AssetTracker {
        let dir = tmpdir(tag);
        AssetTracker::open_at(&dir.join("tapedeck.db")).unwrap()
    }

    /// 回推檔案 mtime 兩小時（讓 roll 比資產舊 → keep）
    fn age_file(path: &Path) {
        let past = SystemTime::now() - Duration::from_secs(7200);
        fs::File::options()
            .append(true)
            .open(path)
            .unwrap()
            .set_modified(past)
            .unwrap();
    }

    async fn batch(
        dir: &Path,
        stale_only: bool,
        clean: bool,
        tracker: &AssetTracker,
        executor: &dyn ScriptExecutor,
    ) -> Result<(Vec<String>, bool)> {
        run_batch(dir, stale_only, clean, tracker, executor, dir).await
    }

    // ── 失敗不中斷 + 彙總計數（REQ-1.3）──

    #[tokio::test]
    async fn failed_roll_does_not_stop_batch() {
        let dir = tmpdir("failstop");
        let _a = write_roll(&dir, "a_fail.roll", &dir.join("assets/a.webm"));
        let _b = write_roll(&dir, "b_ok.roll", &dir.join("assets/b.webm"));
        let tracker = tmp_db("failstop");
        let mock = MockExecutor {
            outcomes: vec![("a_fail.roll".to_owned(), false)],
        };

        let (lines, has_failure) = batch(&dir, false, false, &tracker, &mock).await.unwrap();

        // a❌ 之後 b 仍 ✅：單支失敗不中斷
        assert_eq!(lines.iter().filter(|l| l.starts_with('❌')).count(), 1);
        assert_eq!(lines.iter().filter(|l| l.starts_with('✅')).count(), 1);
        // 彙總行計數
        assert!(lines.last().unwrap().contains("1 成功、1 失敗、0 skip"));
        // exit code 決策：有失敗 → 非零
        assert!(has_failure);
    }

    #[tokio::test]
    async fn parse_fail_counts_as_failed_and_continues() {
        let dir = tmpdir("parsefail");
        // 語法錯誤的 roll → 計畫失敗
        fs::write(dir.join("bad.roll"), "NotARealCommand x\n").unwrap();
        let _ok = write_roll(&dir, "good.roll", &dir.join("assets/g.webm"));
        let tracker = tmp_db("parsefail");
        let mock = MockExecutor { outcomes: vec![] };

        let (lines, has_failure) = batch(&dir, false, false, &tracker, &mock).await.unwrap();

        assert!(lines
            .iter()
            .any(|l| l.starts_with("❌") && l.contains("bad.roll")));
        assert_eq!(lines.iter().filter(|l| l.starts_with('✅')).count(), 1);
        assert!(lines.last().unwrap().contains("1 成功、1 失敗、0 skip"));
        assert!(has_failure);
    }

    // ── stale skip 語意（REQ-2.3）──

    #[tokio::test]
    async fn stale_only_all_skipped_is_nothing_to_do() {
        let dir = tmpdir("allskip");
        let a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("allskip");
        // 已登錄且資產較新 → keep（skip）
        let asset = dir.join("assets/a.webm");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, b"old").unwrap();
        age_file(&a); // roll 兩小時前 → 比資產舊
        tracker
            .register(&asset, Some(a.to_string_lossy().as_ref()))
            .unwrap();

        let (lines, has_failure) = batch(
            &dir,
            true,
            false,
            &tracker,
            &MockExecutor { outcomes: vec![] },
        )
        .await
        .unwrap();

        assert_eq!(lines.iter().filter(|l| l.starts_with("⏭️")).count(), 1);
        assert!(lines.iter().any(|l| l.contains("nothing to do")));
        assert!(lines.last().unwrap().contains("0 成功、0 失敗、1 skip"));
        // 全 skip → exit 0
        assert!(!has_failure);
    }

    #[tokio::test]
    async fn stale_only_unregistered_is_executed() {
        let dir = tmpdir("fresh");
        let _a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("fresh");
        let mock = MockExecutor { outcomes: vec![] };

        let (lines, has_failure) = batch(&dir, true, false, &tracker, &mock).await.unwrap();

        // 從未登錄 → stale 一律重錄（REQ-2.3）
        assert_eq!(lines.iter().filter(|l| l.starts_with('✅')).count(), 1);
        assert!(!has_failure);
    }

    // ── temp 原子置換（REQ-2.4）兩態 ──

    #[tokio::test]
    async fn success_swaps_target_and_registers() {
        let dir = tmpdir("swap-ok");
        let a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("swap-ok");
        let mock = MockExecutor { outcomes: vec![] };

        let (lines, has_failure) = batch(&dir, false, false, &tracker, &mock).await.unwrap();
        assert!(!has_failure);
        assert_eq!(lines.iter().filter(|l| l.starts_with('✅')).count(), 1);

        // 目標被新錄內容置換（暫存已消失）
        let target = dir.join("assets/a.webm");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            format!("recording-of-{}", a.display())
        );
        assert!(!target.with_file_name("a.webm.reroll-tmp").exists());
        // 成功支有登錄，且 source_roll = roll 路徑、path = 置換後目標（REQ-2.2）
        let got = tracker
            .latest_by_source(&a.to_string_lossy())
            .unwrap()
            .expect("成功支應已登錄");
        assert_eq!(got.path, target);
        assert_eq!(
            got.source_roll.as_deref(),
            Some(a.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn swap_failure_keeps_temp_and_old_target() {
        let dir = tmpdir("swap-fail");
        let _a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("swap-fail");
        // mock「成功」但實際輸出落在別處（不為覆寫路徑）→ rename 失敗且 copy 來源不存在 → 置換失敗
        struct MisbehavingExecutor;
        #[async_trait]
        impl ScriptExecutor for MisbehavingExecutor {
            async fn execute(&self, _roll: &Path, _output: Option<&Path>) -> Result<PathBuf> {
                Ok(PathBuf::from("/nonexistent/gone.reroll-tmp"))
            }
        }

        let (lines, has_failure) = batch(&dir, false, false, &tracker, &MisbehavingExecutor)
            .await
            .unwrap();

        assert!(has_failure);
        assert_eq!(lines.iter().filter(|l| l.starts_with('❌')).count(), 1);
        assert!(lines.iter().any(|l| l.contains("置換失敗")));
        // 失敗支不登錄
        assert!(tracker.assets().unwrap().is_empty());
    }

    // ── --clean（REQ-3.1）──

    #[tokio::test]
    async fn clean_flag_removes_orphans() {
        let dir = tmpdir("cleanon");
        let _a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("cleanon");
        let mock = MockExecutor { outcomes: vec![] };
        // 孤兒：db 有登錄、無 .md 引用（與 `tapedeck clean` 同款掃描）
        let orphan = dir.join("assets/trash.webm");
        fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        fs::write(&orphan, b"junk").unwrap();
        tracker.register(&orphan, None).unwrap();

        let (lines, _) = batch(&dir, false, true, &tracker, &mock).await.unwrap();

        assert!(!orphan.exists(), "孤兒應被清理");
        assert!(lines.iter().any(|l| l.contains("已刪除孤兒")));
    }

    #[tokio::test]
    async fn without_clean_flag_nothing_is_removed() {
        let dir = tmpdir("cleanoff");
        let _a = write_roll(&dir, "a.roll", &dir.join("assets/a.webm"));
        let tracker = tmp_db("cleanoff");
        let mock = MockExecutor { outcomes: vec![] };
        let orphan = dir.join("assets/trash.webm");
        fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        fs::write(&orphan, b"junk").unwrap();
        tracker.register(&orphan, None).unwrap();

        let (_lines, _) = batch(&dir, false, false, &tracker, &mock).await.unwrap();

        // 預設不刪除（防誤殺）：孤兒仍在，db row 也在
        assert!(orphan.exists());
        assert_eq!(tracker.assets().unwrap().len(), 2); // 重錄登錄的 a.webm + 孤兒
    }
}
