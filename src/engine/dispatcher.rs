use crate::cli::RunArgs;
use crate::engine::roll_parser::{ClickType, Engine, Script, ScriptCommand};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;
use tokio::time::sleep;

// ─────────────────────────────────────────────
// XDG 路徑解析（REQ-6）
// ─────────────────────────────────────────────

/// `$XDG_*_HOME/<sub>` 或 `$HOME/<fallback>/<sub>`
fn xdg_dir(var: &str, fallback: &str, sub: &str) -> PathBuf {
    let base = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(fallback))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
    });
    base.join(sub)
}

/// 輸出快取目錄（REQ-6.1）
fn cache_dir() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache", "tapedeck")
}

/// 輸出路徑解析（REQ-6.1）：CLI 覆寫/絕對路徑照原樣；相對 → XDG cache
fn resolve_output_path(script_output: &str, cli_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = cli_override {
        return Ok(p.to_path_buf()); // CLI 顯式覆寫，照原樣
    }
    let p = Path::new(script_output);
    if p.is_absolute() {
        return Ok(p.to_path_buf()); // 絕對路徑照原樣
    }
    Ok(cache_dir().join(p))
}

// ─────────────────────────────────────────────
// 錄影引擎抽象層（OQ-03 定案）
// ─────────────────────────────────────────────

/// 引擎 lifecycle：prepare（ExecBefore）→ record（錄製）→ cleanup（ExecAfter/優化）
#[async_trait]
pub trait RecordingEngine {
    async fn prepare(&self, script: &Script) -> Result<()>;
    async fn record(&self, script: &Script) -> Result<()>;
    async fn cleanup(&self, script: &Script) -> Result<()>;
}

// ─────────────────────────────────────────────
// 共用 helper
// ─────────────────────────────────────────────

async fn run_exec_cmds(script: &Script, before: bool, fail_fast: bool) -> Result<()> {
    for cmd in &script.commands {
        let exec = match (before, cmd) {
            (true, ScriptCommand::ExecBefore(c)) => c,
            (false, ScriptCommand::ExecAfter(c)) => c,
            _ => continue,
        };
        let status = TokioCommand::new("sh")
            .arg("-c")
            .arg(exec)
            .status()
            .await
            .with_context(|| format!("failed to start sh -c: {exec}"))?;
        if !status.success() {
            if fail_fast {
                bail!("ExecBefore 失敗（exit {status}）：{exec}");
            }
            eprintln!("警告：ExecAfter 失敗（exit {status}）：{exec}");
        }
    }
    Ok(())
}

/// 從 commands 找出第一個相符指令
fn find_cmd(script: &Script, pred: fn(&ScriptCommand) -> bool) -> Option<&ScriptCommand> {
    script.commands.iter().find(|c| pred(c))
}

// ─────────────────────────────────────────────
// vhs 後端（TUI）：.roll → .tape → vhs
// ─────────────────────────────────────────────

pub struct VhsEngine {
    output: PathBuf,
}

impl VhsEngine {
    pub fn new(output: PathBuf) -> Self {
        Self { output }
    }
}

#[async_trait]
impl RecordingEngine for VhsEngine {
    async fn prepare(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, true, true).await
    }

    async fn record(&self, script: &Script) -> Result<()> {
        let tape_content = script_to_tape_content(script, &self.output)?;

        // 建立暫存 .tape 檔案
        let tape_path = std::env::temp_dir().join(format!("tapedeck-{}.tape", std::process::id()));
        std::fs::write(&tape_path, tape_content)
            .with_context(|| format!("無法寫入暫存 .tape 檔案: {}", tape_path.display()))?;

        // 呼叫 vhs
        let executable = std::env::var("VHS_BIN").unwrap_or_else(|_| "vhs".to_owned());
        let status = TokioCommand::new(&executable)
            .arg(&tape_path)
            .status()
            .await
            .with_context(|| format!("failed to start {executable}; install VHS or set VHS_BIN"))?;

        // 清理暫存檔案
        let _ = std::fs::remove_file(&tape_path);

        if !status.success() {
            bail!("VHS exited with {status}");
        }
        Ok(())
    }

    async fn cleanup(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, false, false).await
    }
}

/// 將 .roll 腳本轉換為 VHS 可理解的 .tape 內容（VHS DSL）
fn script_to_tape_content(script: &Script, output: &Path) -> Result<String> {
    let mut s = String::new();

    writeln!(s, "Output \"{}\"", output.display())?;
    if let Some(fps) = script.fps {
        writeln!(s, "Set Framerate {}", fps)?;
    }
    if let Some(term) = &script.shell {
        writeln!(s, "Set Shell \"{}\"", term)?;
    }

    for cmd in &script.commands {
        match cmd {
            ScriptCommand::Type(text) => writeln!(s, "Type \"{}\"", text)?,
            ScriptCommand::Key(name, count) => {
                let is_single_char = name.chars().count() == 1
                    && !matches!(
                        name.as_str(),
                        "Down"
                            | "Up"
                            | "Enter"
                            | "Tab"
                            | "Left"
                            | "Right"
                            | "PageUp"
                            | "PageDown"
                            | "Space"
                            | "Backspace"
                            | "Delete"
                            | "Insert"
                            | "Escape"
                            | "Home"
                            | "End"
                    );
                if is_single_char {
                    // 單一字母 → Type "q"（vhs 無單鍵指令）
                    for _ in 0..*count {
                        writeln!(s, "Type \"{}\"", name)?;
                    }
                } else if *count > 1 {
                    writeln!(s, "{} {}", name, count)?;
                } else {
                    writeln!(s, "{}", name)?;
                }
            }
            ScriptCommand::Sleep(ms) => writeln!(s, "Sleep {}ms", ms)?,
            ScriptCommand::MouseMove(x, y) => writeln!(s, "MouseMove {} {}", x, y)?,
            ScriptCommand::Click(t) => {
                let btn = match t {
                    ClickType::Left => "left",
                    ClickType::Right => "right",
                    ClickType::Middle => "middle",
                };
                writeln!(s, "MouseClick {}", btn)?;
            }
            ScriptCommand::Roll(secs) => writeln!(s, "Sleep {}s", secs)?,
            // vhs 指令全集透寫（REQ-7.1）：原樣轉譯
            ScriptCommand::Vhs(line) => writeln!(s, "{}", line)?,
            // tapedeck 自動化層指令（VHS 無對應）直接略過
            _ => {}
        }
    }

    Ok(s)
}

// ─────────────────────────────────────────────
// Native 後端（GUI）：compositor + wf-recorder
// ─────────────────────────────────────────────

pub struct NativeEngine {
    output: PathBuf,
}

impl NativeEngine {
    pub fn new(output: PathBuf) -> Self {
        Self { output }
    }
}

#[async_trait]
impl RecordingEngine for NativeEngine {
    async fn prepare(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, true, true).await
    }

    async fn record(&self, script: &Script) -> Result<()> {
        use crate::engine::wayland::compositor::detect_compositor;

        // 取得目標視窗幾何；compositor（非 Send）僅在 block 內存活，取得後即 drop
        let geometry_arg = {
            let compositor = detect_compositor()?;

            // TargetWindow / WaitWindow 目標（預設空字串 → 互動選擇）
            let target = match find_cmd(script, |c| matches!(c, ScriptCommand::TargetWindow(_))) {
                Some(ScriptCommand::TargetWindow(name)) => name.as_str(),
                _ => match find_cmd(script, |c| matches!(c, ScriptCommand::WaitWindow(..))) {
                    Some(ScriptCommand::WaitWindow(name, _)) => name.as_str(),
                    _ => "",
                },
            };

            // WaitWindow：每 200ms 輪詢直到成功或逾時（預設 10s）
            // 同步 sleep（CLI 單執行緒場景可接受），避免非 Send 的 compositor 跨 await
            let timeout_ms = match find_cmd(script, |c| matches!(c, ScriptCommand::WaitWindow(..)))
            {
                Some(ScriptCommand::WaitWindow(_, ms)) => *ms,
                _ => 10_000,
            };
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let geo = loop {
                match compositor.find_window_geometry(target) {
                    Ok(g) => break g,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(_) => {
                        bail!(
                            "WaitWindow 逾時：視窗「{target}」未出現\
                             \n提示：可用 `tapedeck run --dry-run` 檢查，或先手動開啟目標視窗"
                        );
                    }
                }
            };

            // Padding → 幾何外擴
            let padding = match find_cmd(script, |c| matches!(c, ScriptCommand::Padding(_))) {
                Some(ScriptCommand::Padding(p)) => *p,
                _ => 0,
            };
            geo.to_wf_recorder_arg(padding)
        };

        // WindowSize：目前僅記錄，不調整視窗（OQ-02 待實作 resize）
        if let Some(ScriptCommand::WindowSize(w, h)) =
            find_cmd(script, |c| matches!(c, ScriptCommand::WindowSize(..)))
        {
            eprintln!("警告：WindowSize {w}x{h} 僅記錄，不調整視窗大小（OQ-02 待實作）");
        }

        // Shortcut：OQ-02 輸入注入未實作 → 警告略過
        if let Some(ScriptCommand::Shortcut(_)) =
            find_cmd(script, |c| matches!(c, ScriptCommand::Shortcut(_)))
        {
            eprintln!("警告：Shortcut 未實作（OQ-02 wtype/libei 輸入注入待接線）");
        }

        // wf-recorder + Roll 計時
        let executable = std::env::var("WF_RECORDER").unwrap_or_else(|_| "wf-recorder".to_owned());
        let mut child = TokioCommand::new(&executable)
            .arg("-g")
            .arg(&geometry_arg)
            .arg("-f")
            .arg(&self.output)
            .spawn()
            .with_context(|| {
                format!("failed to start {executable}; install wf-recorder or set WF_RECORDER")
            })?;

        let status = match find_cmd(script, |c| matches!(c, ScriptCommand::Roll(_))) {
            Some(ScriptCommand::Roll(secs)) => {
                sleep(Duration::from_secs(*secs)).await;
                let _ = child.kill().await;
                child.wait().await
            }
            _ => child.wait().await,
        }?;

        if !status.success() {
            bail!("wf-recorder exited with {status}");
        }
        Ok(())
    }

    async fn cleanup(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, false, false).await?;

        // Optimize(codec, kv) → ffmpeg 轉換（如 AV1 vaapi → av1_vaapi）
        if let Some(ScriptCommand::Optimize(codec, kv)) =
            find_cmd(script, |c| matches!(c, ScriptCommand::Optimize(..)))
        {
            let encoder = kv
                .iter()
                .find(|(k, _)| k == "encoder")
                .map(|(_, v)| v.as_str())
                .unwrap_or(codec);
            let stem = self
                .output
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "output".to_owned());
            let ext = self
                .output
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "webm".to_owned());
            let optimized = self
                .output
                .with_file_name(format!("{stem}_optimized.{ext}"));

            let status = TokioCommand::new("ffmpeg")
                .arg("-y")
                .arg("-i")
                .arg(&self.output)
                .arg("-c:v")
                .arg(encoder)
                .arg(&optimized)
                .status()
                .await
                .with_context(|| "failed to start ffmpeg; install ffmpeg")?;
            if !status.success() {
                eprintln!("警告：Optimize 失敗（exit {status}），保留原始檔案");
            } else {
                println!("優化完成：{}", optimized.display());
            }
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────
// 引擎解析 + CLI 入口
// ─────────────────────────────────────────────

/// 解析 .roll 的引擎設定（REQ-4.3）：Auto → 依腳本意圖自動選擇
fn resolve_engine(script: &Script) -> Engine {
    match script.engine {
        Some(Engine::Vhs) => Engine::Vhs,
        Some(Engine::Native) => Engine::Native,
        Some(Engine::Auto) | None => {
            let gui_intent = script.commands.iter().any(|c| {
                matches!(
                    c,
                    ScriptCommand::WaitWindow(..)
                        | ScriptCommand::TargetWindow(_)
                        | ScriptCommand::WindowSize(..)
                        | ScriptCommand::Padding(_)
                        | ScriptCommand::Shortcut(_)
                        | ScriptCommand::MouseMove(..)
                        | ScriptCommand::Click(_)
                        | ScriptCommand::Optimize(..)
                )
            });
            if gui_intent {
                Engine::Native
            } else {
                Engine::Vhs
            }
        }
    }
}

/// CLI 入口：解析 .roll 腳本並依引擎分派
pub async fn run(args: RunArgs) -> Result<()> {
    let script = crate::engine::roll_parser::parse_roll_script(&args.script_file)?;
    let engine = resolve_engine(&script);
    let output = resolve_output_path(
        script.output.as_deref().unwrap_or("output.webm"),
        args.output.as_deref(),
    )?;

    if args.dry_run {
        // REQ-5 + REQ-6.3：印出引擎/輸出（解析後絕對路徑）/fps/指令摘要
        println!(
            "dry-run: {} engine={:?} output={} fps={:?} commands={}",
            args.script_file.display(),
            engine,
            output.display(),
            script.fps,
            script.commands.len()
        );
        return Ok(());
    }

    // REQ-6.2：錄製前建立輸出目錄
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("無法建立輸出路徑: {}", parent.display()))?;
    }

    let backend: Box<dyn RecordingEngine> = match engine {
        Engine::Vhs => Box::new(VhsEngine::new(output.clone())),
        Engine::Native => Box::new(NativeEngine::new(output)),
        Engine::Auto => unreachable!("resolve_engine 已解析 Auto"),
    };

    backend.prepare(&script).await?;
    backend.record(&script).await?;
    backend.cleanup(&script).await?;
    Ok(())
}

/// 產生媒體連結語法 (zola/md/html)
pub fn media_link(media_file: &Path, format: &str) -> Result<String> {
    let name = media_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let display = media_file.display();

    let link = match format {
        "md" | "markdown" => format!("[{}]({})", name, display),
        "html" => format!("<img src=\"{}\" alt=\"{}\" />", display, name),
        "zola" => format!("{{{{ figure(src=\"{}\", alt=\"{}\") }}}}", display, name),
        _ => bail!("不支援的格式 '{}'，支援：md, html, zola", format),
    };
    Ok(link)
}

// ─────────────────────────────────────────────
// 單元測試
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_script() -> Script {
        Script {
            title: None,
            engine: None,
            shell: None,
            output: Some("assets/demo.webm".to_owned()),
            fps: Some(15),
            commands: vec![
                ScriptCommand::Type("hello".to_owned()),
                ScriptCommand::Key("q".to_owned(), 1),
                ScriptCommand::Key("Down".to_owned(), 3),
                ScriptCommand::Sleep(500),
                ScriptCommand::MouseMove(100, 200),
                ScriptCommand::Click(ClickType::Left),
                ScriptCommand::Roll(2),
                ScriptCommand::Vhs("Set Theme \"Dracula\"".to_owned()),
                ScriptCommand::ExecBefore("echo before".to_owned()),
                ScriptCommand::ExecAfter("echo after".to_owned()),
            ],
        }
    }

    // ── VHS 轉譯正確性（不實際呼叫 vhs）──

    #[test]
    fn tape_translation_basic() {
        let s = sample_script();
        let out = Path::new("/tmp/out/demo.webm");
        let tape = script_to_tape_content(&s, out).unwrap();

        assert!(tape.contains("Output \"/tmp/out/demo.webm\""));
        assert!(tape.contains("Set Framerate 15"));
        assert!(tape.contains("Type \"hello\""));
        assert!(tape.contains("Type \"q\""));
        assert!(tape.contains("Down 3"));
        assert!(tape.contains("Sleep 500ms"));
        assert!(tape.contains("MouseMove 100 200"));
        assert!(tape.contains("MouseClick left"));
        assert!(tape.contains("Sleep 2s"));
        assert!(tape.contains("Set Theme \"Dracula\""));
        // 自動化層指令不進 .tape
        assert!(!tape.contains("ExecBefore"));
        assert!(!tape.contains("ExecAfter"));
        assert!(!tape.contains("assets/demo.webm"));
    }

    #[test]
    fn tape_translation_key_count() {
        let s = Script {
            commands: vec![ScriptCommand::Key("Enter".to_owned(), 1)],
            ..sample_script()
        };
        let tape = script_to_tape_content(&s, Path::new("/tmp/o.webm")).unwrap();
        assert!(tape.contains("Enter"));
    }

    #[test]
    fn tape_translation_named_key_not_single_char() {
        let s = Script {
            commands: vec![ScriptCommand::Key("Enter".to_owned(), 2)],
            ..sample_script()
        };
        let tape = script_to_tape_content(&s, Path::new("/tmp/o.webm")).unwrap();
        assert!(tape.contains("Enter 2"));
    }

    // ── resolve_output_path（REQ-6）──

    /// 環境變數測試需序列化執行（std::env 全域）
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn output_path_relative_uses_xdg_cache() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-cache");
        std::env::set_var("HOME", "/home/user");
        let p = resolve_output_path("assets/demo.webm", None).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/xdg-cache/tapedeck/assets/demo.webm"));
    }

    #[test]
    fn output_path_xdg_unset_uses_home_fallback() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::set_var("HOME", "/home/user");
        let p = resolve_output_path("assets/demo.webm", None).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/home/user/.cache/tapedeck/assets/demo.webm")
        );
    }

    #[test]
    fn output_path_absolute_kept() {
        let p = resolve_output_path("/var/tmp/demo.webm", None).unwrap();
        assert_eq!(p, PathBuf::from("/var/tmp/demo.webm"));
    }

    #[test]
    fn output_path_cli_override_wins() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-cache");
        let p =
            resolve_output_path("assets/demo.webm", Some(Path::new("/cli/override.webm"))).unwrap();
        assert_eq!(p, PathBuf::from("/cli/override.webm"));
    }

    // ── resolve_engine（REQ-4.3 Auto）──

    #[test]
    fn engine_auto_tui_script_goes_vhs() {
        let s = Script {
            engine: Some(Engine::Auto),
            commands: vec![ScriptCommand::Type("hi".to_owned())],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Vhs);
    }

    #[test]
    fn engine_auto_gui_script_goes_native() {
        let s = Script {
            engine: Some(Engine::Auto),
            commands: vec![ScriptCommand::WaitWindow("Obsidian".to_owned(), 10_000)],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Native);
    }

    #[test]
    fn engine_explicit_wins() {
        let s = Script {
            engine: Some(Engine::Native),
            commands: vec![ScriptCommand::Type("hi".to_owned())],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Native);
    }
}
