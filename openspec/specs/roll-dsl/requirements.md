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

## 非目標（Non-Goals）

- 不支援 YAML 格式 .rec（見 docs/DSL_Concept.md，另議）
- 不實作滑鼠座標的螢幕解析度感知（speed=smooth 僅記錄、目前不影響行為）
- 不實作 TUI 介面的腳本編輯功能
