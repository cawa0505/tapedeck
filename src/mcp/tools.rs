//! MCP 工具層（T1 骨架 → T2 接真實執行路徑）
//!
//! T1：tools/list 回六工具 metadata；tools/call 分派存在但 handler 回未實作。
//! T2 將以 dispatcher / doctor / db / media 取代 stub。

use serde_json::{json, Value};

/// tools/list 用的工具 metadata
#[derive(serde::Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// tools/call 執行錯誤
pub enum ToolError {
    /// 未知工具（→ JSON-RPC -32602）
    Unknown(String),
    /// 執行失敗（→ result.isError:true，session 持續可用）
    Execution(String),
}

/// 六工具清單（design.md 定案：全做）
pub fn list() -> Vec<Tool> {
    vec![
        Tool {
            name: "tapedeck_run",
            description: "執行 .roll 錄製腳本並錄製輸出（webm/gif/webp/png）；可回傳最後一幀 PNG 供視覺驗證（T3）。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": ".roll 腳本內容" },
                    "output": { "type": "string", "description": "輸出檔名（預設依腳本或 XDG）" },
                    "max_size": { "type": "integer", "description": "輸出大小上限 MB（超過自動壓縮）" },
                    "humanize": { "type": "boolean", "description": "人類節奏優化（預設 false）" },
                    "append_signature": { "type": "boolean", "description": "附推廣標籤（預設 false）" }
                },
                "required": ["script"]
            }),
        },
        Tool {
            name: "tapedeck_inspect_environment",
            description: "封裝 tapedeck doctor：回報輸入後端（uinput/wtype）、依賴工具、硬體能力摘要。",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "tapedeck_extract_frames",
            description: "從錄製檔按 timestamp / 均勻抽樣擷取 PNG 影格，供視覺模型審查。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "media": { "type": "string", "description": "輸入媒體檔路徑" },
                    "timestamps": { "type": "array", "items": { "type": "number" }, "description": "指定時間點（秒）" },
                    "count": { "type": "integer", "description": "均勻抽樣數量（未給 timestamps 時）" }
                },
                "required": ["media"]
            }),
        },
        Tool {
            name: "tapedeck_link",
            description: "連結媒體資產到 SQLite 資產圖譜（sha256 + mtime），或查詢既有紀錄。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "media_file": { "type": "string", "description": "媒體檔路徑" },
                    "format": { "type": "string", "description": "輸出格式 md/zola/html（預設 md）" }
                },
                "required": ["media_file"]
            }),
        },
        Tool {
            name: "tapedeck_optimize",
            description: "壓製 GIF/WebP 體積（palettegen 雙 Pass / libwebp）。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "輸入檔路徑" },
                    "format": { "type": "string", "description": "gif/webp（預設依副檔名）" },
                    "quality": { "type": "integer", "description": "品質 1-100（webp，預設 80）" },
                    "fps": { "type": "integer", "description": "抽樣 fps（gif，預設 10）" }
                },
                "required": ["input"]
            }),
        },
        Tool {
            name: "tapedeck_clean",
            description: "清理 SQLite 中的孤兒/失效資產（無 .md 引用的錄製檔）。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dry_run": { "type": "boolean", "description": "只列出不刪除（預設 true）" }
                }
            }),
        },
    ]
}

/// 執行工具呼叫（T1：全部回未實作；T2 接真實路徑）
pub fn execute(name: &str, arguments: Value) -> Result<Value, ToolError> {
    let known = list().iter().any(|t| t.name == name);
    if !known {
        return Err(ToolError::Unknown(name.to_string()));
    }
    let _ = arguments;
    Err(ToolError::Execution(format!(
        "{name} 尚未實作（MCP T2 接線真實執行路徑）"
    )))
}
