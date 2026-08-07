use crate::cli::RunArgs;
use crate::engine::roll_parser::{Engine, Script, ScriptCommand};
use anyhow::{anyhow, bail, Context, Result};
use std::fmt::Write as _;
use std::path::Path;
use tokio::process::Command as TokioCommand;

/// 將 .roll 腳本轉換為 VHS 可理解的 .tape 內容 (VHS DSL)
fn script_to_tape_content(script: &Script) -> Result<String> {
    let mut output = String::new();

    if let Some(out) = &script.output {
        writeln!(output, "Output \"{}\"", out)?;
    }
    if let Some(fps) = script.fps {
        writeln!(output, "Set Framerate {}", fps)?;
    }
    if let Some(term) = &script.shell {
        writeln!(output, "Set Shell \"{}\"", term)?;
    }

    for cmd in &script.commands {
        match cmd {
            ScriptCommand::Type(text) => writeln!(output, "Type \"{}\"", text)?,
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
                        writeln!(output, "Type \"{}\"", name)?;
                    }
                } else if *count > 1 {
                    writeln!(output, "{} {}", name, count)?;
                } else {
                    writeln!(output, "{}", name)?;
                }
            }
            ScriptCommand::Sleep(ms) => writeln!(output, "Sleep {}ms", ms)?,
            ScriptCommand::MouseMove(x, y) => writeln!(output, "MouseMove {} {}", x, y)?,
            ScriptCommand::Click(t) => {
                let btn = match t {
                    crate::engine::roll_parser::ClickType::Left => "left",
                    crate::engine::roll_parser::ClickType::Right => "right",
                    crate::engine::roll_parser::ClickType::Middle => "middle",
                };
                writeln!(output, "MouseClick {}", btn)?;
            }
            ScriptCommand::Roll(s) => writeln!(output, "Sleep {}s", s)?,
            // vhs 指令全集透寫（REQ-7.1）：原樣轉譯
            ScriptCommand::Vhs(line) => writeln!(output, "{}", line)?,
            // tapedeck 自動化層指令（VHS 無對應）直接略過
            _ => {}
        }
    }

    Ok(output)
}

/// 執行 TUI 模式：生成 .tape 檔案並呼叫 vhs
pub async fn run_tui(script: &Script) -> Result<()> {
    let tape_content = script_to_tape_content(script)?;

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

/// 執行 GUI 模式：使用 compositor 找到視窗並呼叫 wf-recorder
pub async fn run_gui(script: &Script) -> Result<()> {
    use crate::engine::wayland::compositor::detect_compositor;

    let compositor = detect_compositor()?;

    // 從 script 中取得目標視窗名稱（預設為空字串，表示使用互動選擇）
    let target = script
        .commands
        .iter()
        .find_map(|cmd| {
            if let ScriptCommand::TargetWindow(name) = cmd {
                Some(name.as_str())
            } else {
                None
            }
        })
        .unwrap_or("");

    // 取得視窗幾何座標
    let geo = compositor.find_window_geometry(target)?;

    // 取得輸出路徑
    let output_path = script
        .output
        .as_ref()
        .ok_or_else(|| anyhow!("腳本缺少 Output 指令"))?;

    // 建立 wf-recorder 參數
    let padding: u32 = 0; // TODO: 可從腳本中讀取 padding
    let geometry_arg = geo.to_wf_recorder_arg(padding);

    // 呼叫 wf-recorder
    let executable = std::env::var("WF_RECORDER").unwrap_or_else(|_| "wf-recorder".to_owned());
    let status = TokioCommand::new(&executable)
        .arg("-g")
        .arg(&geometry_arg)
        .arg("-f")
        .arg(output_path)
        .status()
        .await
        .with_context(|| {
            format!("failed to start {executable}; install wf-recorder or set WF_RECORDER")
        })?;

    if !status.success() {
        bail!("wf-recorder exited with {status}");
    }

    Ok(())
}

/// CLI 入口：解析 .roll 腳本並依引擎分派
pub async fn run(args: RunArgs) -> Result<()> {
    let script = crate::engine::roll_parser::parse_roll_script(&args.script_file)?;

    if args.dry_run {
        let backend = match script.engine {
            Some(Engine::Vhs) => "vhs",
            Some(Engine::Native) => "wf-recorder",
            Some(Engine::Auto) | None => "auto",
        };
        println!(
            "dry-run: {} engine={:?} backend={} output={:?} fps={:?}",
            args.script_file.display(),
            script.engine,
            backend,
            script.output,
            script.fps
        );
        return Ok(());
    }

    match script.engine {
        Some(Engine::Vhs) | Some(Engine::Auto) | None => run_tui(&script).await,
        Some(Engine::Native) => run_gui(&script).await,
    }
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
