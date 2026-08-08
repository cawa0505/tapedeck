# 🎩 腳本語言教學 (.roll)

> 語法規格以 [openspec/specs/roll-dsl/](../openspec/specs/roll-dsl/) 為唯一依據，本文件為教學導覽。

## 總覽

`.roll` 是 tapedeck 唯一的腳本語言，採用兩層設計：

- **輸入層**：`Type` / `Enter` / `Key` / `Sleep` / `MouseMove` / `Click` — 描述錄製過程的操作
- **自動化層**：`ExecBefore` / `WaitWindow` / `Roll` / `Optimize` 等 — 由 tapedeck 執行的錄製前後處理

執行後端由引擎決定：
- `Set Engine Auto` → 自動偵測（TUI 腳本走 vhs、GUI 腳本走原生錄製）
- `Set Engine VHS` → 終端機錄製（轉譯為 vhs 的 .tape 格式執行）
- `Set Engine Native` → 視窗錄製（niri/sway + wf-recorder）

## 基本範例：TUI 錄製

```
# tui_demo.roll
Set Engine Auto
Set Output "assets/tui_demo.gif"
Set Framerate 15

Type "echo Hello Tapedeck"
Enter
Sleep 1s
```

執行：`tapedeck run tui_demo.roll`

## 基本範例：GUI 錄製

```
# gui_demo.roll
Title "Obsidian 自動化"
Set Engine Native
Set Output "assets/obsidian_demo.webm"

ExecBefore "obsidian"
WaitWindow "Obsidian" timeout=10s
TargetWindow "Obsidian"

Roll 15s
MouseMove 500 300 speed=smooth
Click Left
Type "New note for tapedeck demo"
Shortcut "Ctrl+S"

ExecAfter "pkill obsidian"
Optimize AV1 encoder=av1_vaapi
```

## 指令速查

### 輸入層（錄製操作）

| 指令 | 說明 | 範例 |
|------|------|------|
| `Type "文字"` | 輸入文字 | `Type "cargo run"` |
| `Enter` | 按下 Enter | `Enter` |
| `Key <按鍵> [次數]` | 按指定鍵（Down/Up/Enter/Tab/q...） | `Key Down 3`、`Key Tab` |
| `Sleep 500ms` / `Sleep 1s` | 等待 | `Sleep 300ms` |
| `MouseMove x y [speed=smooth]` | 移動滑鼠 | `MouseMove 250 180 speed=smooth` |
| `Click <Left\|Right\|Middle>` | 點擊 | `Click Left` |
| `Shortcut "Ctrl+S"` | 組合鍵 | `Shortcut "Ctrl+S"` |

### 自動化層（tapedeck 執行）

| 指令 | 說明 |
|------|------|
| `ExecBefore "指令"` / `ExecAfter "指令"` | 錄製前/後執行 shell 指令 |
| `WaitWindow "標題" timeout=10s` | 等待視窗出現（逾時則中止） |
| `TargetWindow "標題"` | 鎖定要錄製的視窗 |
| `WindowSize 寬 高` | 設定視窗尺寸 |
| `Padding 20` | 錄製區域內縮 |
| `Roll 15s` | 錄製時長 |
| `Optimize <codec> [encoder=...]` | 錄製後轉換/優化 |

### 設定指令

| 指令 | 說明 |
|------|------|
| `Set Engine <Auto\|VHS\|Native>` | 選擇執行引擎 |
| `Set Output "路徑"` | 輸出檔路徑（副檔名決定格式） |
| `Set Framerate 15` | 影格率（vhs 用 `Set Framerate`，非 `Set FPS`） |
| `Set Shell "bash"` | 終端機 shell（vhs 後端用） |

舊寫法相容：`Mode TUI`（= Set Engine Auto）、`Output "x.gif"`、`FPS 15`、`Terminal "kitty"` 仍可解析。

## 實作狀態

- [x] parser 基礎（Type/Enter/Sleep/Output/FPS）
- [ ] 完整語法（自動化層）— 見 [tasks.md](../openspec/specs/roll-dsl/tasks.md)

## 調試技巧

1. 先 dry-run 確認解析與引擎選擇：`tapedeck run --dry-run script.roll`
2. 錯誤訊息會指出腳本行號與失敗原因
