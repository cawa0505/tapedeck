//! MCP stdio 協議整合測試（T4）：spawn 真實 binary、喂 newline-delimited JSON、
//! 驗證握手 / 工具清單 / 錯誤路徑 / 協定串流純淨。
//!
//! - 無 `#[ignore]` 的測試不依賴外部工具（vhs/ffmpeg），可在任何環境跑。
//! - `test_run_visual_loop` 需真實 vhs 錄製（`#[ignore]`，本機手動跑）。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

/// 啟動 `tapedeck mcp`，回傳 child + 輸入/輸出端。
fn spawn_mcp() -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tapedeck"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tapedeck mcp");
    let stdin = child.stdin.take().expect("child stdin");
    let stdout = child.stdout.take().expect("child stdout");
    (child, stdin, BufReader::new(stdout))
}

/// 送一則訊息（request 或 notification），讀取下一行回應（notification 回 None）。
fn round_trip(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    msg: &Value,
) -> Option<Value> {
    let mut line = serde_json::to_string(msg).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).expect("write stdin");
    stdin.flush().expect("flush stdin");
    let mut buf = String::new();
    if reader.read_line(&mut buf).expect("read stdout") == 0 {
        return None;
    }
    let buf = buf.trim();
    if buf.is_empty() {
        return None;
    }
    Some(serde_json::from_str(buf).expect("stdout 行必須是合法 JSON"))
}

fn init(stdin: &mut ChildStdin, reader: &mut BufReader<ChildStdout>) -> Value {
    let resp = round_trip(
        stdin,
        reader,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "t4-test", "version": "1"},
            },
        }),
    )
    .expect("initialize 回應");
    assert_eq!(resp["id"], 1, "id 回顯");
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(resp["result"]["serverInfo"]["name"], "tapedeck");
    assert!(resp["result"]["capabilities"]["tools"].is_object());
    // initialized notification（無回應 — 只寫不讀）
    let mut nl =
        serde_json::to_string(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .unwrap();
    nl.push('\n');
    stdin.write_all(nl.as_bytes()).expect("write initialized");
    stdin.flush().expect("flush initialized");
    resp
}

#[test]
fn handshake_tools_list_and_error_paths() {
    let (mut child, mut stdin, mut reader) = spawn_mcp();

    // 1. initialize 握手
    init(&mut stdin, &mut reader);

    // 2. tools/list → 六工具
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .expect("tools/list 回應");
    let tools = resp["result"]["tools"].as_array().expect("tools 陣列");
    assert_eq!(tools.len(), 6, "六工具");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "tapedeck_run",
        "tapedeck_inspect_environment",
        "tapedeck_extract_frames",
        "tapedeck_link",
        "tapedeck_optimize",
        "tapedeck_clean",
    ] {
        assert!(names.contains(&expected), "缺少工具 {expected}");
    }
    // 每工具都有 inputSchema
    for t in tools {
        assert!(
            t["inputSchema"].is_object(),
            "inputSchema 缺失: {}",
            t["name"]
        );
    }

    // 3. 未知 method → -32601
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "foo/bar"}),
    )
    .expect("未知 method 回應");
    assert_eq!(resp["error"]["code"], -32601);

    // 4. tools/call 未知工具 → -32602
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "no_such_tool", "arguments": {}}}),
    )
    .expect("未知工具回應");
    assert_eq!(resp["error"]["code"], -32602);

    // 5. tools/call 缺參數 → isError（tapedeck_link 缺 media_file）
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                "params": {"name": "tapedeck_link", "arguments": {}}}),
    )
    .expect("缺參數回應");
    assert_eq!(resp["result"]["isError"], true, "缺參數應 isError");
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("缺少參數"),
        "錯誤訊息: {}",
        resp["result"]["content"][0]["text"]
    );

    // 6. stdin 關閉 → 正常退出
    drop(stdin);
    let status = child.wait().expect("wait");
    assert!(status.success(), "EOF 後正常退出");
}

#[test]
fn inspect_environment_reports_text() {
    let (mut child, mut stdin, mut reader) = spawn_mcp();
    init(&mut stdin, &mut reader);
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "tapedeck_inspect_environment", "arguments": {}}}),
    )
    .expect("inspect_environment 回應");
    assert_eq!(resp["result"]["isError"], false);
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(
        text.contains("Checking system dependencies") && text.contains("Input Provider Diagnostic"),
        "doctor 摘要內容: {text:?}"
    );
    drop(stdin);
    child.wait().expect("wait");
}

/// 完整 run + 視覺閉環（需真實 vhs；本機手動跑：`cargo test --test mcp_stdio -- --ignored`）
#[test]
#[ignore]
fn run_visual_loop_returns_frames() {
    let (mut child, mut stdin, mut reader) = spawn_mcp();
    init(&mut stdin, &mut reader);
    let resp = round_trip(
        &mut stdin,
        &mut reader,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "tapedeck_run", "arguments": {
            "script": "Set Output t4_test.webm\nType \"ls\"\nKey Enter\n",
            "append_signature": true,
        }}}),
    )
    .expect("tapedeck_run 回應");
    let r = &resp["result"];
    assert_eq!(r["isError"], false, "run 失敗: {}", r["content"]);
    assert_eq!(r["content"][0]["text"].as_str().unwrap(), "錄製完成");
    // asset protocol（包裝後在頂層）
    let rec = r["record_id"].as_str().expect("record_id");
    assert!(rec.starts_with("rec_"), "record_id 格式: {rec}");
    assert!(
        r["preview_frame_uri"]
            .as_str()
            .expect("preview_frame_uri")
            .starts_with("tapedeck://"),
        "preview_frame_uri 格式"
    );
    // 視覺閉環：3 幀 Base64 PNG
    let frames = r["frames"].as_array().expect("frames 陣列");
    assert_eq!(frames.len(), 3, "3 張關鍵影格");
    for f in frames {
        let raw = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            f.as_str().unwrap(),
        )
        .expect("Base64 decode");
        assert_eq!(&raw[..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes");
    }
    assert!(r["signature"].as_str().unwrap().contains("tapedeck"));
    assert!(rec.starts_with("rec_"), "record_id 格式");
    drop(stdin);
    child.wait().expect("wait");
}
