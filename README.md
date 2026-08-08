# tapedeck: 跨平台腳本驅動媒體錄影器

## 📌 專案簡介

tapedeck 是一個專為工程師、AI Agent 和技術內容創作者設計的 **雙模錄影引擎**，用 `.roll` 宣告式腳本自動化錄影：

1. **TUI 文字模式**：終端機錄製（轉譯為 vhs .tape 執行），適合 CLI 操作展示
2. **GUI 原生模式**：真實桌面視窗側錄（Wayland/X11），滑鼠/鍵盤注入 + 硬體加速 AV1/VP9 編碼

## 🛠️ 核心功能

- **智慧引擎調度**：`Set Engine Auto` 自動選擇最佳後端（vhs / Native）
- **硬體探針**：偵測 GPU AV1 硬體，自動啟用零負擔編碼（AV1 HW → VP9 HW → VP9 SW 降級鏈）
- **宣告式腳本**：`.roll` 語法，雙層設計 — 輸入操作 + 自動化層（視窗等待/前後置指令/優化）
- **TUI 雙欄導播台**：fzf 選單 + Sixel 即時預覽
- **資產追蹤**：SQLite 關聯媒體檔與 Markdown 引用
- **孤兒資產清理**：一鍵刪除無引用的大檔案

## 🚀 安裝與使用

```bash
# 安裝（需 Rust + cargo）
cargo install tapedeck

# 執行 .roll 腳本
tapedeck run examples/test_tui.roll

# 先乾跑確認解析與引擎選擇
tapedeck run --dry-run examples/test_tui.roll
```

## 📜 腳本語言 (.roll)

> 語法規格以 [openspec/specs/roll-dsl/](openspec/specs/roll-dsl/) 為唯一依據，教學見 [docs/scripting.md](docs/scripting.md)，本段落僅為速覽。

### TUI 錄影範例

```
# examples/test_tui.roll
Set Engine Auto
Set Output "test_tui.gif"
Set FPS 15

Type "echo Hello Tapedeck"
Enter
Sleep 1s
```

### GUI 自動化範例

```
# examples/gui_demo.roll
Set Engine Native
Set Output "assets/obsidian_demo.webm"

ExecBefore "obsidian"
WaitWindow "Obsidian" timeout=10s
TargetWindow "Obsidian"

Roll 15s
MouseMove 500 300 speed=smooth
Click Left
Shortcut "Ctrl+S"

ExecAfter "pkill obsidian"
Optimize AV1 encoder=av1_vaapi
```

### 指令速查

| 指令 | 說明 |
|------|------|
| `Set Engine <Auto\|VHS\|Native>` | 選擇執行引擎 |
| `Set Output "..."` / `Set FPS N` / `Set Shell "..."` | 輸出 / 幀率 / shell |
| `Type "..."` / `Enter` | 輸入文字 / 按下 Enter |
| `Key <按鍵> [次數]` | 按指定鍵（Down/Up/Enter/Tab/q...） |
| `Sleep 500ms` | 暫停 |
| `MouseMove X Y [speed=smooth]` / `Click <Left\|Right\|Middle>` | 滑鼠控制 |
| `Shortcut "Ctrl+S"` | 組合鍵 |
| `ExecBefore "..."` / `ExecAfter "..."` | 錄製前/後執行 shell 指令 |
| `WaitWindow "..." timeout=10s` | 等待視窗出現 |
| `TargetWindow "..."` | 鎖定目標視窗 |
| `Roll 10s` | 錄製時長 |
| `Optimize <codec> [encoder=...]` | 錄製後優化 |

舊寫法相容：`Mode TUI|GUI`、`Title`、`Output`、`FPS`、`Terminal` 仍可解析。

## 🤖 MCP 伺服器（AI Agent 整合）

`tapedeck mcp` 以 stdio 啟動 MCP 伺服器（JSON-RPC 2.0，newline-delimited framing），供 AI Agent（如 opencode / Claude）直接呼叫：

```bash
tapedeck mcp
```

| 工具 | 說明 |
|------|------|
| `tapedeck_run` | 執行 .roll 腳本（可帶 `humanize`、`append_signature`、`max_size`） |
| `tapedeck_inspect_environment` | 環境診斷（依賴 / 輸入後端 / 硬體能力） |
| `tapedeck_extract_frames` | 按時間點抽取 PNG 影格 |
| `tapedeck_link` | 媒體檔登錄資產庫（SQLite） |
| `tapedeck_optimize` | ffmpeg 壓製（webm→gif / webp） |
| `tapedeck_clean` | 清除資產庫孤兒 |

**視覺反饋閉環**：`tapedeck_run` 錄製後回傳 3 張關鍵影格（開始/中間/結束，Base64 PNG）+ `record_id`/`preview_frame_uri`（asset protocol），Agent 以 Vision LLM 自行驗證結果 — 錄製 → PNG 影格 → 視覺驗證，全自動化 E2E。

規格見 `openspec/specs/mcp/`（T1-T4 已完成，T5 文件同步中）。

## 📂 專案結構

```
openspec/          # 規格（唯一需求來源）：project.md + specs/<change-set>/
docs/              # 教學文件
examples/          # .roll 腳本範例
src/               # Rust 原始碼
```

## 📜 授權

本專案採用 **MIT License**。

---

🔧 **貢獻歡迎**：提交 PR 前請先開 Issue 討論。
