# 🎩 腳本語言指南

## 總覽

tapedeck 支援兩種宣告式腳本語言：

- **.tape**：專為 TUI/CLI 環境設計，純文字操作
- **.roll**：針對 GUI 應用程式自動化，含滑鼠/視窗控制

---

## .tape 語法 (.tape 檔案)

### 核心指令

```yaml
Set Engine <Auto|VHS|Native>
Set Output "path/to/file.webm"
Set FPS 60

Type "text"
Sleep 500ms
Key <Up|Down|Enter>
```

### 範例：終端機應用錄製

```yaml
# examples/tui_demo.tape
Set Engine Auto
Set Output "assets/demo.webm"

Type "cargo run"
Sleep 500ms
Key Enter
Key Down 3
```

---

## .roll 語法 (.roll 檔案)

### 核心指令

```yaml
Title "Script Name"
ExecBefore "command"
WaitWindow "Window Title" timeout=10s
TargetWindow "Window Name"
MouseMove x y speed=smooth
Click Left
Type "text"
Shortcut "Ctrl+S"
```

### 範例：Obsidian 自動化

```yaml
# examples/gui_demo.roll
Title "Obsidian Demo"
ExecBefore "obsidian"
WaitWindow "Obsidian"

MouseMove 500 300
Click Left
Type "New note"
Shortcut "Ctrl+S"
```

---

## 進階功能

### 錯誤處理

```yaml
# .roll 腳本
WaitWindow "App" timeout=10s on_fail=abort
```

### 硬體編碼優化

```yaml
Optimize AV1 encoder=av1_vaapi
```

---

## 調試技巧

1. 啟用調試模式：`tapedeck run --verbose script.tape`
2. 逐步執行：在指令前加入 `DebugSleep 5s`

---

參考完整規格：[Spec.md](/Spec.md#腳本語言規格)