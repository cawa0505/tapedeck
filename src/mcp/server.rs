//! MCP stdio 伺服器骨架（T1）
//!
//! 協議：JSON-RPC 2.0 over stdio，newline-delimited JSON（每行一則訊息）。
//! 細節以 docs/ref/mcp-stdio-protocol.md（官方 2025-06-18）為準。

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::tools;

/// MCP 協議版本（2025-06-18 官方 spec）
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// 伺服器狀態：initialize 完成前只接受 initialize / ping / notifications/initialized
#[derive(Default)]
pub struct ServerState {
    initialized: bool,
}

/// 單則 JSON-RPC 訊息（request / notification 共用；response 由我們產生）
#[derive(Deserialize)]
struct RpcMessage {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

/// stdio 迴圈：讀 stdin 逐行 → handle_message → 寫 stdout。
/// async 化：在既有 tokio runtime 內執行（main 是 #[tokio::main]），
/// tools/call 可直接 await dispatcher 的 async run，避免巢狀 runtime panic。
pub async fn serve() -> anyhow::Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    let mut out = stdout;
    let mut state = ServerState::default();

    let mut lines = stdin.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_message(&line, &mut state).await {
            tokio::io::AsyncWriteExt::write_all(&mut out, response.as_bytes()).await?;
            tokio::io::AsyncWriteExt::write_all(&mut out, b"\n").await?;
            tokio::io::AsyncWriteExt::flush(&mut out).await?;
        }
    }
    Ok(())
}

/// 處理單則訊息，回傳要寫出的 response（notification 回 None）。
/// async：tools/call 需 await tools::execute（dispatcher run 是 async）。
pub async fn handle_message(line: &str, state: &mut ServerState) -> Option<String> {
    let msg: RpcMessage = match serde_json::from_str(line) {
        Ok(m) => m,
        Err(e) => {
            return Some(response_error(
                Value::Null,
                -32700,
                &format!("Parse error: {e}"),
            ));
        }
    };

    let method = match msg.method {
        Some(m) => m,
        None => {
            // 沒有 method 也不是 request → 忽略
            return None;
        }
    };
    let id = msg.id.clone().unwrap_or(Value::Null);
    let is_request = !msg.id.is_none();

    // initialize 完成前：只放行 initialize / ping / notifications/initialized
    if !state.initialized
        && !matches!(
            method.as_str(),
            "initialize" | "ping" | "notifications/initialized"
        )
    {
        return Some(response_error(
            id,
            -32000,
            "Server not initialized: call initialize first",
        ));
    }

    match method.as_str() {
        "initialize" => {
            if !is_request {
                return None;
            }
            // 版本協商：回顯客戶端送的版本（官方 spec：回自己支援的版）
            let requested = msg
                .params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_VERSION);
            let negotiated = if requested == PROTOCOL_VERSION {
                requested
            } else {
                PROTOCOL_VERSION
            };
            state.initialized = true;
            Some(response_result(
                id,
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "tapedeck", "version": env!("CARGO_PKG_VERSION") }
                }),
            ))
        }
        "notifications/initialized" => None,
        "ping" => {
            if !is_request {
                return None;
            }
            Some(response_result(id, json!({})))
        }
        "tools/list" => {
            if !is_request {
                return None;
            }
            Some(response_result(id, json!({ "tools": tools::list() })))
        }
        "tools/call" => {
            if !is_request {
                return None;
            }
            let name = msg
                .params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = msg
                .params
                .as_ref()
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            match tools::execute(name, arguments).await {
                Ok(result) => {
                    // MCP tools/call 規範：result 必須含 content 陣列 + isError；
                    // 結構化欄位（record_id/frames/...）一併保留供 client 讀取
                    let text = result
                        .get("message")
                        .and_then(|m| m.as_str())
                        .or_else(|| result.get("report").and_then(|r| r.as_str()))
                        .unwrap_or("ok")
                        .to_string();
                    let mut wrapped = json!({
                        "content": [ { "type": "text", "text": text } ],
                        "isError": false,
                    });
                    if let Some(obj) = result.as_object() {
                        for (k, v) in obj {
                            if k != "message" && k != "report" {
                                wrapped[k.as_str()] = v.clone();
                            }
                        }
                    }
                    Some(response_result(id, wrapped))
                }
                Err(tools::ToolError::Unknown(name)) => {
                    Some(response_error(id, -32602, &format!("Unknown tool: {name}")))
                }
                Err(tools::ToolError::Execution(msg)) => {
                    // tool 執行失敗 → isError:true，session 持續可用
                    Some(response_result(
                        id,
                        json!({
                            "content": [ { "type": "text", "text": msg } ],
                            "isError": true
                        }),
                    ))
                }
            }
        }
        other => Some(response_error(
            id,
            -32601,
            &format!("Method not found: {other}"),
        )),
    }
}

fn response_result(id: Value, result: Value) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        .unwrap_or_else(|_| "{}".into())
}

fn response_error(id: Value, code: i64, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    }))
    .unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn handle(line: &str, state: &mut ServerState) -> Value {
        let out = handle_message(line, state).await.expect("應有 response");
        serde_json::from_str(&out).unwrap()
    }

    #[tokio::test]
    async fn initialize_handshake_returns_negotiated_version() {
        let mut s = ServerState::default();
        let resp = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"test","version":"0.1"}}}"#,
            &mut s,
        )
        .await;
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "tapedeck");
        assert!(s.initialized);
    }

    #[tokio::test]
    async fn initialized_notification_returns_none() {
        let mut s = ServerState::default();
        let _ = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"t","version":"0"}}}"#,
            &mut s,
        )
        .await;
        assert!(handle_message(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &mut s,
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn tools_list_returns_six_tools() {
        let mut s = ServerState::default();
        let _ = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"t","version":"0"}}}"#,
            &mut s,
        )
        .await;
        let resp = handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &mut s).await;
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"tapedeck_run"));
        assert!(names.contains(&"tapedeck_clean"));
    }

    #[tokio::test]
    async fn ping_returns_empty_result() {
        let mut s = ServerState::default();
        let _ = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"t","version":"0"}}}"#,
            &mut s,
        )
        .await;
        let resp = handle(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, &mut s).await;
        assert_eq!(resp["result"], json!({}));
    }

    #[tokio::test]
    async fn unknown_method_returns_32601() {
        let mut s = ServerState::default();
        let _ = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"t","version":"0"}}}"#,
            &mut s,
        )
        .await;
        let resp = handle(r#"{"jsonrpc":"2.0","id":4,"method":"bogus"}"#, &mut s).await;
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn request_before_initialize_is_rejected() {
        let mut s = ServerState::default();
        let resp = handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &mut s).await;
        assert_eq!(resp["error"]["code"], -32000);
    }
}
