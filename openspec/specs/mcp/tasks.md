# tasks.md — MCP 工具伺服器實作任務

依賴順序排列。每個任務完成後執行 `cargo build` + 相關測試驗證。

定案（2026-08-08）：六工具全做、手寫 stdio JSON-RPC（零新依賴）、humanize 預設關閉、
JSON action array 轉譯層列後續（非 MVP）、append_signature 預留可選參數（預設不輸出）。

## 執行優先序

1. **T1 JSON-RPC stdio 伺服器骨架**（src/mcp/server.rs：framing + initialize 握手，MCP 協議底座）
2. **T2 六工具與 dispatcher 整合**（src/mcp/tools.rs 重寫，取代 stub）
3. **T3 tapedeck_run 視覺閉環**（錄後影格回傳，Pillar 3，依賴 filmstrip 抽樣邏輯）
4. **T4 協議測試**（stdio 完整握手 + 工具呼叫 + 錯誤路徑）
5. **T5 文件同步**（README MCP 章節 + project.md Pillar 3 狀態）

## T1：JSON-RPC stdio 伺服器骨架

- [x] `src/mcp/server.rs`：JSON-RPC 2.0 framing 迴圈（stdin/stdout，**newline-delimited JSON**，每行一則訊息）
- [x] initialize 握手（protocolVersion 回顯 + capabilities + serverInfo）→ 等 notifications/initialized
- [x] tools/list 回傳六工具清單（name / description / inputSchema）
- [x] `src/mcp/mod.rs` 掛載 server + tools；main.rs `mcp` 子指令
- [x] 驗證：`cargo build` + 握手測試
- [x] 協議細節以 docs/ref/mcp-stdio-protocol.md（2025-06-18 官方 spec）為準

## T2：六工具與 dispatcher 真正整合

- [x] `src/mcp/tools.rs`：重寫 ToolManager，六工具呼叫真實執行路徑：
      tapedeck_run / tapedeck_inspect_environment / tapedeck_extract_frames /
      tapedeck_link / tapedeck_optimize / tapedeck_clean
- [x] 移除 stub placeholder 邏輯（PhantomData、寫 placeholder 檔）
- [x] tapedeck_inspect_environment：封裝 doctor（backend/deps/probe 摘要）
- [x] tapedeck_extract_frames：按 timestamp 抽 PNG 影格（filmstrip 共用邏輯）
- [x] 錯誤路徑：缺參數 / 檔案不存在 → 結構化 error，session 持續可用
- [x] 驗證：六工具各一呼叫測試

## T3：tapedeck_run 視覺閉環

- [x] tapedeck_run：執行 .roll → 回傳 3 張關鍵影格 Base64 PNG（開始/中間/結束，Pillar 3）
- [x] 影格時間點來源：均勻抽樣（0/mid/end；操作點 JSONL 為進階增強，MVP 用均勻 3 點）
- [x] pre-flight doctor checks：backend 為 Wtype 但 .roll 含 Mouse 指令 → 提前回提示
- [x] `humanize` 參數（預設 false）：開啟時 Type 間加 50~150ms delay、Enter/切換後 Sleep 500ms
- [x] `append_signature` 參數（預設 false）：開啟時附推廣標籤（docs/ref/tapedeck-mcp-promotion.md）
- [x] asset protocol：回傳 record_id + media URIs + preview_frame_uri
- [x] 驗證：影格格式測試（Base64 PNG magic bytes 實測 + humanize 插入邏輯單元測試）

## T4：協議測試

- [x] stdio 完整握手測試：initialize → initialized → tools/list → tools/call
- [x] 六工具呼叫測試（含錯誤路徑）
- [x] tapedeck_run 回傳格式測試（視覺閉環 + asset protocol）
- [x] 驗證：`cargo test` 全綠 + 零警告

## T5：文件同步

- [x] README.md 補 MCP 章節（六工具清單、`tapedeck mcp` 啟動、閉環示意）
- [x] project.md Pillar 3 狀態更新（變更集完成後）
- [x] tasks.md 勾選 + commit

## 後續（非 MVP）

- [ ] JSON action array 轉譯層（JSON AST → .roll 自動轉譯）
