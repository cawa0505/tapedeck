use anyhow::{anyhow, Result};
use std::path::Path;

/// 腳本指令的枚舉
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptCommand {
    Title(String),
    Mode(Mode),
    Terminal(String),
    Output(String),
    FPS(u32),
    Type(String),
    Enter,
    KeyDown(u32),
    KeyUp(u32),
    Sleep(u64), // 毫秒
    MouseMove(i32, i32),
    Click(ClickType),
    ExecBefore(String),
    ExecAfter(String),
    WaitPort(u16, u64), // port, timeout_ms
    TargetWindow(String),
    Roll(u64), // 秒
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    TUI,
    GUI,
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
    pub mode: Option<Mode>,
    pub terminal: Option<String>,
    pub output: Option<String>,
    pub fps: Option<u32>,
    pub commands: Vec<ScriptCommand>,
}

/// 解析 .roll 腳本
pub fn parse_roll_script(path: &Path) -> Result<Script> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("無法讀取腳本 {}: {}", path.display(), e))?;

    let mut script = Script {
        title: None,
        mode: None,
        terminal: None,
        output: None,
        fps: None,
        commands: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 無參數指令
        if line == "Enter" {
            script.commands.push(ScriptCommand::Enter);
            continue;
        }

        match line.split_once(' ') {
            Some((cmd, rest)) => {
                match cmd {
                    "Title" => {
                        if let Some(title) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.title = Some(title.to_string());
                        } else {
                            return Err(anyhow!("第 {} 行：Title 格式錯誤，應為 Title \"...\"", line_num + 1));
                        }
                    }
                    "Mode" => {
                        match rest {
                            "TUI" => script.mode = Some(Mode::TUI),
                            "GUI" => script.mode = Some(Mode::GUI),
                            _ => return Err(anyhow!("第 {} 行：無效的模式 '{}', 應為 TUI 或 GUI", line_num + 1, rest)),
                        }
                    }
                    "Terminal" => {
                        if let Some(term) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.terminal = Some(term.to_string());
                        } else {
                            return Err(anyhow!("第 {} 行：Terminal 格式錯誤，應為 Terminal \"...\"", line_num + 1));
                        }
                    }
                    "Output" => {
                        if let Some(output) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.output = Some(output.to_string());
                        } else {
                            return Err(anyhow!("第 {} 行：Output 格式錯誤，應為 Output \"...\"", line_num + 1));
                        }
                    }
                    "FPS" => {
                        match rest.parse::<u32>() {
                            Ok(fps) => script.fps = Some(fps),
                            Err(_) => return Err(anyhow!("第 {} 行：FPS 必須是數字", line_num + 1)),
                        }
                    }
                    "Type" => {
                        if let Some(text) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.commands.push(ScriptCommand::Type(text.to_string()));
                        } else {
                            return Err(anyhow!("第 {} 行：Type 格式錯誤，應為 Type \"...\"", line_num + 1));
                        }
                    }
                    "Enter" => {
                        script.commands.push(ScriptCommand::Enter);
                    }
                    "Key Down" => {
                        match rest.parse::<u32>() {
                            Ok(n) => script.commands.push(ScriptCommand::KeyDown(n)),
                            Err(_) => return Err(anyhow!("第 {} 行：Key Down 必須是數字", line_num + 1)),
                        }
                    }
                    "Key Up" => {
                        match rest.parse::<u32>() {
                            Ok(n) => script.commands.push(ScriptCommand::KeyUp(n)),
                            Err(_) => return Err(anyhow!("第 {} 行：Key Up 必須是數字", line_num + 1)),
                        }
                    }
                    "Sleep" => {
                        let (value, mult) = if let Some(v) = rest.strip_suffix("ms") {
                            (v, 1u64)
                        } else if let Some(v) = rest.strip_suffix('s') {
                            (v, 1000u64)
                        } else {
                            (rest, 1u64)
                        };
                        match value.parse::<u64>() {
                            Ok(n) => script.commands.push(ScriptCommand::Sleep(n * mult)),
                            Err(_) => return Err(anyhow!("第 {} 行：Sleep 必須是數字 ms 或 s", line_num + 1)),
                        }
                    }
                    "MouseMove" => {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() == 2 {
                            match (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                                (Ok(x), Ok(y)) => script.commands.push(ScriptCommand::MouseMove(x, y)),
                                _ => return Err(anyhow!("第 {} 行：MouseMove 格式錯誤，應為 MouseMove X Y", line_num + 1)),
                            }
                        } else {
                            return Err(anyhow!("第 {} 行：MouseMove 格式錯誤，應為 MouseMove X Y", line_num + 1));
                        }
                    }
                    "Click" => {
                        match rest {
                            "Left" => script.commands.push(ScriptCommand::Click(ClickType::Left)),
                            "Right" => script.commands.push(ScriptCommand::Click(ClickType::Right)),
                            "Middle" => script.commands.push(ScriptCommand::Click(ClickType::Middle)),
                            _ => return Err(anyhow!("第 {} 行：無效的點擊類型 '{}', 應為 Left, Right 或 Middle", line_num + 1, rest)),
                        }
                    }
                    "Exec Before" => {
                        if let Some(cmd) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.commands.push(ScriptCommand::ExecBefore(cmd.to_string()));
                        } else {
                            return Err(anyhow!("第 {} 行：Exec Before 格式錯誤，應為 Exec Before \"...\"", line_num + 1));
                        }
                    }
                    "Exec After" => {
                        if let Some(cmd) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.commands.push(ScriptCommand::ExecAfter(cmd.to_string()));
                        } else {
                            return Err(anyhow!("第 {} 行：Exec After 格式錯誤，應為 Exec After \"...\"", line_num + 1));
                        }
                    }
                    "Wait Port" => {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() == 2 {
                            match (parts[0].parse::<u16>(), parts[1].split('=').last().unwrap_or(&parts[1]).parse::<u64>()) {
                                (Ok(port), Ok(timeout)) => script.commands.push(ScriptCommand::WaitPort(port, timeout)),
                                _ => return Err(anyhow!("第 {} 行：Wait Port 格式錯誤，應為 Wait Port PORT timeout=TIMEOUT", line_num + 1)),
                            }
                        } else {
                            return Err(anyhow!("第 {} 行：Wait Port 格式錯誤，應為 Wait Port PORT timeout=TIMEOUT", line_num + 1));
                        }
                    }
                    "Target Window" => {
                        if let Some(window) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                            script.commands.push(ScriptCommand::TargetWindow(window.to_string()));
                        } else {
                            return Err(anyhow!("第 {} 行：Target Window 格式錯誤，應為 Target Window \"...\"", line_num + 1));
                        }
                    }
                    "Roll" => {
                        match rest.strip_suffix('s').unwrap_or(&rest).parse::<u64>() {
                            Ok(seconds) => script.commands.push(ScriptCommand::Roll(seconds)),
                            Err(_) => return Err(anyhow!("第 {} 行：Roll 必須是數字 s", line_num + 1)),
                        }
                    }
                    _ => {
                        return Err(anyhow!("第 {} 行：未知的指令 '{}', 支援的指令：Title, Mode, Terminal, Output, FPS, Type, Enter, Key Down, Key Up, Sleep, MouseMove, Click, Exec Before, Exec After, Wait Port, Target Window, Roll", line_num + 1, cmd));
                    }
                }
            }
            None => {
                return Err(anyhow!("第 {} 行：指令格式錯誤，應為 '指令 參數'", line_num + 1));
            }
        }
    }

    Ok(script)
}

/// 執行腳本
pub async fn execute_script(script: Script) -> Result<()> {
    use tokio::process::Command;
    use std::time::Duration;

    // 設置輸出路徑
    let output_path = script.output.as_ref().ok_or_else(|| anyhow!("腳本缺少 Output 指令"))?;

    // 根據模式選擇後端
    match script.mode {
        Some(Mode::TUI) => {
            // TUI 模式：執行 TUI 錄製
            // TODO: 實作 TUI 錄製邏輯
            println!("TUI 模式：將執行 TUI 錄製，輸出到 {}", output_path);
        }
        Some(Mode::GUI) => {
            // GUI 模式：執行 GUI 錄製
            // TODO: 實作 GUI 錄製邏輯
            println!("GUI 模式：將執行 GUI 錄製，輸出到 {}", output_path);
        }
        None => {
            return Err(anyhow!("腳本缺少 Mode 指令"));
        }
    }

    // 執行腳本中的指令
    for cmd in script.commands {
        match cmd {
            ScriptCommand::Type(text) => {
                // TODO: 實作輸入文字邏輯
                println!("輸入文字：{}", text);
            }
            ScriptCommand::Enter => {
                // TODO: 實作 Enter 鍵邏輯
                println!("按下 Enter 鍵");
            }
            ScriptCommand::KeyDown(n) => {
                // TODO: 實作向下鍵邏輯
                println!("按下向下鍵 {} 次", n);
            }
            ScriptCommand::KeyUp(n) => {
                // TODO: 實作向上鍵邏輯
                println!("按上向上鍵 {} 次", n);
            }
            ScriptCommand::Sleep(ms) => {
                tokio::time::sleep(Duration::from_millis(ms)).await;
            }
            ScriptCommand::MouseMove(x, y) => {
                // TODO: 實作滑鼠移動邏輯
                println!("滑鼠移動至 ({}, {})", x, y);
            }
            ScriptCommand::Click(click_type) => {
                // TODO: 實作點擊邏輯
                let click_str = match click_type {
                    ClickType::Left => "左鍵",
                    ClickType::Right => "右鍵",
                    ClickType::Middle => "中鍵",
                };
                println!("點擊 {} 鍵", click_str);
            }
            ScriptCommand::ExecBefore(cmd) => {
                // TODO: 實作前置指令邏輯
                println!("執行前置指令：{}", cmd);
                let status = Command::new("sh").arg("-c").arg(&cmd).status().await?;
                if !status.success() {
                    return Err(anyhow!("前置指令執行失敗：{}", cmd));
                }
            }
            ScriptCommand::ExecAfter(cmd) => {
                // TODO: 實作後置指令邏輯
                println!("執行後置指令：{}", cmd);
                let status = Command::new("sh").arg("-c").arg(&cmd).status().await?;
                if !status.success() {
                    return Err(anyhow!("後置指令執行失敗：{}", cmd));
                }
            }
            ScriptCommand::WaitPort(port, timeout) => {
                // TODO: 實作等待 Port 邏輯
                println!("等待 Port {}，超時 {} 毫秒", port, timeout);
                // 這裡需要實現真正的 Port 等待邏輯
            }
            ScriptCommand::TargetWindow(window) => {
                // TODO: 實作目標視窗邏輯
                println!("鎖定目標視窗：{}", window);
            }
            ScriptCommand::Roll(seconds) => {
                // TODO: 實作錄製邏輯
                println!("錄製 {} 秒", seconds);
            }
            _ => {
                // 忽略其他指令（如 Title, Terminal, FPS）
            }
        }
    }

    Ok(())
}