# tapedeck 技術規格書 (OpenSpec 規範)

## 1. 總覽

tapedeck 是一個專為 Linux 環境設計的双模媒體錄影工具，提供 CLI/TUI 介面與宣告式腳本語言（.tape for TUI、.roll for GUI），支援硬體加速編碼（AV1/VP9）與 XDG 規範儲存。

---

## 2. 系統架構

```plaintext
┌──────────────────────────────┐
│ User Interface              │
├──────────────────────────────┤
│ CLI (clap)          TUI (ratatui)│
├──────────────────────────────┤
│ Core Pipeline               │
│ ┌─────────────────────┐     │
│ │ Script Parser       │◄──┐ │
│ │ (.tape/.roll)       │   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Engine Router       │   │ │
│ │ (VHS Web / Native)  │   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Recorder Executor   │   │ │
│ │ (wf-recorder/ffmpeg)│   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Asset Manager       │   │ │
│ │ (SQLite + XDG)      │   │ │
│ └─────────────────────┘   │ │
└──────────────────────────────┘
```

---

## 3. 模組詳細設計

### 3.1 CLI 引擎

**指令結構**（使用 clap derive）：

```bash
tapedeck run SCRIPT_FILE [--output PATH] [--fps FPS] [--max-size MB] [--gif|--webp]
tapedeck link MEDIA_FILE [--format zola|md|html]
tapedeck optimize INPUT -o OUTPUT [--quality 1-100]
tapedeck clean [--dry-run]
```

**錯誤處理**：
- 檔案不存在 → 回傳 `Err(FileNotFound)`
- 編碼失敗 → 自動降級至軟體編碼並警告

---

### 3.2 TUI 導播台

**布局**（ratatui + fzf）：

```plaintext
┌───────────────────────────────────────────────────────────────┐
│ TAPEDECK v0.1.0 [REC 00:00] [WAYLAND]                        │
│ > 01_demo.tape                                               │
│   01_demo.gif │ ┌────────────── Preview ────────────┐       │
│               │ │ [Sixel/Kitty Rendered Preview]   │       │
│               │ └───────────────────────────────────┘       │
│               │ Type: GIF | Size: 2.4MB | Linked: 2x       │
├───────────────────────────────────────────────────────────────┤
│ [/] Filter [e] Edit [r] Run [y] CopyMD [o] Open [c] Clean      │
└───────────────────────────────────────────────────────────────┘
```

**快捷鍵**：
- `e`：編輯腳本
- `r`：背景執行
- `y`：複製 Markdown 語法
- `o`：系統預設播放器開啟
- `c`：標記孤兒資產

---

### 3.3 腳本語言規格

#### .tape (TUI 模式)

```yaml
# demo.tape
Set Engine Auto
Set Output "assets/demo.webm"
Set FPS 60

Type "cargo run"
Sleep 500ms
Key Enter
```

#### .roll (GUI 模式)

```yaml
# gui_demo.roll
Title "Obsidian Automation"
Engine Native
TargetWindow "Obsidian"

ExecBefore "obsidian"
WaitWindow "Obsidian" timeout=10s

MouseMove 500 300 speed=smooth
Click Left
Type "New note"
Sleep 300ms
Shortcut "Ctrl+S"
```

---

## 4. 效能與限制

| 指標                | 目標值                  |
|---------------------|------------------------|
| 錄影延遲            | < 100ms (硬體編碼)     |
| 最大支援解析度      | 3840x2160 (4K)         |
| 孤兒資產掃描速度    | < 1s (SQLite 增量索引)  |
| TUI 渲染 FPS        | ≥ 60 FPS               |

---

## 5. 安全性與隱私

- **XDG 規範**：配置檔與快取不污染 `$HOME`
- **權限隔離**：錄製流程以使用者權限執行
- **資料庫加密**：tapedeck.db 未來支援 SQLCipher

---

## 6. 未來擴展

1. **macOS/Windows 支援**（開放社群貢獻）
2. **雲端同步**（透過 MCP 協議）
3. **AI 導播**（自動偵測畫面變化優化影格率）

---

## 7. 驗證標準

- **單元測試**：覆蓋核心邏輯（引擎選擇、腳本解析）
- **E2E 測試**：使用 .tape/.roll 腳本驗證錄製流程
- **效能基準**：每月執行 `cargo criterion` 比較編碼速度

---

## 附錄 A：錯誤碼表

| 代碼      | 意義                     | 處理策略                  |
|-----------|--------------------------|---------------------------|
| E001      | 腳本檔案不存在           | 終止並顯示檔案路徑        |
| E002      | 硬體編碼失敗             | 降級至軟體編碼並警告      |
| E003      | 視窗定位失敗             | 中止錄製並建議手動模式    |

---

## 附錄 B：Benchmark 工具

```bash
# 執行所有基準測試
cargo bench -- --output-format bencher
```