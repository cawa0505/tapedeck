//! Re-roll 計畫層（openspec/specs/reroll T2）
//!
//! 只「產出計畫」：搜尋 .roll、解析引擎與輸出路徑、依圖譜 mtime 判 stale。
//! 批次執行／登錄／置換屬 T3，本檔不做。

use crate::db::AssetTracker;
use crate::engine::dispatcher::resolve_engine;
use crate::engine::roll_parser::{parse_roll_script, Engine};
use crate::paths::resolve_output_path;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 遞迴蒐集 `*.roll`（std::fs 手寫，不新增依賴），結果依路徑字串排序穩定
pub fn find_rolls(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map(|n| n != ".git").unwrap_or(false) {
                    out.extend(find_rolls(&path));
                }
            } else if path.extension().map(|e| e == "roll").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    out.sort_by_key(|p| p.to_string_lossy().into_owned());
    out
}

/// 單支計畫項：roll 路徑、引擎、輸出路徑、stale（include）與否
pub struct RerollItem {
    pub roll: PathBuf,
    pub engine: Engine,
    pub output: PathBuf,
    /// include 決策：false 表示 --stale-only 下 skip（keep）
    pub stale: bool,
}

/// 產出批次計畫：每支 roll 一筆計畫項。
/// 未帶 `--stale-only` → 全數 include；帶時：db 無登錄記錄 → stale（include）、
/// `.roll` mtime > 資產 mtime → stale（include）、否則 skip。
pub fn build_plan(
    rolls: &[PathBuf],
    stale_only: bool,
    tracker: &AssetTracker,
) -> Result<Vec<RerollItem>> {
    let mut plan = Vec::new();
    for roll in rolls {
        let script = parse_roll_script(roll)?;
        let engine = resolve_engine(&script);
        // 空字串視同未指定 → 預設 output.webm
        let raw_output = script.output.as_deref().unwrap_or("output.webm");
        let output = resolve_output_path(
            if raw_output.is_empty() {
                "output.webm"
            } else {
                raw_output
            },
            None,
        )?;

        let stale = if !stale_only {
            true
        } else {
            match tracker.latest_by_source(&roll.to_string_lossy())? {
                // 無登錄記錄 → stale
                None => true,
                Some(asset) => file_mtime(roll) > asset.mtime,
            }
        };

        plan.push(RerollItem {
            roll: roll.clone(),
            engine,
            output,
            stale,
        });
    }
    Ok(plan)
}

/// 檔案 mtime（秒）；取不到（不存在／早於 epoch）→ 0
fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// T2 入口：`--dry-run` 計畫輸出。每支一行 `roll 路徑 | engine | 輸出路徑 | stale/keep`，
/// 結尾總數一行。不錄製、不寫 db（僅唯讀查詢）、不建輸出目錄；批次執行屬 T3。
pub fn run_dry_run(args: &crate::cli::RerollArgs) -> Result<()> {
    let root = match &args.path {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    let rolls = find_rolls(&root);
    // 計畫層即需圖譜：db 開不起來整批 bail（design §6）
    let tracker = AssetTracker::open()?;
    let plan = build_plan(&rolls, args.stale_only, &tracker)?;

    let reroll_count = plan.iter().filter(|it| it.stale).count();
    for item in &plan {
        println!(
            "{} | {:?} | {} | {}",
            item.roll.display(),
            item.engine,
            item.output.display(),
            if item.stale { "stale" } else { "keep" },
        );
    }
    println!(
        "{} 支：{} 重錄、{} skip",
        plan.len(),
        reroll_count,
        plan.len() - reroll_count
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-reroll-{tag}-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_roll(dir: &Path, rel: &str, output: &str) -> PathBuf {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            format!("Set Engine VHS\nSet Output \"{output}\"\nType \"hi\"\n"),
        )
        .unwrap();
        p
    }

    // ── find_rolls ──

    #[test]
    fn find_rolls_sorted_and_only_roll() {
        let dir = tmpdir("sorted");
        write_roll(&dir, "b/dir/x.roll", "o.webm");
        write_roll(&dir, "a.roll", "o.webm");
        write_roll(&dir, "c/notes.txt", "not a roll");
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/skip.roll"), "Set Engine VHS\n").unwrap();

        let found = find_rolls(&dir);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.roll", "b/dir/x.roll"]);
    }

    #[test]
    fn find_rolls_missing_root_empty() {
        assert!(find_rolls(Path::new("/nonexistent/tapedeck-test")).is_empty());
    }

    // ── build_plan：stale 三態 ──
    fn tmp_db(tag: &str) -> AssetTracker {
        let dir = tmpdir(tag);
        AssetTracker::open_at(&dir.join("tapedeck.db")).unwrap()
    }

    /// 推進/回推檔案 mtime，delta 秒（正＝未來、負＝過去）
    fn shift_mtime(path: &Path, delta_secs: i64) {
        let t = std::fs::metadata(path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let target = if delta_secs >= 0 {
            SystemTime::UNIX_EPOCH + t + Duration::from_secs(delta_secs as u64)
        } else {
            SystemTime::UNIX_EPOCH + t - Duration::from_secs((-delta_secs) as u64)
        };
        fs::File::options()
            .append(true)
            .open(path)
            .unwrap()
            .set_modified(target)
            .unwrap();
    }

    #[test]
    fn plan_without_stale_only_includes_all() {
        let dir = tmpdir("all");
        let a = write_roll(&dir, "a.roll", "a.webm");
        let tracker = tmp_db("all");
        // 已登錄且資產較新 → 未帶 --stale-only 仍全數 include
        let asset = dir.join("assets/a.webm");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, b"x").unwrap();
        shift_mtime(&asset, 3600); // 資產在未來 → 比 roll 新
        tracker
            .register(&asset, Some(a.to_string_lossy().as_ref()))
            .unwrap();

        let plan = build_plan(std::slice::from_ref(&a), false, &tracker).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(plan[0].stale);
        assert_eq!(plan[0].roll, a);
        assert_eq!(plan[0].engine, crate::engine::roll_parser::Engine::Vhs);
    }

    #[test]
    fn plan_stale_only_unregistered_is_stale() {
        let dir = tmpdir("unreg");
        let a = write_roll(&dir, "a.roll", "a.webm");
        let tracker = tmp_db("unreg");

        let plan = build_plan(&[a], true, &tracker).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(plan[0].stale);
    }

    #[test]
    fn plan_stale_only_roll_newer_than_asset_is_stale() {
        let dir = tmpdir("newer");
        let a = write_roll(&dir, "a.roll", "a.webm");
        let tracker = tmp_db("newer");
        // 資產登錄在過去 → 已登錄，但 roll 剛被改過 → 比資產新 → stale
        let asset = dir.join("assets/a.webm");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, b"x").unwrap();
        shift_mtime(&asset, -7200); // 資產在 2 小時前，roll 現在 = 比資產新
        tracker
            .register(&asset, Some(a.to_string_lossy().as_ref()))
            .unwrap();

        let plan = build_plan(&[a], true, &tracker).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(plan[0].stale);
    }

    #[test]
    fn plan_stale_only_asset_newer_is_kept() {
        let dir = tmpdir("older");
        let a = write_roll(&dir, "a.roll", "a.webm");
        let tracker = tmp_db("older");
        // 資產檔先存在，register 才算得到 hash；登錄後 roll mtime 已是過去 → 資產較新 → keep
        let asset = dir.join("assets/a.webm");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, b"x").unwrap();
        tracker
            .register(&asset, Some(a.to_string_lossy().as_ref()))
            .unwrap();

        let plan = build_plan(&[a], true, &tracker).unwrap();
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].stale); // keep
    }

    #[test]
    fn plan_output_path_from_script_and_default() {
        let dir = tmpdir("out");
        let custom = write_roll(&dir, "custom.roll", "assets/custom.webm");
        let default_out = write_roll(&dir, "default.roll", "");

        let tracker = tmp_db("out");
        let plan = build_plan(&[custom, default_out], false, &tracker).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(plan[0].output.ends_with("tapedeck/assets/custom.webm"));
        assert!(plan[1].output.ends_with("tapedeck/output.webm"));
    }
}
