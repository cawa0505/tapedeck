# proposal.md — MCP 工具伺服器（JSON-RPC stdio + 視覺自我驗證閉環）

## 動機（Motivation）

README 與 openspec 宣稱 tapedeck 提供「原生 JSON-RPC stdio 對接 Cursor/OpenCode」的 MCP 工具，Agent 錄完可自動驗證 UI；但實際 `src/mcp/` 的 ToolManager 是 stub（只寫 placeholder 檔、未與 dispatcher 整合）。MCP 視覺閉環是內容工程工作檯的四大功能支柱之一（Pillar 3），宣稱與實作落差必須消除。

## 問題（Problem）

- `src/mcp/tools.rs` 是 stub：`ToolManager::execute` 寫 placeholder 檔，未呼叫真實引擎
- Agent 無法透過 MCP 協議呼叫 tapedeck 的錄製/資產管理/壓製能力
- 錄製完成後無自動化視覺驗證：Agent 無法取得影格做 Vision 檢查（按鈕顏色/Layout/文字輸入）
- README 對外宣稱與實際能力不一致（OQ-07 議案）

## 成功標準（Success Criteria）

1. **JSON-RPC stdio 伺服器**：`src/mcp/` 實作完整伺服器（非 stub），main.rs 掛載 `mod mcp`
2. **四工具與 dispatcher 真正整合**：record_roll / link / optimize / clean 呼叫真實執行路徑
3. **record_and_inspect 閉環工具**：錄完抽 3 張關鍵影格（PNG/Base64）回傳 Agent Context
4. **100% 自動化 E2E**：Agent 錄製 → PNG 影格 → Vision 驗證，全程無手工介入
5. **驗證**：stdio 協議測試（initialize/tools/list/tools/call 握手）+ 四工具端到端
