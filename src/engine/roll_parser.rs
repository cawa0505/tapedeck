use anyhow::{anyhow, Result};
use std::path::Path;

/// 引擎選擇（REQ-1.1 / REQ-4.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Auto,
    Vhs,
    Native,
}

/// 腳本指令的枚舉（僅執行層指令；metadata 存於 Script）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptCommand {
    /// 輸入文字
    Type(String),
    /// 具名按鍵 + 次數（`Key Down 3`，預設 1）；單一字母如 `Key q`
    Key(String, u32),
    /// 睡眠（毫秒）
    Sleep(u64),
    /// 滑鼠移動（speed 參數僅記錄、不影響行為，見 Non-Goals）
    MouseMove(i32, i32),
    /// 滑鼠點擊
    Click(ClickType),
    /// 錄製前執行（失敗即中止）
    ExecBefore(String),
    /// 錄製後執行（失敗僅警告）
    ExecAfter(String),
    /// 等待視窗出現（title, timeout_ms，預設 10000）
    WaitWindow(String, u64),
    /// 指定錄製目標視窗
    TargetWindow(String),
    /// 錄製視窗尺寸
    WindowSize(u32, u32),
    /// 視窗幾何外擴像素
    Padding(u32),
    /// 錄製時長（秒）
    Roll(u64),
    /// 送出組合鍵（GUI 模式）
    Shortcut(String),
    /// 錄製後優化（codec, key=value 清單）
    Optimize(String, Vec<(String, String)>),
    /// vhs 指令全集透寫（REQ-7.1，原樣轉譯至 .tape）
    Vhs(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickType {
    Left,
    Right,
    Middle,
}

/// 腳本結構
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub title: Option<String>,
    pub engine: Option<Engine>,
    pub shell: Option<String>,
    pub output: Option<String>,
    pub fps: Option<u32>,
    pub commands: Vec<ScriptCommand>,
}

/// vhs 指令全集（REQ-7.1）：首詞匹配即透寫原行
const VHS_PASSTHROUGH: &[&str] = &[
    "Require",
    "Ctrl",
    "Alt",
    "Escape",
    "Space",
    "Backspace",
    "Delete",
    "Insert",
    "Down",
    "Left",
    "Right",
    "Tab",
    "Up",
    "PageUp",
    "PageDown",
    "ScrollUp",
    "ScrollDown",
    "Hide",
    "Show",
    "Wait",
    "Source",
    "Screenshot",
    "Copy",
    "Paste",
    "MouseClick",
];

/// 具名按鍵（REQ-1.2）
const NAMED_KEYS: &[&str] = &[
    "Down",
    "Up",
    "Enter",
    "Tab",
    "Left",
    "Right",
    "PageUp",
    "PageDown",
    "Space",
    "Backspace",
    "Delete",
    "Insert",
    "Escape",
    "Home",
    "End",
];

/// 解析 .roll 腳本
pub fn parse_roll_script(path: &Path) -> Result<Script> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("無法讀取腳本 {}: {}", path.display(), e))?;
    parse_roll_content(&content).map_err(|e| anyhow!("{}: {}", path.display(), e))
}

/// 從字串解析 .roll 內容（測試與 CLI 共用）
pub fn parse_roll_content(content: &str) -> Result<Script> {
    let mut script = Script {
        title: None,
        engine: None,
        shell: None,
        output: None,
        fps: None,
        commands: Vec::new(),
    };

    for (line_num, raw) in content.lines().enumerate() {
        let line = strip_comment(raw.trim());
        if line.is_empty() {
            continue;
        }
        let line_no = line_num + 1;

        // 首詞分派
        let (first, rest) = match line.split_once(char::is_whitespace) {
            Some((f, r)) => (f, r.trim()),
            None => (line, ""),
        };

        match first {
            "Set" => parse_set(&mut script, rest, line_no)?,
            "Title" => {
                script.title = Some(parse_quoted(rest, "Title", line_no)?);
            }
            "Mode" => {
                // 舊寫法別名（REQ-2.2）：TUI ⇒ Auto、GUI ⇒ Native
                script.engine = Some(match rest {
                    "TUI" => Engine::Auto,
                    "GUI" => Engine::Native,
                    _ => {
                        return Err(anyhow!(
                            "第 {} 行：無效的模式 '{}'，應為 TUI 或 GUI",
                            line_no,
                            rest
                        ))
                    }
                });
            }
            // 舊寫法別名（REQ-2.3 ~ 2.5）
            "Output" => {
                script.output = Some(parse_quoted(rest, "Output", line_no)?);
            }
            "FPS" => {
                script.fps = Some(parse_num(rest, "FPS", line_no)?);
            }
            "Terminal" => {
                script.shell = Some(parse_quoted(rest, "Terminal", line_no)?);
            }
            "Type" => {
                script
                    .commands
                    .push(ScriptCommand::Type(parse_quoted(rest, "Type", line_no)?));
            }
            "Enter" => {
                // 舊寫法別名（REQ-2.6）：視為 Key Enter
                let n = parse_opt_count(rest, "Enter", line_no)?;
                script.commands.push(ScriptCommand::Key("Enter".into(), n));
            }
            "Key" => parse_key(&mut script, rest, line_no)?,
            "Sleep" => {
                script
                    .commands
                    .push(ScriptCommand::Sleep(parse_sleep(rest, "Sleep", line_no)?));
            }
            "MouseMove" => {
                // MouseMove <x> <y> [speed=smooth]（speed 僅記錄）
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() < 2 || parts.len() > 3 {
                    return Err(anyhow!(
                        "第 {} 行：MouseMove 格式錯誤，應為 MouseMove X Y [speed=smooth]",
                        line_no
                    ));
                }
                let x: i32 = parts[0]
                    .parse()
                    .map_err(|_| anyhow!("第 {} 行：MouseMove X 必須是數字", line_no))?;
                let y: i32 = parts[1]
                    .parse()
                    .map_err(|_| anyhow!("第 {} 行：MouseMove Y 必須是數字", line_no))?;
                script.commands.push(ScriptCommand::MouseMove(x, y));
            }
            "Click" => {
                let t = match rest {
                    "Left" => ClickType::Left,
                    "Right" => ClickType::Right,
                    "Middle" => ClickType::Middle,
                    _ => {
                        return Err(anyhow!(
                            "第 {} 行：無效的點擊類型 '{}'，應為 Left, Right 或 Middle",
                            line_no,
                            rest
                        ))
                    }
                };
                script.commands.push(ScriptCommand::Click(t));
            }
            "ExecBefore" => {
                script.commands.push(ScriptCommand::ExecBefore(parse_quoted(
                    rest,
                    "ExecBefore",
                    line_no,
                )?));
            }
            "ExecAfter" => {
                script.commands.push(ScriptCommand::ExecAfter(parse_quoted(
                    rest,
                    "ExecAfter",
                    line_no,
                )?));
            }
            "WaitWindow" => {
                // WaitWindow "<title>" [timeout=<Ns|Nms>]
                let (title, timeout) = parse_wait_window(rest, line_no)?;
                script
                    .commands
                    .push(ScriptCommand::WaitWindow(title, timeout));
            }
            "TargetWindow" => {
                script
                    .commands
                    .push(ScriptCommand::TargetWindow(parse_quoted(
                        rest,
                        "TargetWindow",
                        line_no,
                    )?));
            }
            "WindowSize" => {
                let (w, h) = parse_two_nums(rest, "WindowSize", line_no)?;
                script.commands.push(ScriptCommand::WindowSize(w, h));
            }
            "Padding" => {
                script
                    .commands
                    .push(ScriptCommand::Padding(parse_num(rest, "Padding", line_no)?));
            }
            "Roll" => {
                let s = parse_sleep_seconds(rest, "Roll", line_no)?;
                script.commands.push(ScriptCommand::Roll(s));
            }
            "Shortcut" => {
                script.commands.push(ScriptCommand::Shortcut(parse_quoted(
                    rest, "Shortcut", line_no,
                )?));
            }
            "Optimize" => {
                let (codec, kv) = parse_optimize(rest, line_no)?;
                script.commands.push(ScriptCommand::Optimize(codec, kv));
            }
            _ => {
                // vhs 指令全集透寫（REQ-7.1）
                if is_vhs_passthrough(first) {
                    script.commands.push(ScriptCommand::Vhs(line.to_string()));
                } else {
                    return Err(anyhow!(
                        "第 {} 行：未知的指令 '{}'，支援的指令：Set Engine/Output/FPS/Shell, Title, Mode, Output, FPS, Terminal, Type, Enter, Key, Sleep, MouseMove, Click, ExecBefore, ExecAfter, WaitWindow, TargetWindow, WindowSize, Padding, Roll, Shortcut, Optimize, 及 vhs 指令全集（Require/Ctrl/Alt+/Escape/Space/Backspace/Delete/Insert/Down/Left/Right/Tab/Up/PageUp/PageDown/ScrollUp/ScrollDown/Hide/Show/Wait/Source/Screenshot/Copy/Paste）",
                        line_no, first
                    ));
                }
            }
        }
    }

    Ok(script)
}

/// 剝離行尾註釋（引號內的 `#` 保留）：`Key q  # 退出` → `Key q`
fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return line[..i].trim(),
            _ => {}
        }
    }
    line
}

/// 首詞是否為 vhs 透寫指令（REQ-7.1）；Ctrl/Alt 組合鍵為 `Ctrl+L` 前綴形式
fn is_vhs_passthrough(first: &str) -> bool {
    VHS_PASSTHROUGH.contains(&first) || first.starts_with("Ctrl+") || first.starts_with("Alt+")
}

/// Set 系列分派：`Set Engine|Output|FPS|Shell`（REQ-1.1），其他 Set 透寫為 vhs
fn parse_set(script: &mut Script, rest: &str, line_no: usize) -> Result<()> {
    let (setting, value) = match rest.split_once(char::is_whitespace) {
        Some((s, v)) => (s, v.trim()),
        None => (rest, ""),
    };
    match setting {
        "Engine" => {
            script.engine = Some(match value {
                "Auto" => Engine::Auto,
                "VHS" => Engine::Vhs,
                "Native" => Engine::Native,
                _ => {
                    return Err(anyhow!(
                        "第 {} 行：無效的引擎 '{}'，應為 Auto, VHS 或 Native",
                        line_no,
                        value
                    ))
                }
            });
        }
        "Output" => {
            script.output = Some(parse_quoted(value, "Set Output", line_no)?);
        }
        "FPS" => {
            script.fps = Some(parse_num(value, "Set FPS", line_no)?);
        }
        "Shell" => {
            script.shell = Some(parse_quoted(value, "Set Shell", line_no)?);
        }
        // 其他 vhs Set 選項（Framerate/Theme/Width/...）原樣透寫
        _ => {
            script
                .commands
                .push(ScriptCommand::Vhs(format!("Set {}", rest)));
        }
    }
    Ok(())
}

/// `Key <name> [count]`（REQ-1.2）：具名按鍵或單一字母
fn parse_key(script: &mut Script, rest: &str, line_no: usize) -> Result<()> {
    let (name, count_part) = match rest.split_once(char::is_whitespace) {
        Some((n, c)) => (n, Some(c.trim())),
        None => (rest, None),
    };

    let is_named = NAMED_KEYS.contains(&name);
    let is_single_char = name.chars().count() == 1;
    if !is_named && !is_single_char {
        return Err(anyhow!(
            "第 {} 行：無效的按鍵 '{}'，應為 {} 或單一字母",
            line_no,
            name,
            NAMED_KEYS.join("/")
        ));
    }

    let n = match count_part {
        Some(c) => c
            .parse::<u32>()
            .map_err(|_| anyhow!("第 {} 行：Key {} 的次數必須是數字", line_no, name))?,
        None => 1,
    };
    script
        .commands
        .push(ScriptCommand::Key(name.to_string(), n));
    Ok(())
}

/// WaitWindow "<title>" [timeout=<Ns|Nms>]，timeout 預設 10000ms
fn parse_wait_window(rest: &str, line_no: usize) -> Result<(String, u64)> {
    let mut parts = rest.split_whitespace();
    let title = parts.next().ok_or_else(|| {
        anyhow!(
            "第 {} 行：WaitWindow 格式錯誤，應為 WaitWindow \"<title>\" [timeout=...]",
            line_no
        )
    })?;
    let title = title
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| anyhow!("第 {} 行：WaitWindow 格式錯誤，title 需以引號包住", line_no))?
        .to_string();

    let mut timeout = 10000u64;
    for opt in parts {
        if let Some(v) = opt.strip_prefix("timeout=") {
            timeout = parse_sleep(v, "WaitWindow timeout", line_no)?;
        }
    }
    Ok((title, timeout))
}

/// `Optimize <codec> [key=value...]`（REQ-7.4）
fn parse_optimize(rest: &str, line_no: usize) -> Result<(String, Vec<(String, String)>)> {
    let mut parts = rest.split_whitespace();
    let codec = parts
        .next()
        .ok_or_else(|| {
            anyhow!(
                "第 {} 行：Optimize 格式錯誤，應為 Optimize <codec> [key=value...]",
                line_no
            )
        })?
        .to_string();
    let mut kv = Vec::new();
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            kv.push((k.to_string(), v.to_string()));
        } else {
            return Err(anyhow!(
                "第 {} 行：Optimize 參數格式錯誤 '{}'，應為 key=value",
                line_no,
                p
            ));
        }
    }
    Ok((codec, kv))
}

/// 帶引號字串解析：`"..."` 或 `...`
fn parse_quoted(rest: &str, cmd: &str, line_no: usize) -> Result<String> {
    if rest.is_empty() {
        return Err(anyhow!(
            "第 {} 行：{} 格式錯誤，應為 {} \"...\"",
            line_no,
            cmd,
            cmd
        ));
    }
    if let Some(s) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Ok(s.to_string())
    } else {
        Ok(rest.to_string())
    }
}

/// 數字解析
fn parse_num(rest: &str, cmd: &str, line_no: usize) -> Result<u32> {
    rest.trim()
        .parse()
        .map_err(|_| anyhow!("第 {} 行：{} 必須是數字", line_no, cmd))
}

/// 兩個數字解析（WindowSize）
fn parse_two_nums(rest: &str, cmd: &str, line_no: usize) -> Result<(u32, u32)> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "第 {} 行：{} 格式錯誤，應為 {} W H",
            line_no,
            cmd,
            cmd
        ));
    }
    let a: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow!("第 {} 行：{} 必須是數字", line_no, cmd))?;
    let b: u32 = parts[1]
        .parse()
        .map_err(|_| anyhow!("第 {} 行：{} 必須是數字", line_no, cmd))?;
    Ok((a, b))
}

/// 時間解析：支援 ms / s 後綴，預設毫秒
fn parse_sleep(rest: &str, cmd: &str, line_no: usize) -> Result<u64> {
    let (value, mult) = if let Some(v) = rest.strip_suffix("ms") {
        (v, 1u64)
    } else if let Some(v) = rest.strip_suffix('s') {
        (v, 1000u64)
    } else {
        (rest, 1u64)
    };
    let n: u64 = value
        .parse()
        .map_err(|_| anyhow!("第 {} 行：{} 必須是數字 ms 或 s", line_no, cmd))?;
    Ok(n * mult)
}

/// 秒數解析（Roll 用，可選 s 後綴）
fn parse_sleep_seconds(rest: &str, cmd: &str, line_no: usize) -> Result<u64> {
    let v = rest.strip_suffix('s').unwrap_or(rest);
    v.trim()
        .parse()
        .map_err(|_| anyhow!("第 {} 行：{} 必須是數字 s", line_no, cmd))
}

/// 可選計數（Enter [count]），預設 1
fn parse_opt_count(rest: &str, cmd: &str, line_no: usize) -> Result<u32> {
    if rest.is_empty() {
        return Ok(1);
    }
    rest.parse()
        .map_err(|_| anyhow!("第 {} 行：{} 的次數必須是數字", line_no, cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-3.1：三份 examples 全部解析成功
    #[test]
    fn parse_all_examples() {
        for name in ["test_tui.roll", "tui_zago.roll", "gui_demo.roll"] {
            let path = Path::new("examples").join(name);
            let script =
                parse_roll_script(&path).unwrap_or_else(|e| panic!("{} 解析失敗: {}", name, e));
            assert!(!script.commands.is_empty(), "{} 無指令", name);
        }
    }

    /// test_tui.roll：舊寫法別名（REQ-2）
    #[test]
    fn test_tui_legacy_aliases() {
        let s = parse_roll_script(Path::new("examples/test_tui.roll")).unwrap();
        assert_eq!(s.title.as_deref(), Some("TUI Test"));
        assert_eq!(s.engine, Some(Engine::Auto)); // Mode TUI ⇒ Auto
        assert_eq!(s.output.as_deref(), Some("test_tui.gif"));
        assert_eq!(s.fps, Some(15));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Enter".into(), 1)));
    }

    /// tui_zago.roll：Set 系列 + Key 具名按鍵（REQ-1.1/1.2）
    #[test]
    fn tui_zago_set_and_key() {
        let s = parse_roll_script(Path::new("examples/tui_zago.roll")).unwrap();
        assert_eq!(s.engine, Some(Engine::Auto));
        assert_eq!(s.output.as_deref(), Some("assets/zago_tui_test.webm"));
        assert_eq!(s.fps, Some(30));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Down".into(), 1)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Enter".into(), 1)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Tab".into(), 1)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("q".into(), 1)));
    }

    /// gui_demo.roll：自動化指令全解析（REQ-1.3）
    #[test]
    fn gui_demo_automation() {
        let s = parse_roll_script(Path::new("examples/gui_demo.roll")).unwrap();
        assert_eq!(s.engine, Some(Engine::Native));
        assert_eq!(s.title.as_deref(), Some("Obsidian Automation"));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::ExecBefore("obsidian".into())));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::WaitWindow("Obsidian".into(), 10000)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::TargetWindow("Obsidian".into())));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::WindowSize(1200, 800)));
        assert!(s.commands.iter().any(|c| *c == ScriptCommand::Padding(20)));
        assert!(s.commands.iter().any(|c| *c == ScriptCommand::Roll(15)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::MouseMove(500, 300)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Click(ClickType::Left)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Shortcut("Ctrl+S".into())));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::ExecAfter("pkill obsidian".into())));
        assert!(s.commands.iter().any(|c| {
            matches!(c, ScriptCommand::Optimize(codec, kv) if codec == "AV1" && kv == &vec![("encoder".to_string(), "av1_vaapi".to_string())])
        }));
    }

    /// REQ-7.1：vhs 指令全集透寫
    #[test]
    fn vhs_passthrough_full_set() {
        let content = r#"
Set Engine VHS
Output "t.webm"
Require ffmpeg
Set Framerate 30
Set Theme "rose-pine"
Type "hi"
Ctrl+L
Alt+f
Escape
Space 3
Backspace 2
Delete 1
Insert 1
Down 3
Left 2
Right 1
Tab 1
Up 1
PageUp 1
PageDown 1
ScrollUp 1
ScrollDown 1
Hide
Show
Wait 2s
Wait /Hello/
Source other.tape
Screenshot shot.png
Copy "text"
Paste
MouseClick left
"#;
        let s = parse_roll_content(content).unwrap();
        let passthrough: Vec<&str> = s
            .commands
            .iter()
            .filter_map(|c| match c {
                ScriptCommand::Vhs(line) => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            passthrough,
            vec![
                "Require ffmpeg",
                "Set Framerate 30",
                "Set Theme \"rose-pine\"",
                "Ctrl+L",
                "Alt+f",
                "Escape",
                "Space 3",
                "Backspace 2",
                "Delete 1",
                "Insert 1",
                "Down 3",
                "Left 2",
                "Right 1",
                "Tab 1",
                "Up 1",
                "PageUp 1",
                "PageDown 1",
                "ScrollUp 1",
                "ScrollDown 1",
                "Hide",
                "Show",
                "Wait 2s",
                "Wait /Hello/",
                "Source other.tape",
                "Screenshot shot.png",
                "Copy \"text\"",
                "Paste",
                "MouseClick left",
            ],
            "透寫指令數量不符"
        );
    }

    /// WaitWindow timeout 解析（Ns/Nms/預設）
    #[test]
    fn wait_window_timeout() {
        let s = parse_roll_content(
            r#"
WaitWindow "App" timeout=5s
WaitWindow "App2" timeout=250ms
WaitWindow "App3"
"#,
        )
        .unwrap();
        let cmds: Vec<_> = s
            .commands
            .iter()
            .filter_map(|c| match c {
                ScriptCommand::WaitWindow(t, ms) => Some((t.as_str(), *ms)),
                _ => None,
            })
            .collect();
        assert_eq!(cmds, vec![("App", 5000), ("App2", 250), ("App3", 10000)]);
    }

    /// Key 計數與單一字母
    #[test]
    fn key_count_and_char() {
        let s = parse_roll_content("Key Down 3\nKey q\nKey Enter\nEnter 2\n").unwrap();
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Down".into(), 3)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("q".into(), 1)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Enter".into(), 1)));
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::Key("Enter".into(), 2)));
    }

    /// 未知指令報錯（REQ-3.2）
    #[test]
    fn unknown_command_errors() {
        let err = parse_roll_content("Bogus 123").unwrap_err();
        assert!(
            err.to_string().contains("未知的指令"),
            "錯誤訊息應列出支援清單: {}",
            err
        );
    }

    /// Optimize 格式（REQ-7.4）
    #[test]
    fn optimize_kv() {
        let s = parse_roll_content("Optimize VP9 crf=32 b:v=0\n").unwrap();
        assert!(s.commands.iter().any(|c| {
            matches!(c, ScriptCommand::Optimize(codec, kv) if codec == "VP9" && *kv == vec![
                ("crf".to_string(), "32".to_string()),
                ("b:v".to_string(), "0".to_string()),
            ])
        }));
    }

    /// MouseMove speed 參數僅記錄（Non-Goals）
    #[test]
    fn mousemove_speed_ignored() {
        let s = parse_roll_content("MouseMove 100 200 speed=smooth\n").unwrap();
        assert!(s
            .commands
            .iter()
            .any(|c| *c == ScriptCommand::MouseMove(100, 200)));
    }
}
