# requirements.md — MCP 工具伺服器

## REQ-1：JSON-RPC stdio 伺服器

- **REQ-1.1**：`src/mcp/` 實作完整 MCP 伺服器（非 stub），以 stdin/stdout 走 JSON-RPC 2.0
- **REQ-1.2**：main.rs 掛載 `mod mcp`，`tapedeck mcp` 子指令啟動伺服器
- **REQ-1.3**：支援 initialize 握手（protocolVersion / capabilities / serverInfo）
- **REQ-1.4**：實作 tools/list 回傳工具清單（name / description / inputSchema）
- **REQ-1.5**：實作 tools/call 分派到真實執行路徑（非 placeholder）
- **REQ-1.6**：單一 stdio session：client 啟動 process → stdin/stdout 通訊 → EOF 結束

## REQ-2：四工具與 dispatcher 真正整合

- **REQ-2.1**：`record_roll`：執行指定 .roll（vhs/Native 引擎自動解析），回傳輸出檔路徑
- **REQ-2.2**：`link`：資產登錄（T8 db.rs 既有路徑），回傳 DB 記錄
- **REQ-2.3**：`optimize`：呼叫 media-export optimize 模組，回傳輸出檔路徑
- **REQ-2.4**：`clean`：孤兒掃描（T8 db.rs 既有路徑，dry-run 預設），回傳報告
- **REQ-2.5**：工具錯誤以 JSON-RPC error 或 isError 回傳（含 stderr 摘要）

## REQ-3：record_and_inspect 視覺閉環（Pillar 3）

- **REQ-3.1**：`record_and_inspect` 工具：執行 .roll 後抽 3 張關鍵影格（PNG/Base64）
- **REQ-3.2**：影格時間點來源優先序：操作點 JSONL → 均勻抽樣（media-export T4 共用邏輯）
- **REQ-3.3**：回傳格式：`[{frame_index, timestamp_ms, data_base64}]`，供 Agent Vision LLM 驗證
- **REQ-3.4**：閉環：錄製 → PNG 影格 → Vision 驗證，Agent 端自行完成

## REQ-4：協議測試

- **REQ-4.1**：stdio 協議測試覆蓋 initialize → tools/list → tools/call 完整握手
- **REQ-4.2**：四工具各一工具呼叫測試（含錯誤路徑：缺參數 / 檔案不存在）
- **REQ-4.3**：record_and_inspect 影格回傳格式測試（3 張、Base64、timestamp 排序）

## REQ-5：文件同步

- **REQ-5.1**：README.md 補 MCP 章節（工具清單、啟動方式、閉環示意）
- **REQ-5.2**：project.md Pillar 3 狀態更新為已實作（變更集完成後）

## SCN 情境

- **SCN-1**：Cursor/OpenCode 連 tapedeck MCP → 呼叫 record_roll 錄 TUI 展示 → 收到輸出路徑
- **SCN-2**：Agent 呼叫 record_and_inspect → 收到 3 張 PNG Base64 → Vision LLM 確認按鈕顏色正確
- **SCN-3**：Agent 呼叫 clean → 收到孤兒資產清單 → 確認無誤後呼叫（非 dry-run）
- **SCN-4**：工具參數錯誤 → 收到結構化 error，不 crash 伺服器（session 持續可用）

## 非目標（Non-Goals）

- 不實作 Streamable HTTP transport（僅 stdio，OQ-07 定案範圍）
- 不實作資源/提示（resources/prompts）— 僅 tools
- 不實作認證（stdio 由本機啟動，無網路暴露）
- 不實作錄製即時進度 notifications（錄製為同步阻塞呼叫）
