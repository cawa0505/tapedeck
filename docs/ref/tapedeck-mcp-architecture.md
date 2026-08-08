# tapedeck-mcp 架構建議（用戶提供，2026-08-08）

> 來源：用戶於 2026-08-08 貼入的設計建議文件，逐字保存。
> 用途：MCP 變更集（openspec/specs/mcp/）的設計輸入參考。

## 一、tapedeck-mcp 的全貌圖景（Architecture Vision）

在 MCP 的規範下，AI Agent（如 Cursor, Claude Desktop, AutoGPT）不是單純把 tapedeck 當一個 Shell 指令來執行，而是把它當成「Terminal 導播與動態展演系統」。

Tapedeck-mcp 應該提供三個核心維度：

```
┌──────────────────────────────────────────────────────────┐
│ AI Agent / Cursor                                        │
└──────────────┬─────────────────┬─────────────────┬───────┘
               │                 │                 │
               ▼                 ▼                 ▼
       【 1. Tools 】      【 2. Resources 】  【 3. Prompts 】
       動作執行與錄製      歷程檢索與資產庫    語法範本與引導
```

### 1. Tools（工具箱：Agent 的手與腳）

- **Tapedeck_run**：接受 .roll 內容，執行自動化操作並錄製（可指定產出格式 WebM / GIF / PNG 影格）。
- **Tapedeck_inspect_environment**：封裝 tapedeck doctor，讓 Agent 在寫腳本前先知道當前環境（顯示器解析度、Backend 是 Uinput 還是 Wtype、支援哪些 CLI 工具）。
- **Tapedeck_extract_frames**：將錄好的 WebM 拆解成特定 Timestamp 或關鍵影格（PNG），供 Vision Model 審查。

### 2. Resources（資源庫：Agent 的眼睛與記憶）

- `Tapedeck://records/latest`：取得最近一次錄製的元資料（SQLite 紀錄、耗時、產出路徑、成功/失敗狀態）。
- `Tapedeck://records/{id}/frames`：暴露 SQLite 資產庫裡的影格清單，讓 Agent 能直接讀取圖像進行視覺驗證。

### 3. Prompts（引導範本：Agent 的大腦教練）

- **Generate_demo_roll**：內建系統級 Prompt，引導 Agent「如何寫出高質感、有停頓感（Sleep）、打字速度自然（Type）的 .roll 腳本」。

## 二、AI Agent 最需要且最貼心的「殺手級」實作建議

AI Agent 在操作 CLI 或生成自動化腳本時，最常遇到的痛點是：「盲目執行、不知畫面發生什麼事、出錯時無法自我修復」。針對這些痛點，以下是 5 個能讓 Agent「感動到哭」的貼心實作：

### 1. 視覺反饋閉環（Visual Feedback Loop — 超級重點！）

**痛點**：Agent 丟出一個 .roll 腳本後，如果語法寫錯或 UI 沒反應，它完全不知道發生什麼事。

**貼心實作**：tapedeck_run Tool 的回傳值（Response），不要只傳回「錄製成功/失敗」的純文字。**MCP Image Content：在工具回傳中，直接附帶最後一幀（Final Frame）的 Base64 PNG 圖像或關鍵狀態截圖！**

好處：具備 Vision 能力的 Agent（如 Claude 3.5 Sonnet / GPT-4o）只要一執行完，瞬間就能「看到」畫面有沒有被正確渲染、選單有沒有打開。如果沒成功，Agent 會根據看到的圖片自動修正 .roll 腳本再試一次（Self-Healing）。

### 2. 智慧自我診斷前置（Pre-flight Doctor Checks）

**痛點**：Agent 寫了滑鼠移動語法，結果環境權限不足（/dev/uinput 沒權限），導致執行掛掉。

**貼心實作**：在 tapedeck_run 執行前，內部自動發起輕量級探針。如果偵測到當前是 Wtype（無滑鼠能力），但在 .roll 裡看到了 Mouse 操作指令，MCP Tool 在執行前就直接返回明確提示：

> "Notice: Current input backend is Wtype (Keyboard only), but your .roll script contains Mouse relative movement. Fallback to Keyboard navigation or instruct user to grant /dev/uinput permissions."

這能避免 Agent 浪費 Token 去跑無效的錄製。

### 3. 人類節奏優化器（Human-like Timing Assistant）

**痛點**：AI 寫出的操作腳本通常「太快」，指令黏在一起，錄出來的展演影片像機器人在飆車，視覺體驗極差。

**貼心實作**：MCP 在解析 Agent 傳入的 .roll 時，提供一個 `humanize: true` 參數（預設開啟）。

- 自動在 Type 指令間加入微小的隨機 delay（e.g. 50~150ms）。
- 自動在關鍵 Enter 或視窗切換後插入 Sleep 500ms。

結果：Agent 不需要精確計算人類的觀看節奏，tapedeck-mcp 自動幫它壓出最完美的展示影片。

### 4. 防呆 .roll 語法 Schema（Strict JSON Schema for Tools）

**痛點**：Agent 經常會發明不存在的 DSL 指令。

**貼心實作**：在 MCP Tool 的 Input Schema 中，明確定義 .roll 支援的 AST / 指令集（或提供一個 tapedeck_validate_roll Tool）。甚至可以允許 Agent 傳入 JSON 格式的操作 Array，由 MCP 自動轉譯成 .roll 檔案：

```json
{
  "actions": [
    { "type": "type", "text": "git status" },
    { "type": "key", "code": "enter" },
    { "type": "sleep", "ms": 1000 }
  ]
}
```

這能讓不擅長字串拼接的 Agent 用 100% 正確的 JSON 結構生成自動化劇本。

### 5. 產出物與 SQLite 資產直接關聯（Asset Protocol）

**貼心實作**：錄製完成後，MCP 返回的 JSON 結果直接包含 SQLite 中的 record_id 與實體檔案 URIs：

```json
{
  "status": "success",
  "record_id": "rec_20260808_001",
  "media": {
    "webm": "/path/to/demo.webm",
    "gif": "/path/to/demo.gif",
    "frame_count": 45
  },
  "preview_frame_uri": "tapedeck://records/rec_20260808_001/frames/latest"
}
```

讓 Agent 可以非常方便地把這些產出拿去寫 Markdown 文件、發 PR Description，或是貼進 Issue 裡。

## 三、建議的 MCP 實作步驟（Phase 2 開工清單）

既然你打算先做 MCP，建議可以照這個順序推進：

1. **Tapedeck-mcp Crate 獨立或 Cargo Workspace 拆分**：使用 Rust 生態中成熟的 rmcp（Rust MCP SDK）或搭建標準 stdio JSON-RPC。
2. **實作第一個 Tool: tapedeck_run**：接通你現有的 Native Engine 與 .roll Parser。將執行的最後一張 PNG 影格包進 MCP Response（Image Content type）。
3. **實作 Resource: tapedeck://records**：讀取 SQLite 資產庫，讓 Agent 能查詢過往錄製紀錄。
4. **驗證與 Agent 閉環**：設定 opencode mcp，試著對 AI 說：「幫我錄一段展示 git log 的 5 秒 GIF，並確認畫面有沒有好看」，觀察 AI 根據回傳圖片進行自我修正的過程。

---

## 與既有決策的對照

| 建議項目 | 既有 OQ-07 / Pillar 3 定案 | 差距 |
|---------|---------------------------|------|
| Tools 三個 | OQ-07 四工具（record_roll/link/optimize/clean） | 建議多了 inspect_environment、extract_frames；clean/link 保留 |
| Image Content 回傳 | Pillar 3 record_and_inspect 錄完抽 3 張 PNG/Base64 | 建議採「最後一幀」而非 3 張 |
| Pre-flight 診斷 | doctor 已實作 Input Provider Diagnostic（T10） | 需 MCP 層包裝 |
| humanize 參數 | 未定案 | 新增項目 [待討論] |
| JSON actions 輸入 | .roll 是唯一編寫 DSL（#2925/#2930） | 衝突：建議允許 JSON 輸入，需定案 |
| Asset Protocol | T8 SQLite 資產圖譜（三層關聯） | record_id 需與 assets 表連結 |
| rmcp SDK vs 手寫 JSON-RPC | OQ-07 原生 JSON-RPC stdio | 待 lib-5 調研確認 |
