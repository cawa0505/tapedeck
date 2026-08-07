# tapedeck: 跨平台腳本驅動媒體錄影器

## 📌 專案簡介

tapedeck 是一個專為工程師、AI Agent 和技術內容創作者設計的 **雙模錄影引擎**，支援兩種腳本自動化模式：

1. **TUI 文字模式 (.tape)**：
   - 純終端機環境（Headless CI）
   - 像素完美 xterm.js 渲染
   - 鍵盤/指令模擬（如 `cargo run` + 自動輸入）

2. **GUI 原生模式 (.roll)**：
   - 真實桌面視窗側錄（Wayland/X11）
   - 滑鼠/鍵盤注入（如開啟 App → 點擊按鈕）
   - 硬體加速 AV1/VP9 編碼

## 🛠️ 核心功能

- **智慧引擎調度**：自動選擇最佳後端（VHS Web 或 Native PTY）
- **硬體探針**：偵測 GPU AV1 硬體，自動啟用零負擔編碼
- **腳本語言**：宣告式 YAML/TOML 語法定義錄影流程
- **TUI 雙欄導播台**：fzf 選單 + Sixel 即時預覽
- **資產追蹤**：SQLite 關聯媒體檔與 Markdown 引用
- **孤兒資產清理**：一鍵刪除無引用的大檔案

## 🚀 安裝與使用

```bash
# 安裝（需 Rust + cargo）
cargo install tapedeck

# 執行 TUI 模式（.tape 腳本）
tapedeck run demo_tui.tape

# 執行 GUI 模式（.roll 腳本）
tapedeck run --native demo_gui.roll
```

## 📜 授權

本專案採用 **MIT License**。

---

🔧 **貢獻歡迎**：提交 PR 前請先開 Issue 討論。