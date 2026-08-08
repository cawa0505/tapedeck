# MCP 2.0 Stateless 升級討論（預留）

來源：2026-08-08 用戶與另一 session 的討論建議。此為未來升級預留，不影響現階段 stdio JSON-RPC 實作。

## 背景：Stateless 是 MCP 2.0 趨勢

SSE / stdio 解耦後，Stateless（無狀態）與 RESTful 微服務化是 MCP 2.0 的重要趨勢。傳統 JSON-RPC 雙向長連接（Stateful Session）需在記憶體維護連線狀態與 Session ID；Stateless 要求每次 Tool Call / Resource Request 都是自我包含（Self-contained）的獨立請求，不依賴記憶體 Session Context。

對 tapedeck-mcp 是紅利：SQLite 架構 + Rust 零依賴設計剛好符合。

## 核心影響

- **記憶體零殘留**：每次呼叫不需要永續 Daemon；請求進來 → Process 啟動 → 讀 SQLite → 執行 → 輸出 JSON → 釋放
- **併發 Scale-Out**：不依賴記憶體 Session，3 段 .roll 可完全獨立並行
- **重試成本歸零**：Agent 崩潰重連後，拿 record_id 再發請求即可無縫接軌

## 4 大優化點

1. **SQLite 作為唯一真理來源**：執行狀態（Pending→Running→Success/Failed）全部下壓 SQLite；後續工具不從記憶體要資料，只傳 record_id，MCP 從 SQLite 查出 WebM/PNG 路徑與 metadata — 所有 Tools 徹底解耦
2. **Token-based / Self-contained 請求參數**：extract_frames / optimize 的 Schema 明確要求 record_id 或絕對路徑，不假設 Server 記得上次 run 產出
3. **CLI 本身作為 Gateway**：`tapedeck mcp --stateless` 一行啟動純 stdin/stdout 無狀態 RPC 解析器，不留背景進程，用完即扔
4. **Response 顯式攜帶可修復狀態**：失敗時回 `{"status":"error","error_code":"PERMISSION_DENIED","detail":"...","suggested_action":"..."}` — Agent 即使無先前 Context 也能依單次 Payload 立即反應

## 總結

「SQLite 本地資產資料庫 + Rust 零依賴單一 Binary + doctor 探針」路線已把狀態交給 SQLite（Disk）而非記憶體 — 轉向 Stateless MCP 2.0 不需改核心邏輯，只需把 stdio JSON-RPC 請求處理器寫成「無狀態轉向器（Stateless Router）」。

## 落地時機

- MCP 1.x（現階段）：stateful stdio session 內多請求；record_id 已在 tool 參數帶入（見 design.md asset protocol）
- MCP 2.0：視官方協議發布後評估；上述 4 點已與現設計相容，升級路徑為包裝層替換，核心工具邏輯不變
