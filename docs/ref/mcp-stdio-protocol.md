# MCP stdio 協議細節（2025-06-18 版）

來源：modelcontextprotocol.io/specification/2025-06-18（2026-08-08 抓取）
用途：MCP 變更集 T1 伺服器骨架的協議實作依據。

## Transport：stdio（newline-delimited JSON）

- 客戶端以子程序啟動 MCP server；server 讀 stdin、寫 stdout
- **訊息以 newline 分隔，訊息內不得含 embedded newline**（非 Content-Length framing！）
- 每則訊息是獨立的 JSON-RPC request / notification / response
- server 可寫 UTF-8 到 stderr 做 logging；stdout 只能輸出合法 MCP 訊息
- 訊息一律 JSON-RPC 2.0 + UTF-8

## Lifecycle：initialize → initialized → operation

1. 客戶端送 `initialize`（第一則訊息）：
```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocolVersion":"2025-06-18",
  "capabilities":{},
  "clientInfo":{"name":"...","version":"..."}}}
```
2. server 回應同 protocolVersion + 自身 capabilities + serverInfo：
```json
{"jsonrpc":"2.0","id":1,"result":{
  "protocolVersion":"2025-06-18",
  "capabilities":{"tools":{"listChanged":true}},
  "serverInfo":{"name":"tapedeck","version":"..."}}}
```
3. 客戶端送 `notifications/initialized` 通知（無 id）後進入 operation
4. 版本協商：client 送最新版，server 支援就回顯相同版，否則回自己支援的版

## Tools

### tools/list（request）
- 支援 pagination（params.cursor 選填）
- response.result：`{"tools":[{"name","title"?,"description","inputSchema"}...],"nextCursor"?}`
- inputSchema 是 JSON Schema（type:"object", properties, required）

### tools/call（request）
- `params: {"name": "工具名", "arguments": {...}}`
- response.result：
```json
{"content":[{"type":"text","text":"..."}],"isError":false}
```

### 內容類型（content array）
- `text`：`{"type":"text","text":"..."}`
- `image`：`{"type":"image","data":"<base64>","mimeType":"image/png"}`（視覺閉環用！）

### 錯誤兩軌
1. **Protocol error**（JSON-RPC error，-32602 Invalid params、未知工具等）：
   `{"code":-32602,"message":"Unknown tool: xxx"}`
2. **Tool execution error**（result 內 `isError:true` + text content）：
   業務失敗（API fail / 檔案不存在）用這個，session 持續可用

## 收尾

- 無 shutdown 訊息：client 關 stdin → server 自然退出（EOF）
- server 也可關 stdout 並 exit

## 實作要點（T1）

- 讀 stdin 逐行：serde_json::from_str 每行 → match method
- 寫 stdout 每則訊息加 newline：serde_json::to_string + println
- initialize 未完成前只接受 initialize / ping / notifications/initialized
- 未知 method → JSON-RPC -32601 Method not found
- ping → 回 empty result（`{"jsonrpc":"2.0","id":N,"result":{}}`）
