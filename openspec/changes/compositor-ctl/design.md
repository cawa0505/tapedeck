# design.md — compositor-ctl v0（DRAFT）

## 模組配置

- `src/engine/wayland/probe.rs`：classify() 加入 umbriel socket 規則 `umbriel-<name>.sock`（名稱含 display name，實作掃目錄 prefix match；抓到的 display 即 WAYLAND_DISPLAY 同值，因 socket 名取自 wayland socket name）。
- `src/engine/wayland/compositor.rs`：`UmbrielCompositor` 新 impl；`detect_compositor()` 選擇序：niri → sway → umbriel。
- `src/engine/wayland/umbriel/`（或單檔 umbriel.rs，視行數 ≤400 決定）：
  - `fn list_windows(env) -> Result<Vec<UmbrielWindow>>`：spawn `umbriel windows --json`，serde 解析。
  - geometry：回 (x,y,w,h)。注意 tiled 的 x/y 是 layout slot 座標（workspace 內），w/h 是 content box——與 niri 行為對齊比照。
  - `move_to_workspace(id, ws)`：先 `umbriel msg window-focus:<id>`，成功後 `umbriel msg window-move-to-workspace:<ws>`；第二步失敗保留錯誤根因。
- JSON 解析沿用 serde_json（專案已有）。

## 關鍵設計決策

1. **CLI spawn 而非直連 socket（v0）**：CLI 已處理連線、錯誤訊息、--json 格式；直連省下的開銷對錄製（秒級）不敏感。事件流（v1）才直連。
2. **focus-then-move 兩步法**：上游無 by-id move。（替代：請上游改——記為 umbriel repo 建議票，非阻塞。）
3. **detect 順序 niri → sway → umbriel**：真 niri 機（arhat/wheeljack 優先吃原生支援；umbriel 是 cybertron 在地實作。
4. **誠實條款**：umbriel 上錄製走 zwlr_screencopy 擷取＋CLI 控制，是完整原生路徑；不再如有「″標 niri/sway 環境不支援″」的誤報。

## 測試

- 單元：umbriel windows JSON fixture 解析（真機實測樣本）、probe classify umbriel 情境、兩步法失敗穿隧（第一步失敗/第二步失敗）。
- e2e：cybertron umbriel session 實錄。
