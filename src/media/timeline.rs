//! timeline — 操作時間點日誌（P1 media-export / design.md 4）
//!
//! Native 後端的時間點來源：走訪 .roll 指令、累加 `Sleep` 時長推算
//! 每個 `Click`/`Type` 的相對起錄時間（ms）。OQ-02 輸入注入未接線前，
//! 以腳本時序推算為準；注入實作後再改為實際執行當下記錄。
//!
//! JSONL 格式：每行 `{"ms":2340,"command":"Click Left"}`（filmstrip 以
//! `ffmpeg -ss <ms>` 抽幀）。

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::engine::roll_parser::{ClickType, Script, ScriptCommand};

/// 單一操作時間點
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelinePoint {
    /// 相對起錄 0 基準（毫秒）
    pub ms: u64,
    /// 操作描述："Click Left" / "Type \"hi\""
    pub command: String,
}

/// 走訪指令、累加 Sleep，推算每個操作點的相對時間（0 基準）
pub fn compute_timeline(script: &Script) -> Vec<TimelinePoint> {
    let mut ms = 0u64;
    let mut points = Vec::new();
    for cmd in &script.commands {
        match cmd {
            ScriptCommand::Sleep(secs) => ms += secs.saturating_mul(1000),
            ScriptCommand::Click(btn) => {
                let name = match btn {
                    ClickType::Left => "Left",
                    ClickType::Right => "Right",
                    ClickType::Middle => "Middle",
                };
                points.push(TimelinePoint {
                    ms,
                    command: format!("Click {name}"),
                });
            }
            ScriptCommand::Type(text) => points.push(TimelinePoint {
                ms,
                command: format!("Type \"{text}\""),
            }),
            _ => {} // 其餘指令不影響時間軸（WaitWindow 輪詢時間無法精確推算）
        }
    }
    points
}

/// 寫出 JSONL（每行一個 TimelinePoint）
pub fn write_jsonl(path: &Path, points: &[TimelinePoint]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("建立日誌目錄失敗: {}", parent.display()))?;
    }
    let mut out = String::new();
    for p in points {
        out.push_str(&serde_json::to_string(p)?);
        out.push('\n');
    }
    std::fs::write(path, out).with_context(|| format!("寫入日誌失敗: {}", path.display()))
}

/// 讀取 JSONL（filmstrip 用）
// ponytail: 消費者（filmstrip T4）未實作，先保留 API
#[allow(dead_code)]
pub fn read_jsonl(path: &Path) -> Result<Vec<TimelinePoint>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("讀取日誌失敗: {}", path.display()))?;
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .with_context(|| format!("解析日誌列失敗: {l}（檔案 {}）", path.display()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn script_with(cmds: Vec<ScriptCommand>) -> Script {
        Script {
            title: None,
            engine: None,
            shell: None,
            output: None,
            fps: None,
            commands: cmds,
        }
    }

    #[test]
    fn timeline_accumulates_sleep_before_each_action() {
        let script = script_with(vec![
            ScriptCommand::Sleep(2),
            ScriptCommand::Type("hello".to_owned()),
            ScriptCommand::Sleep(1),
            ScriptCommand::Click(ClickType::Left),
        ]);
        let points = compute_timeline(&script);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].ms, 2000);
        assert_eq!(points[0].command, "Type \"hello\"");
        assert_eq!(points[1].ms, 3000);
        assert_eq!(points[1].command, "Click Left");
    }

    #[test]
    fn timeline_ignores_non_clock_commands() {
        let script = script_with(vec![
            ScriptCommand::Sleep(1),
            ScriptCommand::Click(ClickType::Right),
            ScriptCommand::Type("q".to_owned()),
        ]);
        let points = compute_timeline(&script);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].ms, 1000);
        assert_eq!(points[0].command, "Click Right");
        assert_eq!(points[1].ms, 1000);
        assert_eq!(points[1].command, "Type \"q\"");
    }

    #[test]
    fn jsonl_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-timeline-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("test.timeline.jsonl");
        let points = vec![
            TimelinePoint {
                ms: 2340,
                command: "Click Left".to_owned(),
            },
            TimelinePoint {
                ms: 5300,
                command: "Type \"hi\"".to_owned(),
            },
        ];
        write_jsonl(&path, &points).unwrap();
        let back = read_jsonl(&path).unwrap();
        assert_eq!(back, points);
        // 驗證格式：每行單一 JSON 物件
        let raw = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("{\"ms\":2340,\"command\":\"Click Left\"}"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
