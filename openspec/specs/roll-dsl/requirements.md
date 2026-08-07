# requirements.md — .roll DSL 需求規格

## REQ-1：語法以 examples 新寫法為正式語法

- REQ-1.1 `Set` 系列指令：`Set Engine <Auto|VHS|Native>`、`Set Output "<path>"`、`Set FPS <n>`
- REQ-1.2 `Key` 指令支援具名按鍵：`Key Down`、`Key Up`、`Key Enter`、`Key Tab`、`Key q`（單一字母直接對應字元）；支援選擇性計數 `Key Down 3`（按 3 次）
- REQ-1.3 自動化指令：`ExecBefore "<cmd>"`、`ExecAfter "<cmd>"`、`WaitWindow "<title>" timeout=<Ns|Nms>`、`TargetWindow "<title>"`、`WindowSize <w> <h>`、`Padding <n>`、`Roll <Ns>`、`Shortcut "<Ctrl+X>"`、`Optimize <codec> [key=value...]`
- REQ-1.4 輸入指令：`Type "<text>"`、`Sleep <Nms|Ns>`、`MouseMove <x> <y> [speed=smooth]`、`Click <Left|Right|Middle>`
- REQ-1.5 註解以 `#` 開頭；指令大小寫敏感（沿用現有慣例）

## REQ-2：舊寫法相容別名

- REQ-2.1 `Title "<name>"` → 保留（metadata，無對應 Set）
- REQ-2.2 `Mode TUI|GUI` → `Set Engine` 的相容別名：`Mode TUI` ⇒ `Engine Auto`（TUI 意圖）、`Mode GUI` ⇒ `Engine Native`
- REQ-2.3 `Output "<path>"` → `Set Output "<path>"` 的別名
- REQ-2.4 `FPS <n>` → `Set FPS <n>` 的別名
- REQ-2.5 `Terminal "<cmd>"` → `Set Shell "<cmd>"` 的別名（若 vhs 支援）
- REQ-2.6 `Enter`（無參數）→ 視為 `Key Enter`
- REQ-2.7 舊寫法的 `Key Down N`/`Key Up N` → 新寫法 `Key Down N`/`Key Up N`（已由 REQ-1.2 的計數語法涵蓋）

## REQ-3：parser 必須能解析全部 examples

- REQ-3.1 `examples/test_tui.roll`、`examples/tui_zago.roll`、`examples/gui_demo.roll` 三者皆可解析
- REQ-3.2 解析錯誤須回報行號與支援的指令清單（沿用現有錯誤格式）

## REQ-4：雙層執行

- REQ-4.1 vhs 轉譯層：`Type`/`Enter`/`Key`/`Sleep`/`MouseMove`/`Click`/`Roll` 轉成 VHS DSL 指令（`Set Framerate` 而非 `Set FPS`，`MouseClick` 而非 `Click`，`Sleep Nms|Ns` 格式）
- REQ-4.2 tapedeck 自動化層：
  - `ExecBefore`：錄製前以 sh -c 執行，失敗即中止
  - `ExecAfter`：錄製後執行，失敗僅警告
  - `WaitWindow`：輪詢 compositor 直到視窗出現或逾時（預設 timeout=10s）
  - `TargetWindow`：指定 wf-recorder 錄製的視窗
  - `WindowSize`：設定錄製視窗尺寸
  - `Padding`：視窗幾何外擴像素（傳給 wf-recorder -g）
  - `Roll Ns`：錄製時長，結束後停止 wf-recorder
  - `Shortcut`：送出組合鍵（GUI 模式）
  - `Optimize`：錄製後優化（encoder 參數傳給 ffmpeg）
- REQ-4.3 `Set Engine Auto`：依環境自動選擇 — 有 niri/sway 用 Native，否則 vhs

## REQ-5：dry-run 驗證

- REQ-5.1 `tapedeck run --dry-run <script>` 顯示：解析後的 engine 選擇、輸出、fps、指令摘要
- REQ-5.2 dry-run 不執行任何外部工具

## REQ-6：XDG 路徑（必要條件）

- REQ-6.1 **輸出路徑**：腳本中**相對**輸出解析至 `$XDG_CACHE_HOME/tapedeck/`（未設定 `XDG_CACHE_HOME` 時用 `$HOME/.cache/tapedeck/`）；**絕對**輸出與 CLI `--output` 覆寫照原樣使用
- REQ-6.2 目標目錄不存在時自動建立（`fs::create_dir_all`）
- REQ-6.3 dry-run 與實際執行顯示**解析後的絕對輸出路徑**
- REQ-6.4 測試/範例錄製產物一律落於 XDG cache，不得寫入 repo 目錄（禁止 CWD 出現 .gif/.webm）
- REQ-6.5 **config 路徑**：`$XDG_CONFIG_HOME/tapedeck/config.toml`（未設定時 `$HOME/.config/tapedeck/config.toml`）；不存在則以預設值執行並提示，**首次執行自動建立預設設定檔**（含註解範例）
- REQ-6.6 **state 路徑**：`$XDG_STATE_HOME/tapedeck/`（未設定時 `$HOME/.local/state/tapedeck/`），用於 SQLite DB 與錄製歷程

## SCN-1：TUI 錄製（vhs 後端）

輸入 `tapedeck run examples/test_tui.roll`（或 tui_zago.roll），engine=Auto→vhs：
1. 解析腳本
2. 執行 ExecBefore（若有）
3. 轉譯 Type/Enter/Sleep 為 VHS DSL，呼叫 vhs
4. 執行 ExecAfter（若有）
5. 輸出 gif/webm

## SCN-2：GUI 錄製（Native 後端）

輸入 `tapedeck run examples/gui_demo.roll`，engine=Native：
1. 解析腳本
2. detect_compositor() 偵測 niri/sway
3. 執行 ExecBefore（啟動 obsidian）
4. WaitWindow 輪詢直到視窗出現
5. TargetWindow + Padding 產生 wf-recorder -g 參數
6. wf-recorder 錄製 Roll 15s
7. ExecAfter（pkill obsidian）、Optimize（AV1 vaapi）

## REQ-7：vhs 指令全集支援（轉譯層）— 已定案

> 決策（2026-08-08）：MVP 優先支援**所有 .tape 原語法**（vhs 指令全集）+ tapedeck 擴充指令（REQ-1/4）。文件宣稱的 `delay`/`Scroll`/`Engine Wayland` 等未來不確定項目**不納入**，parser 保留擴充餘裕但不預先實作。

- REQ-7.1 parser（轉譯層）必須能解析下列 vhs 全集指令（來源：本機 `vhs manual`），使 .roll 可透寫任何 vhs 指令：
  - `Output <path>`（.gif/.webm/.mp4）、`Require <program>`、`Set <setting> <value>`、`Sleep <time>`
  - `Type "<string>"`、`Ctrl [+Alt][+Shift]+<char>`、`Alt+<key>`、`Escape`、`Space [repeat]`
  - `Backspace [repeat]`、`Delete [repeat]`、`Insert [repeat]`
  - `Down/Enter/Left/Right/Tab/Up/PageUp/PageDown/ScrollUp/ScrollDown [repeat]`
  - `Hide`、`Show`、`Wait[+Screen][@<timeout>] /<regexp>/`、`Source <path>.tape`
  - `Screenshot <path>.png`、`Copy "<string>"`、`Paste`
- REQ-7.2 vhs 全集指令在 .roll 中**原樣轉譯**至 .tape（tapedeck 不做語意處理），僅 tapedeck 擴充指令（REQ-1.3）由自動化層處理
- REQ-7.3 **不納入**（維持定案語法）：`Engine Wayland` 別名、`Type "..." delay=Nms`、`Scroll Down N`、`Optimize WebM` 容器先決形式。此類語法未來若確定需求，另行開 change-set
- REQ-7.4 `Optimize` 維持 `Optimize <codec> [key=value...]` 形式（codec 先決）；容器由 `Output` 副檔名決定

## 非目標（Non-Goals）

- 不支援 YAML 格式 .rec（過時概念，已從 docs/ 移除）
- 不實作滑鼠座標的螢幕解析度感知（speed=smooth 目前僅記錄、不影響行為；Bézier 插補待 OQ-02 決策）
- 不實作 TUI 介面的腳本編輯功能（OQ-06 待決）
