# tapedeck-mcp 全功能神兵規格書（另一 session 討論建議）

> 來源：用戶 2026-08-08 貼文（另一 session 討論建議），與 docs/ref/tapedeck-mcp-architecture.md 互補
> 狀態：已整合進 openspec/specs/mcp/design.md（六工具全做 + Features 定案）

## 架構總覽

```
┌─────────────────────────────────────────┐
│ AI Agent / Cursor / Claude Desktop      │
└────────────────────┬────────────────────┘
                     │ (stdio JSON-RPC 2.0)
                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ tapedeck-mcp                                                            │
│                                                                         │
│ [Tools]                                                                 │
│ 1. Tapedeck_run           ➔ 執行 .roll + 視覺反饋（含 Final Frame）     │
│ 2. Tapedeck_inspect_env   ➔ 封裝 doctor 探針（uinput / wtype）          │
│ 3. Tapedeck_extract_frames➔ 按 Timestamp/Keyframe 抓取多張 PNG 影格     │
│ 4. Tapedeck_link          ➔ 連結/查詢 SQLite 資產庫與元資料              │
│ 5. Tapedeck_optimize      ➔ 自動壓製/裁剪 GIF/WebM 體積與畫質           │
│ 6. Tapedeck_clean         ➔ 清理 SQLite 中的過期/失效快取資產           │
│                                                                         │
│ [Features]                                                              │
│ • humanize（預設可選：隨機打字微 delay + 鍵盤節奏自然化）               │
│ • JSON Action Array 轉譯層（支援 JSON AST ➔ .roll 自動轉譯）            │
│ • append_signature（可選推廣標籤，Agent 生成 Issue 時一鍵帶入）          │
└─────────────────────────────────────────────────────────────────────────┘
```

## 全功能上線後的殺手級應用場景

- **視覺診斷與調校閉環（run + extract_frames）**：Agent 丟出 .roll 跑一遍後，不僅看得到最後一幀，還能用 extract_frames 抽取出第 2 秒、第 5 秒的關鍵畫面，精確確認選單動畫是否有殘影或過渡中斷。
- **自動資產減肥（optimize）**：Agent 錄好一支 20MB 的 WebM 要貼到 GitHub 時，調用 optimize 工具自動調用 FFmpeg 將體積壓到 5MB 以下，完全不需人類介入。
- **安全雙軌輸入（script 或 action_array）**：不管 Agent 是想直接寫原生的 .roll DSL，還是傾向生成結構化的 JSON Action Array（`[{"type": "type", "text": "ls"},...]`），MCP 都能 100% 完美解析。
