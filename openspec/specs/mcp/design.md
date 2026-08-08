# design.md — MCP 工具實作（OQ-07 / Pillar 3）

> 變更集：`mcp`
> 規格來源：OQ-07 決策（project.md）、Pillar 3（project.md 6.1）、docs/ref/tapedeck-mcp-architecture.md（用戶 2026-08-08 架構建議）、docs/ref/tapedeck-mcp-promotion.md（Issue Repro 場景）、docs/ref/tapedeck-mcp-spec-full.md（另一 session 規格書）
> 定案（2026-08-08 用戶）：六工具全做、手寫 stdio JSON-RPC、humanize 預設關閉、JSON action array MVP 不做、append_signature 預留可選
> 協議細節（stdio framing、initialize 握手、tools/list/tools/call schema 精確格式）：依賴 lib-5 調研（進行中），T1 實作前定稿

## 1. 定位

tapedeck-mcp 讓 AI Agent（Cursor / Claude / OpenCode）把 tapedeck 當成 **Terminal 導播與動態展演系統**，而非單純 shell 指令：

- 寫 .roll → 執行錄製 → 回傳視覺證據（PNG 影格）→ Agent 自我驗證 / 自我修正
- 存取 SQLite 資產庫（歷程檢索）
- 生成高品質 .roll 腳本的引導範本

## 2. 架構（MCP 三維度）

```
┌─────────────────────────────┐
│  AI Agent (Cursor/Claude…)  │
└──────────────┬──────────────┘
      │ stdio JSON-RPC 2.0
┌─────┴──────────────────────────────┐
│         tapedeck-mcp server         │
│  ┌─────────┐ ┌─────────┐ ┌───────┐ │
│  │ Tools    │ │ Resources│ │Prompts│ │
│  │ 動作執行 │ │ 歷程檢索 │ │ 引導 │ │
│  └────┬────┘ └────┬────┘ └───┬───┘ │
│       └───────────┼──────────┘      │
│              dispatcher / db         │
└──────────────────────────────────────┘
```

### 2.1 Tools（Agent 的手與腳）— ✅ 定案：六工具全做

OQ-07 四工具（record_roll / link / optimize / clean）+ 用戶架構建議
（inspect_environment / extract_frames）合併為六工具：

| 工具 | 對應現有 CLI | 說明 |
|------|--------------|------|
| `tapedeck_run` | `tapedeck run` | 執行 .roll + 錄後影格回傳（Pillar 3）|
| `tapedeck_inspect_environment` | `tapedeck doctor` | doctor 封裝（backend/deps/probe 摘要）|
| `tapedeck_extract_frames` | filmstrip | 按 Timestamp/Keyframe 抽多張 PNG |
| `tapedeck_link` | `tapedeck link` | 連結/查詢 SQLite 資產庫與元資料 |
| `tapedeck_optimize` | `tapedeck optimize` | 壓製/裁剪 GIF/WebM 體積與畫質 |
| `tapedeck_clean` | `tapedeck clean` | 清理 SQLite 過期/失效快取資產 |

### 2.2 Resources（Agent 的眼睛與記憶）

| URI | 內容 |
|-----|------|
| `tapedeck://records/latest` | 最近一次錄製元資料（SQLite：record_id、耗時、產出路徑、狀態）|
| `tapedeck://records/{id}/frames` | 資產庫影格清單（Agent 讀圖做視覺驗證）|

### 2.3 Prompts（Agent 的大腦教練）

| Prompt | 內容 |
|--------|------|
| `generate_demo_roll` | 引導 Agent 寫高質感 .roll（Sleep 停頓、Type 自然節奏、Set Output）|

## 3. 關鍵實作細節（用戶架構「殺手級」五點）

### 3.1 視覺反饋閉環（Pillar 3 核心）

`tapedeck_run` 的 Response 不只回文字，回 **最後一幀 Base64 PNG**（MCP Image Content）：
- Vision Agent 執行完即「看到」畫面是否正確渲染
- 失敗 → Agent 依圖自動修正 .roll 再試（Self-Healing）
- 實作：錄製完成後抽 3 張關鍵影格（開始/中間/結束，filmstrip 現有能力），回傳結束幀為主、其餘可選

### 3.2 Pre-flight Doctor Checks

`tapedeck_run` 執行前內部輕量探針：
- 偵測 backend 是 Uinput 還是 Wtype
- .roll 含 Mouse 指令但 backend 為 Wtype → 提前返回提示
  「Current input backend is Wtype (Keyboard only), but script contains mouse ops. Fallback to keyboard navigation or grant /dev/uinput permissions.」
- 避免 Agent 浪費 token 跑無效錄製（doctor Input Provider Diagnostic 已有此資料）

### 3.3 Human-like Timing（humanize 參數）— ✅ 定案：預設關閉

`humanize: false`（預設）— Agent 明確設 `humanize: true` 才生效：
Type 間加 50~150ms 隨機 delay、Enter/視窗切換後插 Sleep 500ms。
> 定案理由：.roll 是作者精確控制時間點的 DSL，預設開啟會改寫語義、行為不可預期。

### 3.4 防呆語法 Schema — ✅ 定案：MVP 只做 .roll 字串 + 校驗錯誤訊息

Tool Input Schema 明確定義 .roll 支援的指令集；執行前語法校驗，錯誤回傳明確訊息。
> JSON action array 轉譯層（`[{type, text, code, ms}]` → .roll）列後續（非 MVP）。

### 3.5 Asset Protocol

`tapedeck_run` Response 直接含 SQLite record_id + 實體檔案 URI：

```json
{
  "status": "success",
  "record_id": "rec_20260808_001",
  "media": { "webm": "/path/to/demo.webm", "gif": "/path/to/demo.gif", "frame_count": 45 },
  "preview_frame_uri": "tapedeck://records/rec_20260808_001/frames/latest"
}
```

### 3.6 append_signature（推廣標籤）— ✅ 定案：預留可選參數

`append_signature: false`（預設）— Agent 明確指定才在 Response/產出附上推廣標籤
（`Generated with tapedeck — Automated Terminal Visual Director`）。內容與格式依
docs/ref/tapedeck-mcp-promotion.md，不預設輸出。

## 4. 技術選擇 — ✅ 定案：手寫 stdio JSON-RPC

serde_json + stdin/stdout framing（Content-Length 分隔），零新依賴，協議細節全自管。
> 符合專案「外部工具 CLI 適配 + 零新依賴」慣例；協議 schema 依 lib-5 調研定稿。

## 5. 模組分工（src/mcp/）

- `src/mcp/mod.rs` — 模組宣告 + stdio loop 掛載（main.rs）
- `src/mcp/server.rs` — JSON-RPC 2.0 framing loop（initialize → tools/list → tools/call）
- `src/mcp/tools.rs` — 工具定義（schema + 分派到 dispatcher / doctor / db / filmstrip）
- `src/mcp/prompts.rs` — Prompt 範本（generate_demo_roll）

## 6. 測試策略

- 協議層：stdio 假 input/output 迴圈測試（initialize/tools/list/tools/call 對談）
- 工具層：mock dispatcher（沿用專案 mock 慣例）
- 閉環 E2E：open code mcp 設定 → 對 AI 說「錄一段展示 git log 的 5 秒 GIF 並確認畫面好看」→ 觀察自我修正

## 7. 定案清單（2026-08-08）

1. ✅ 工具集：六工具全上（run / inspect_environment / extract_frames / link / optimize / clean）
2. ✅ 技術：手寫 stdio JSON-RPC（serde_json，零新依賴）
3. ✅ humanize：預設關閉（`humanize: false`，Agent 明確開啟才生效）
4. ✅ JSON action array 轉譯層：MVP 不做（列後續）
5. ✅ append_signature：預留可選參數（預設不輸出，Agent 明確指定才附上）
6. ⏳ 協議細節（framing/握手/schema）：待 lib-5 調研定稿（T1 前）
