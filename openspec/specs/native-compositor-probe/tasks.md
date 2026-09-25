# tasks.md — native-compositor-probe

> 執行者：code agent（zero）。每個 Task 一個 commit，順序執行。
> 驗證基準：`CARGO_HOME=$HOME/.cargo cargo test --offline` 全綠＋`cargo fmt`/`clippy` 過。

## T1 — CompositorError 分類＋IPC 快速失敗（`feat(engine): compositor 錯誤分類，IPC 快速失敗`）

1. `compositor.rs` 新增錯誤分類（enum 或可辨識訊息），`NiriCompositor` /
   `SwayCompositor` 的 `find_window_geometry` 對「IPC 層失敗」（io error、非零
   exit、無法connect）與「查無視窗」分開回報；IPC 錯誤訊息需含根因（io error
   描述或 stderr 尾段）
2. `dispatcher.rs` WaitWindow 輪詢（313-326）：IPC 錯誤立即 bail（不重試），
   訊息含根因；只有「查無視窗」進入 200ms 重試；逾時訊息維持現行結構
3. 單元測試：IPC 錯誤不重試（mock）、Unknown 逾時路徑不變

## T2 — detect_compositor 探針化（`feat(engine): compositor 偵測改 socket 探針`）

1. `detect_compositor()`：字串比對保留為第一優先；字串無法判定時對
   `$XDG_RUNTIME_DIR` 做 socket 探針（niri / niri-*.sock / sway-ipc.<uid>.sock，
   實際名稱以 T1 觀察為準——需在 task 說明中記錄實際觀察）
2. 皆未命中 → `bail!` 含 `XDG_CURRENT_DESKTOP` 現值（**移除 niri fallback**）
3. 偵測函式拆純函式（socket 判定可注入目錄）以利測試；tempdir 假 socket 測試
   ：空目錄 → Err、有 niri socket → Niri、有 sway socket → Sway
4. `doctor.rs` 新增 compositor 檢查項（呼叫 detect_compositor，失敗顯示 ❌ 原因）

## T3 — e2e 驗收（Hermes host 端執行，非 repo commit）

1. cybertron（umbriel）：`env WAYLAND_DISPLAY=wayland-0 XDG_RUNTIME_DIR=/run/user/1000
   tapedeck run <gui .roll>` → 明確「無法偵測 compositor」錯誤、exit 1、非
   WaitWindow 逾時
2. `tapedeck doctor` → compositor 檢查項顯示偵測結果與原因
3. 無 WAYLAND 變數環境（預設 Hermes shell）→ 同樣明確錯誤，不誤導
4. 報告寫入 ticket 交付區

## 風險與回退

- 若 niri/sway socket 檔名與假設不符 → 以實機 `ls $XDG_RUNTIME_DIR` 觀察為準，
  樣式寫進測試
- 移除 niri fallback 可能影響「沒設 XDG_CURRENT_DESKTOP 但真在跑 niri」的邊界
  使用者 → socket 探針已覆蓋此情境（niri 一定有 socket）
