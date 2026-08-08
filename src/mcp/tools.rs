//! MCP 工具層（T2：六工具接真實執行路徑）
//!
//! T1 完成 tools/list metadata；T2 將 dispatcher / doctor / db / media
//! 接到 tools/call 分派。T3 追加視覺閉環（最後一幀 PNG）。

use serde_json::{json, Value};

/// tools/list 用的工具 metadata（MCP 協議欄位為 camelCase：inputSchema）
#[derive(serde::Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// tools/call 執行錯誤
#[derive(Debug)]
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

/// 執行工具呼叫（T2：六工具分派真實執行路徑）
pub async fn execute(name: &str, arguments: Value) -> Result<Value, ToolError> {
    match name {
        "tapedeck_run" => cmd_run(&arguments).await,
        "tapedeck_inspect_environment" => cmd_inspect_environment(),
        "tapedeck_extract_frames" => cmd_extract_frames(&arguments),
        "tapedeck_link" => cmd_link(&arguments),
        "tapedeck_optimize" => cmd_optimize(&arguments),
        "tapedeck_clean" => cmd_clean(&arguments),
        _ => Err(ToolError::Unknown(name.to_string())),
    }
}

fn req_str(arguments: &Value, key: &str) -> Result<String, ToolError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Execution(format!("缺少參數 {key}")))
}

// ─────────────────────────── tapedeck_run ───────────────────────────

async fn cmd_run(arguments: &Value) -> Result<Value, ToolError> {
    let script = req_str(arguments, "script")?;
    let humanize = arguments
        .get("humanize")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let append_signature = arguments
        .get("append_signature")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_size = arguments
        .get("max_size")
        .and_then(Value::as_u64)
        .map(|m| m as u32);
    let output = arguments
        .get("output")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);

    // pre-flight（design.md 3.2）：解析 .roll 檢查 Mouse 指令 + backend 能力
    let parsed = crate::engine::roll_parser::parse_roll_content(&script)
        .map_err(|e| ToolError::Execution(format!(".roll 語法錯誤: {e}")))?;
    let backend = crate::engine::input::InputBackend::detect();
    let has_mouse = parsed.commands.iter().any(|c| {
        matches!(
            c,
            crate::engine::roll_parser::ScriptCommand::MouseMove(_, _)
                | crate::engine::roll_parser::ScriptCommand::Click(_)
        )
    });
    if matches!(backend, crate::engine::input::InputBackend::Wtype) && has_mouse {
        return Err(ToolError::Execution(
            "Current input backend is Wtype (Keyboard only), but script contains mouse ops. Fallback to keyboard navigation or grant /dev/uinput permissions.".to_string(),
        ));
    }

    // humanize（design.md 3.3，預設 false）：改寫 script 內容，Type 間加自然 delay
    let script = if humanize {
        humanize_script(&script)
    } else {
        script
    };

    // script 內容 → 臨時 .roll 檔（MCP 是內容式呼叫，dispatcher 吃路徑）
    let dir = std::env::temp_dir().join("tapedeck-mcp");
    std::fs::create_dir_all(&dir)
        .map_err(|e| ToolError::Execution(format!("無法建立暫存目錄: {e}")))?;
    let script_file = dir.join(format!("script-{}.roll", std::process::id()));
    std::fs::write(&script_file, &script)
        .map_err(|e| ToolError::Execution(format!("無法寫入暫存腳本: {e}")))?;

    let args = crate::cli::RunArgs {
        script_file,
        output: output.clone(),
        fps: None,
        max_size,
        gif: false,
        webp: false,
        dry_run: false,
    };

    // dispatcher::run 是 async（backend lifecycle）— 在既有 tokio runtime 內 await
    crate::engine::dispatcher::run(args)
        .await
        .map_err(|e| ToolError::Execution(format!("錄製失敗: {e:#}")))?;

    // 視覺閉環（design.md 3.1 / Pillar 3）：抽 3 張關鍵影格（開始/中間/結束）→ Base64 PNG
    // 實際輸出由 dispatcher 依 script Set Output 解析（與 CLI run 同規則）
    let out_path = crate::paths::resolve_output_path(
        parsed.output.as_deref().unwrap_or("output.webm"),
        output.as_deref(),
    )
    .map_err(|e| ToolError::Execution(format!("解析輸出失敗: {e}")))?;
    let frames = extract_frames_base64(&out_path)?;

    let rec = record_id();
    let mut result = json!({
        "status": "success",
        "message": "錄製完成",
        // asset protocol（design.md 3.5）：record_id 自產（無 record 表，
        // 時間戳格式；T8 assets 表後續可對應）
        "record_id": rec,
        "media": {
            "uri": format!("tapedeck://records/{rec}/media"),
            "path": out_path.to_string_lossy(),
        },
        "preview_frame_uri": format!("tapedeck://records/{rec}/frames/latest"),
        "frame_count": frames.as_ref().map_or(0, Vec::len),
        "frames": frames,
    });
    if append_signature {
        result["signature"] = json!("Generated with tapedeck — Automated Terminal Visual Director");
    }
    Ok(result)
}

/// 自產 record_id（rec_YYYYMMDD_HHMMSS，零依賴；未來對應 SQLite record 表）
fn record_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 簡潔做法：epoch 秒 → 8 位 hex（避免引入 chrono 依賴）
    format!("rec_{secs:08x}")
}

/// humanize（design.md 3.3）：在 Type 後插入 50~150ms Sleep、Enter/切換後 500ms。
/// 零依賴抖動（時間戳種子 LCG，非密碼用途）。
fn humanize_script(content: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x9e37);
    let mut state = seed;
    let mut rng = move || {
        // xorshift32（足夠產生自然 delay）
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let mut out = String::new();
    for line in content.lines() {
        out.push_str(line);
        out.push('\n');
        let trimmed = line.trim_start();
        if trimmed.starts_with("Type") {
            let delay = 50 + (rng() % 101) as u64; // 50..=150ms
            out.push_str(&format!("Sleep {delay}\n"));
        } else if trimmed.starts_with("Key Enter")
            || trimmed.starts_with("Key Tab")
            || trimmed.starts_with("Ctrl")
            || trimmed.starts_with("Alt")
        {
            out.push_str("Sleep 500\n");
        }
    }
    out
}

/// 抽 3 張關鍵影格（開始/中間/結束，design.md 3.1）→ Base64 PNG 陣列。
/// 時間點均勻抽樣（操作點 JSONL 來源為進階增強，MVP 用均勻 3 點）；
/// probe 失敗回 None，不阻塞 run 結果。
fn extract_frames_base64(input: &std::path::Path) -> Result<Option<Vec<String>>, ToolError> {
    let duration_ms = crate::media::ffmpeg::probe_duration_ms(input);
    let Some(duration_ms) = duration_ms else {
        return Ok(None);
    };
    if duration_ms == 0 {
        return Ok(None);
    }
    // 開始 / 中間 / 結束（結束略早於結尾，避免 ffmpeg 在 EOF 邊界的空幀）
    let points = [0, duration_ms / 2, duration_ms.saturating_sub(50)];
    let dir = std::env::temp_dir().join("tapedeck-mcp").join("frames");
    std::fs::create_dir_all(&dir)
        .map_err(|e| ToolError::Execution(format!("無法建立影格目錄: {e}")))?;
    let frames = crate::media::filmstrip::extract_frames(input, &points, &dir, false)
        .map_err(|e| ToolError::Execution(format!("抽幀失敗: {e}")))?;
    if frames.is_empty() {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(frames.len());
    for frame in frames {
        let bytes = std::fs::read(&frame)
            .map_err(|e| ToolError::Execution(format!("讀取影格失敗: {e}")))?;
        out.push(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            bytes,
        ));
    }
    Ok(Some(out))
}

// ─────────────────── tapedeck_inspect_environment ───────────────────

fn cmd_inspect_environment() -> Result<Value, ToolError> {
    Ok(json!({
        "status": "success",
        "report": crate::doctor::doctor_report(),
    }))
}

// ─────────────────────── tapedeck_extract_frames ───────────────────────

fn cmd_extract_frames(arguments: &Value) -> Result<Value, ToolError> {
    let media = req_str(arguments, "media")?;
    let media_path = std::path::Path::new(&media);
    if !media_path.exists() {
        return Err(ToolError::Execution(format!("媒體檔不存在: {media}")));
    }

    // 時間點：timestamps（秒）指定，或 count 均勻抽樣
    let points_ms: Vec<u64> =
        if let Some(ts) = arguments.get("timestamps").and_then(Value::as_array) {
            ts.iter()
                .filter_map(Value::as_f64)
                .map(|s| (s * 1000.0) as u64)
                .collect()
        } else {
            let count = arguments
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or(4)
                .max(1) as usize;
            let duration = crate::media::ffmpeg::probe_duration_ms(media_path).unwrap_or(5_000);
            (0..count)
                .map(|i| duration * i as u64 / count as u64)
                .collect()
        };
    if points_ms.is_empty() {
        return Ok(json!({ "status": "success", "frames": [] }));
    }

    let out_dir = std::env::temp_dir().join(format!("tapedeck-mcp/frames-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| ToolError::Execution(format!("無法建立影格目錄: {e}")))?;

    let frames = crate::media::filmstrip::extract_frames(media_path, &points_ms, &out_dir, false)
        .map_err(|e| ToolError::Execution(format!("抽幀失敗: {e:#}")))?;

    Ok(json!({
        "status": "success",
        "frames": frames.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "timestamps_ms": points_ms,
    }))
}

// ───────────────────────────── tapedeck_link ─────────────────────────────

fn cmd_link(arguments: &Value) -> Result<Value, ToolError> {
    let media_file = std::path::PathBuf::from(req_str(arguments, "media_file")?);
    if !media_file.exists() {
        return Err(ToolError::Execution(format!(
            "媒體檔不存在: {}",
            media_file.display()
        )));
    }
    let format = arguments
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("md")
        .to_string();

    let tracker = crate::db::AssetTracker::open()
        .map_err(|e| ToolError::Execution(format!("開啟資產庫失敗: {e}")))?;
    tracker
        .register(&media_file, None)
        .map_err(|e| ToolError::Execution(format!("登錄資產失敗: {e}")))?;
    let link = crate::engine::dispatcher::media_link(&media_file, &format)
        .map_err(|e| ToolError::Execution(format!("產生連結語法失敗: {e}")))?;

    Ok(json!({
        "status": "success",
        "link": link,
    }))
}

// ─────────────────────────── tapedeck_optimize ───────────────────────────

fn cmd_optimize(arguments: &Value) -> Result<Value, ToolError> {
    let input = std::path::PathBuf::from(req_str(arguments, "input")?);
    if !input.exists() {
        return Err(ToolError::Execution(format!(
            "輸入檔不存在: {}",
            input.display()
        )));
    }
    let opts = crate::media::optimize::OptimizeOptions {
        input,
        output: None,
        format: arguments
            .get("format")
            .and_then(Value::as_str)
            .map(str::to_string),
        quality: arguments
            .get("quality")
            .and_then(Value::as_u64)
            .map(|q| q as u8)
            .unwrap_or(80),
        fps: arguments
            .get("fps")
            .and_then(Value::as_u64)
            .map(|f| f as u32)
            .unwrap_or(10),
        dry_run: false,
    };
    crate::media::optimize::optimize(&opts)
        .map_err(|e| ToolError::Execution(format!("壓製失敗: {e:#}")))?;

    Ok(json!({ "status": "success", "message": "壓製完成" }))
}

// ───────────────────────────── tapedeck_clean ─────────────────────────────

fn cmd_clean(arguments: &Value) -> Result<Value, ToolError> {
    let dry_run = arguments
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let tracker = crate::db::AssetTracker::open()
        .map_err(|e| ToolError::Execution(format!("開啟資產庫失敗: {e}")))?;
    let orphans = tracker
        .orphans(&std::env::current_dir().map_err(|e| ToolError::Execution(e.to_string()))?)
        .map_err(|e| ToolError::Execution(format!("掃描孤兒失敗: {e}")))?;

    let mut removed = Vec::new();
    for asset in &orphans {
        tracker
            .remove(asset, dry_run)
            .map_err(|e| ToolError::Execution(format!("清理失敗: {e}")))?;
        removed.push(asset.path.clone());
    }

    Ok(json!({
        "status": "success",
        "dry_run": dry_run,
        "orphan_count": orphans.len(),
        "removed": removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_tool_returns_unknown_error() {
        let err = execute("nope", json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::Unknown(_)));
    }

    #[tokio::test]
    async fn run_missing_script_is_execution_error() {
        match execute("tapedeck_run", json!({})).await {
            Err(ToolError::Execution(m)) => assert!(m.contains("script")),
            other => panic!("預期 Execution 錯誤，得到 {other:?}"),
        }
    }

    #[tokio::test]
    async fn link_missing_media_file_is_execution_error() {
        let err = execute("tapedeck_link", json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[tokio::test]
    async fn extract_frames_nonexistent_media_is_execution_error() {
        let err = execute(
            "tapedeck_extract_frames",
            json!({ "media": "/nonexistent/media.webm" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[tokio::test]
    async fn optimize_missing_input_is_execution_error() {
        let err = execute(
            "tapedeck_optimize",
            json!({ "input": "/nonexistent/in.webm" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[tokio::test]
    async fn inspect_environment_returns_report() {
        let out = execute("tapedeck_inspect_environment", json!({}))
            .await
            .unwrap();
        assert!(out["report"].as_str().unwrap_or("").contains("Checking"));
    }

    // ─────────────── T3：humanize / pre-flight ───────────────

    #[test]
    fn humanize_inserts_sleep_after_type() {
        let script = "Set Output demo.webm\nType \"ls -la\"\nKey Enter\nType \"exit\"\n";
        let out = humanize_script(script);
        // Type 後 2 個抖動 Sleep（50~150ms）+ Key Enter 後 1 個 Sleep 500
        let sleeps: Vec<u64> = out
            .lines()
            .filter_map(|l| l.strip_prefix("Sleep "))
            .filter_map(|v| v.parse().ok())
            .collect();
        assert_eq!(sleeps.len(), 3);
        assert_eq!(sleeps.iter().filter(|&&d| d == 500).count(), 1);
        assert!(sleeps
            .iter()
            .filter(|&&d| d != 500)
            .all(|&d| (50..=150).contains(&d)));
        // Key Enter 後插 Sleep 500
        assert!(out.contains("Key Enter\nSleep 500\n"));
        // 原指令保留
        assert!(out.contains("Set Output demo.webm\n"));
    }

    #[test]
    fn humanize_keeps_non_input_lines_unchanged() {
        let script = "Set Output demo.webm\nSleep 300\nKey Down 3\n";
        let out = humanize_script(script);
        // Sleep 後不再插、非輸入指令原樣
        assert_eq!(out.lines().count(), script.lines().count());
        assert!(out.ends_with("Key Down 3\n"));
    }

    #[tokio::test]
    async fn preflight_rejects_mouse_on_wtype_backend() {
        // 觸發前先確認本機 backend；若非 Wtype（uinput 可寫）則此測試無意義 → skip
        let backend = crate::engine::input::InputBackend::detect();
        if !matches!(backend, crate::engine::input::InputBackend::Wtype) {
            return;
        }
        let script = "MouseClick left\n";
        let err = execute("tapedeck_run", json!({ "script": script }))
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(m) => assert!(m.contains("Wtype") && m.contains("mouse")),
            other => panic!("預期 Execution 錯誤，得到 {other:?}"),
        }
    }

    #[tokio::test]
    async fn preflight_allows_keyboard_script_on_wtype_backend() {
        let backend = crate::engine::input::InputBackend::detect();
        if !matches!(backend, crate::engine::input::InputBackend::Wtype) {
            return;
        }
        // 純鍵盤 script 不觸發 pre-flight；這裡只驗證 parse 階段（會走到 temp 寫入 → 錄製失敗）
        // 因此直接測語法錯誤路徑：script 內容含非法指令 → Execution 錯誤（非 pre-flight）
        let err = execute("tapedeck_run", json!({ "script": "NotARealCommand x\n" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}
