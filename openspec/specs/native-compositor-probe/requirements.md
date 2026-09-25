# requirements.md — native-compositor-probe

## REQ-1：偵測改為「先變數、後 IPC socket 探針」

`detect_compositor()` 判定順序：

1. env 字串比對（現行邏輯，保留）：
   - `XDG_CURRENT_DESKTOP` 或 `WAYLAND_DISPLAY` 含 "niri" → Niri
   - 含 "sway" → Sway
2. env 無法判定時 → **socket 探針**（`$XDG_RUNTIME_DIR` 下）：
   - `niri` 或 `niri-<display>.sock` 存在 → Niri
   - `sway-ipc.<uid>.sock` 存在 → Sway
   - 都不存在 → `Err`（**不再 niri fallback**）

單元測試用臨時目錄假 socket 驗證（不得依賴真 session）。

## REQ-2：ErrorMessage 歸因分離

- IPC 失敗（niri msg / swaymsg io error、socket 不存在）→ 回報「compositor IPC
  不可用（<根因>）」，**不得**包裝成「視窗未出現」
- IPC 正常但視窗未出現 → 維持現行「WaitWindow 逾時：視窗「X」未出現」
- impl 細節：`find_window_geometry` 回傳的 Err 分兩類（IPC 層 vs 查無視窗層），
  WaitWindow 輪詢只重試「查無視窗」，IPC 層錯誤立即 bail 並保留根因

## REQ-3：doctor 顯示 compositor 偵測

- `tapedeck doctor` 新增「compositor」檢查項：偵測結果（Niri/Sway/不可用＋session
  變數現值）、socket 存在與否
- doctor 不得因偵測失敗而 crash（顯示 ❌＋原因即可）

## 驗收場景（隨驗收 ticket 更新）

- SCN-1：niri session（真機或模擬）→ 偵測 Niri、GUI roll 走 wf-recorder 照舊
- SCN-2：umbriel session（cybertron 現況）→ 明確錯誤訊息（含 session 變數）、exit 1
- SCN-3：無任何 wayland session 變數（Hermes shell）→ 明確錯誤「無法偵測
  compositor」，不誤導
