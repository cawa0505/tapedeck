# tapedeck 系統架構與技術規格書

**專案名稱**：tapedeck
**定位**：專為極客工程師與 AI Code Agent 設計的三棲（MCP / CLI / TUI）極速媒體錄影與自動化展示工具箱
**主要語言**：Rust (2021 Edition)

---

## 1. 執行摘要 (Executive Summary)

tapedeck 旨在解決現代軟體開發中「終端機 UI (TUI) 演示錄製繁瑣」與「AI Agent 缺乏視覺驗證與展示能力」的痛點。傳統上，為 README 或 Devblog 錄製一個高質量的 GIF/MP4 需要手動開啟錄影軟體、裁切視窗並轉檔；而在 Wayland 環境下更面臨 X11 工具失效的問題。

tapedeck 透過整合 VHS Tape 腳本引擎、Wayland 原生錄影管道 (wf-recorder / PipeWire) 與 MCP 通訊協定，提供：

- **Agent 輕量無頭調用 (MCP Server)**：讓 AI Agent 在寫完程式碼後自動錄製 TUI/GUI Demo 進行自我驗證或發表部落格。
- **直覺高效的 CLI/Wayland 管道**：極速錄製指定視窗或全螢幕。
- **fzf 風格的輕量 TUI 介面**：極短學習曲線，結合模糊搜尋與 $EDITOR 整合。

---

## 2. 核心使用場景 (Use Cases)

### 2.1 Agent 視覺化自我驗證與 Blog 發文 (MCP Mode)

情境：使用者叫 Agent 修復 BloggerAgent 的 Preview 重疊 bug，並發一篇 Zola Devlog。

流程：
1. Agent 修改完 Rust/Go 程式碼。
2. Agent 調用 tapedeck MCP Tool `record_tui_tape`，輸入生成的 VHS 腳本。
3. tapedeck 在背景以 Headless 方式執行 vhs，輸出 `assets/demo.gif`。
4. Agent 將產出的 GIF 貼入 Zola / Blogger 文章內，完成開源專案更新與發布閉環。

### 2.2 現代 Wayland 桌面極速視窗/螢幕側錄 (CLI Mode)

情境：在 Hyprland / Sway / GNOME Wayland 環境下，工程師想快速錄下 Zola 本地 Preview 網頁或特化視窗的 10 秒操作。

流程：
1. 使用者執行 `tapedeck run --wayland --window "Zola Server" -o preview.mp4`。
2. tapedeck 自動偵測環境變數並啟動 wf-recorder 或 PipeWire 抓取該視窗座標與音訊。
3. 錄製結束後自動觸發內部 ffmpeg 進行 H.264 / GIF 最佳化壓碼。

### 2.3 極客開發者的 TUI 腳本管理與快顯編輯 (TUI Mode)

情境：開發者手頭有幾十個專案的 .tape 錄影腳本，想快速檢視、修整與渲染。

流程：
1. 終端機直接輸入 `tapedeck`。
2. 彈出 fzf 風格雙欄面板，左側模糊搜尋 .tape 檔案，右側語法高亮預覽內容。
3. 按 `e` 鍵瞬移至 Vim/Neovim 編輯。
4. 按 `r` 直接背景觸發渲染。

---

## 3. 系統架構與 CLI / MCP 路由設計 (System Architecture)

```
Plaintext
┌───────────────────────────┐
│   CLI Entry (clap)        │
└─────────────┬─────────────┘
              │
    ┌─────────────────────────────────────────┐
    ▼                                         ▼
[ Subcommand: (None)/tui ]        [ Subcommand: mcp ]        [ Subcommand: run ]
    │                                         │                 │
    ▼                                         ▼                 ▼
┌────────────────────────┐   ┌────────────────────────┐   ┌────────────────────────┐
│ Ratatui + Nucleo Engine │   │ stdio JSON-RPC 2.0 Server│   │ Headless Runner Engine │
│ (fzf-like List + Editor) │   │ (MCP Protocol 2024-11-05)│   │ (Direct Execution)     │
└────────────┬─────────────┘   └────────────┬─────────────┘   └────────────┬─────────────┘
              │                              │                              │
              └──────────────────────────────┼──────────────────────────────┘
                                              │
                                              ▼
                                ┌─────────────────────────────┐
                                │ Recording Engine Router      │
                                └──────────────┬──────────────┘
                                               │
              ┌───────────────────────┴───────────────────────┐
              ▼                                               ▼
  ┌──────────────────────────┐                    ┌──────────────────────────┐
  │ Tape Engine (VHS/ttyd)   │                    │ Wayland/Display Engine   │
  │ • Output: GIF/WebM/MP4   │                    │ • wf-recorder / PipeWire │
  └──────────────────────────┘                    └──────────────────────────┘
```

### 3.1 CLI Command Routing

```bash
tapedeck                     # 預設啟動輕量 TUI 模式 (fzf + editor)
tapedeck tui                 # 同上，顯式啟動 TUI
tapedeck mcp                 # 啟動 stdio MCP Server (對接 OpenCode / Cursor)
tapedeck run demo.tape       # CLI 無頭跑 VHS Tape 渲染
tapedeck run --wayland       # CLI 觸發 Wayland 螢幕/視窗錄影
```

### 3.2 子命令分派邏輯

- **tui**：啟動 Ratatui + Nucleo 引擎，fzf 雙欄列表 + $EDITOR 整合。
- **mcp**：啟動 stdio JSON-RPC 2.0 Server，協助 OpenCode / Cursor。
- **run**：無頭跑 VHS Tape 或 Wayland 錄影；`--wayland` 啟動 Wayland 錄影管道。

---

## 4. 建議技術選型 (Technical Stack)

### 4.1 模組分工與選型

| 模組 | 選用技術 | 選型理由 |
|------|----------|----------|
| 語言與運行時 | Rust 2021 Edition | 零成本抽象、無 GC 延遲、單一 Binary 部署、極致輕量。非同步 Coretokio 生態，最成熟的非同步 runtime，負責子行程 (Process) 生命週期管理。 |
| CLI 路由解析 | clap (v4 with derive) | 標準化的命令列子指令 (Subcommand) 解析與 Help 生成。 |
| MCP Protocol | serde + serde_json | 自研輕量 stdio JSON-RPC 2.0 處理器，靈活相容 MCP 規範。 |
| TUI 繪製引擎 | ratatui + crossterm | Rust 生態最強大的 2D Terminal UI 繪製庫，極致高效。 |
| Fuzzy Search | nucleo | 由 Zed 團隊開發的超高速 Fuzzy Finder 引擎（fzf 的 Rust 絕佳替代品）。 |
| TUI 錄影底層 | Charm vhs CLI | 終端機錄影效果最好、社群生態最豐富的 Tape 引擎。 |
| Wayland 錄影底層 | wf-recorder + slurp | Wayland 生態原生最輕量、支援 PipeWire 與硬體編碼 (VAAPI/NVENC) 的錄影 CLI。 |
| 通用轉檔與底層 | ffmpeg | 跨平台 (macOS/X11/Windows) 備用錄影管道與 GIF 壓縮最佳化。 |
| 圖片預覽 | ratatui-image + image | ratatui-image 支援 Kitty/Sixel/iTerm2 及 ANSI 降級降級; image 圖片解碼與縮圖生成。 |

### 4.2 技術選型降級階梯 (Graceful Degradation)

```
Plaintext
┌─────────────────────────────────────────────────────────┐
│ Preview Strategy Ladder                                   │
│                                                          │
│  1. Kitty Graphics Protocol / Sixel (真彩色高畫質原生圖片) │
│  2. iTerm2 Inline Images Protocol                       │
│  3. chafa / tview (ANSI Block/ASCII 藝術降級渲染)        │
└─────────────────────────────────────────────────────────┘
```

**1. 高畫質原生協定**：Kitty Graphics Protocol & Sixel
- 機制：像 WezTerm, Kitty, Ghostty, Alacritty (搭配 Sixel) 等現代 Terminal，支援直接把 PNG/GIF 解碼後的像素繪製在 Terminal 緩衝區。
- Rust 庫：ratatui-image 庫（專為 Ratatui 設計），自動偵測當前 Terminal 協定（Kitty, Sixel, iTerm2），並將 .gif 或 .mp4 的第一幀（或預覽圖）直接以原像素畫在右側的 Preview Block 裡！

**2. 通用降級方案**：chafa (ANSI Block Art)
- 機制：若使用者用的是舊版 Terminal，底層可以 spawn chafa 或使用 Rust 內建的 ANSI block 轉換算法，把圖片/影片幀轉成 ANSI 色塊。
- 優點：相容性 100%，任何連 SSH 的 Terminal 都能秀出色彩豐富的微縮預覽圖。

**3. 影片/GIF 動態預覽 (Hover Playback)**
- 靜態縮圖：tapedeck 背景自動透過 ffmpeg -i demo.mp4 -vframes 1 preview.png 產生快取縮圖，滑到該項目時秒出畫面。
- 動態循環：若 Terminal 效能足夠，可抓取前 3 秒 frame，在 Ratatui 繪製迴圈（Loop）裡以 15~30 FPS 動態刷新 Preview 視窗！

### 4.3 Cargo.toml 依賴

```toml
[dependencies]
tokio = { version = "1.38", features = ["full"] }
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
ratatui = "0.26"
crossterm = "0.27"
nucleo = "0.5" # High-performance Fuzzy Search
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
ratatui-image = "1.0"  # 支援 Kitty, Sixel, iTerm2 及 ANSI 降級
image = "0.25"          # 圖片解碼與縮圖生成
```

---

## 5. MCP 介面規格 (MCP Server Specification)

tapedeck mcp 開啟後，會透過 stdio 提供以下符合 MCP Specification 的 Tools：

### 5.1 Tool 1: record_tui_tape

用途：傳入 VHS Tape 腳本，無頭生成高品質 TUI GIF / MP4。

**JSON Input Schema**：

```json
{
  "name": "record_tui_tape",
  "description": "Render a high-quality TUI demo GIF/MP4 using Charm VHS script engine.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "tape_content": {
        "type": "string",
        "description": "The full VHS tape script content (e.g. Set Theme 'dracula', Type 'blogger-agent', Sleep 1s)."
      },
      "output_path": {
        "type": "string",
        "description": "Output file path (e.g., 'assets/tui-demo.gif')."
      }
    },
    "required": ["tape_content", "output_path"]
  }
}
```

**說明**：`tape_content` 為完整的 VHS 腳本（Set Theme、Type、Sleep 等指令組合），`output_path` 為輸出檔案路徑。tapedeck 在背景以 headless 方式執行 vhs，將結果寫入 `assets/demo.gif`。

### 5.2 Tool 2: record_wayland_screen

用途：在 Wayland 環境下錄製全螢幕或特定視窗。

**JSON Input Schema**：

```json
{
  "name": "record_wayland_screen",
  "description": "Record Wayland desktop or specific window into MP4/GIF using wf-recorder.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "output_path": {
        "type": "string",
        "description": "Target output path (e.g., 'assets/desktop.mp4')."
      },
      "window_title": {
        "type": "string",
        "description": "Optional window title/app_id to capture (e.g. 'Zola', 'BloggerAgent')."
      },
      "duration_seconds": {
        "type": "integer",
        "description": "Duration to record in seconds."
      }
    },
    "required": ["output_path", "duration_seconds"]
  }
}
```

**說明**：`output_path` 為目標輸出檔案路徑（可為 `.mp4` 或 `.gif`），`window_title` 為可選的視窗名稱，`duration_seconds` 為錄製秒數。tapedeck 自動偵測環境變數並啟動 wf-recorder 或 PipeWire 抓取該視窗座標與音訊。錄製結束後自動觸發內部 ffmpeg 進行 H.264 / GIF 最佳化壓碼。

---

## 6. Rust 專案骨架與核心實作 (Code Skeleton)

### 6.1 Cargo.toml

```toml
[package]
name = "tapedeck"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.38", features = ["full"] }
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
ratatui = "0.26"
crossterm = "0.27"
nucleo = "0.5" # High-performance Fuzzy Search
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
ratatui-image = "1.0"
image = "0.25"
```

### 6.2 專案目錄結構

```
tapedeck/
├── Cargo.toml
└── src/
    ├── main.rs                 # CLI 進入點與模式派發
    ├── cli.rs                  # Clap 結構定義
    ├── mcp/
    │   ├── mod.rs              # stdio 訊息 loop
    │   └── tools.rs            # MCP Tools JSON 宣告與執行邏輯
    ├── tui/
    │   ├── mod.rs              # Ratatui Event Loop
    │   ├── app.rs              # TUI State (fzf List, Active File)
    │   └── ui.rs               # fzf 雙欄 Layout 繪製
    └── engine/
        ├── mod.rs              # 錄影 Engine 抽象
        ├── vhs.rs              # VHS Tape Process 管理
        └── wayland.rs          # wf-recorder / PipeWire 控制器
```

### 6.3 核心進入點 (src/main.rs)

```rust
mod cli;
mod engine;
mod mcp;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Mcp) => {
            // 啟動 MCP Server (stdio)
            mcp::run_server().await?;
        }
        Some(Commands::Run { tape, output, wayland }) => {
            if *wayland {
                engine::wayland::record_screen(output, 5).await?;
            } else if let Some(tape_path) = tape {
                engine::vhs::run_tape_file(tape_path, output).await?;
            }
        }
        Some(Commands::Tui) | None => {
            // 預設直接敲 `tapedeck` 打開 fzf 風格 TUI
            tui::run_app().await?;
        }
    }

    Ok(())
}
```

---

## 7. 專案開發里程碑 (Milestones)

### Phase 1: Minimal MCP & VHS Engine
- 完成 tapedeck mcp stdio JSON-RPC 骨架。
- 實作 `record_tui_tape`，確認 OpenCode / Cursor 能調用 vhs 並落檔 .gif。

### Phase 2: Light-weight fzf TUI
- 實作 tapedeck tui，支援 .tape 清單檢視、nucleo 模糊搜尋、按 e 調用 $EDITOR。

### Phase 3: Wayland Integration
- 實作 `engine/wayland.rs`，整合 wf-recorder 與視窗座標擷取，擴充 `record_wayland_screen` MCP Tool。

### Phase 4: Open Source & Packaging
- 撰寫完整 README (附帶由 tapedeck 自己錄製自己的 Demo GIF)。
- 上架至 crates.io 與 GitHub Release。

---

## 8. Preview Pane 升級方案 (Preview Pane)

### 8.1 背景規格

在 fzf 的 Pane 裡加上圖片/影片預覽（Preview Pane），使用者在切換選單時就不只是看 .tape 的純文字腳本，而是能在 Terminal 裡面直覺看到渲染出來的 GIF/MP4 成果。

### 8.2 升級後的 TUI Layout (fzf + Media Preview)

```
┌─ TAPEDECK v0.1.0 ──────────────────────────────────────────────────┐
│ > 01_                                                             │
│   01_tui_demo.tape       │ ┌─ Preview: assets/01_tui_demo.gif ──┐ │
│ > 02_blogger_sync.tape   │ │                                    │ │
│   03_zola_publish.mp4    │ │  [ █ Kitty/Sixel Rendered GIF ]    │ │
│                          │ │  $ blogger-agent                   │ │
│                          │ │  ❯ [Draft] Go TUI Design System    │ │
│                          │ │                                    │ │
│                          │ └────────────────────────────────────┘ │
│                          │ File: 2.4MB | Res: 1200x800 | 60 FPS   │
├───────────────────────────────────────────────────────────────────┤
│ [↑/↓] Select  [/] Filter  [e] Edit  [o] Open External  [q] Quit   │
└───────────────────────────────────────────────────────────────────┘
```

### 8.3 快捷開啟 (Open External)

按 `o` 鍵：
- macOS: `tokio::process::Command::new("open").arg(&file_path)`
- Linux (Wayland/X11): `tokio::process::Command::new("xdg-open").arg(&file_path)` (呼叫 mpv, vlc 或 Imv)
- Windows: `cmd /c start &file_path`

### 8.4 內容規格

- 當游標指到 .tape 檔案 ➔ 右側顯示 .tape 腳本文字語法高亮。
- 當游標指到 .gif / .mp4 / .webm 檔案 ➔ 右側切換為 ratatui-image 媒體預覽模組，並顯示檔案大小與解析度等 Meta 資訊。
- 當選擇 [o] 鍵 ➔ 用 xdg-open / open 彈出系統原生的 MPV / 圖片檢視器觀看全畫質影片。

### 8.5 圖片預覽支援

```toml
[dependencies]
ratatui-image = "1.0"  # 支援 Kitty, Sixel, iTerm2 及 ANSI 降級
image = "0.25"         # 圖片解碼與縮圖生成
```

---

## 9. 補充規格到 tapedeck 規格書

### 9.1 Cargo.toml 圖片預覽支援

```toml
[dependencies]
ratatui-image = "1.0"  # 支援 Kitty, Sixel, iTerm2 及 ANSI 降級
image = "0.25"         # 圖片解碼與縮圖生成
```

### 9.2 TUI 邏輯新增

- 當游標指到 .tape 檔案 ➔ 右側顯示 .tape 腳本文字語法高亮。
- 當游標指到 .gif / .mp4 / .webm 檔案 ➔ 右側切換為 ratatui-image 媒體預覽模組，並顯示檔案大小與解析度等 Meta 資訊。
- 按 `o` 鍵 ➔ 用 xdg-open / open 彈出系統原生的 MPV / 圖片檢視器觀看全畫質影片。

---

## 10. 跨版本互操作性

### 10.1 與已存版本同步

- `ratatui-image` 1.0：相容版本 0.26 之 Ratatui。
- `image` 0.25：相容版本 0.26 之 Ratatui。
- `nucleo` 0.5：相容 Zed 團隊最新架構。

### 10.2 互操作門檻

- MCP 2024-11-05 規範：`serde` + `serde_json` 為基礎。
- Wayland 錄影管道：`wf-recorder` 為底層。
- 圖片預覽：`ratatui-image` + `image` 為互操作層。

---

*文件最終更新：2025-08-07 | 版本：0.1.0*
