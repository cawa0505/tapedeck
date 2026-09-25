# design.md — native-compositor-probe

## 1. 現況與腳手架

`src/engine/wayland/compositor.rs`（414 行）已有：

- `trait Compositor`（find_window_geometry / window_on_output / move_to_workspace）
- `NiriCompositor`（niri msg --json windows / outputs）
- `SwayCompositor`（swaymsg -t get_tree -r）
- `detect_compositor()`（280-296）：字串比對 → fallback Niri

`NativeEngine::record()`（dispatcher.rs:289-336）：

- `detect_compositor()` → 找 target（TargetWindow > WaitWindow > ""）
- WaitWindow 輪詢（313-326）：`compositor.find_window_geometry(target)` 每 200ms
  一次，任何 Err 都當「還沒出現」繼續等，deadline 過了 bail「視窗未出現」
- 之后 `window_on_output()` → `to_wf_recorder_arg(padding)`

## 2. 錯誤分類（核心設計）

`Compositor::find_window_geometry` 回傳 `anyhow::Result` 無法分類。改為連結
error type，**不在 trait 上改動**（避免动 sway/niri impl 的方法签名）——
把「IPC 層」與「查無視窗無視窗層」錯誤訊息加上可辨識前綴，WaitWindow 以
`anyhow::Error::downcast_ref` 或字串前綴判別：

```rust
// compositor.rs
#[derive(Debug, thiserror::Error)]  // repo 無 thiserror 則用 anyhow 靜態字串
pub enum CompositorError {
    #[error("compositor IPC 不可用：{0}")]
    Ipc(String),          // io error / 非零 exit / stderr 初步
    #[error("查無視窗：{0}")]
    NotFound(String),     // 確實查了但沒有符合 target
}
```

WaitWindow 輪詢邏輯（dispatcher.rs:313-326）改為：

```rust
loop {
    match compositor.find_window_geometry(target) {
        Ok(g) => break g,
        Err(e) if is_ipc_error(&e) => bail!("{e}"),          // 立即失敗，不一輪詢
        Err(_) if Instant::now() < deadline => sleep(200ms),  // 只有 NotFound 重試
        Err(e) => bail!("WaitWindow 逾時：視窗「{target}」未出現\n提示：…"),
    }
}
```

> 實作者可自行斟酌用 enum downcast 或 bonus 訊息判別；關鍵是（a）IPC 錯誤
> 不得進入重試迴圈（b）最終 bail 訊息必含根因。

## 3. detect_compositor() 簽法

```rust
pub fn detect_compositor() -> Result<Box<dyn Compositor>> {
    // 1. 字串（現行）
    if env contains niri  → Niri
    if env contains sway  → Sway
    // 2. socket 探針（XDG_RUNTIME_DIR 必存在，否則視為不在 wayland session）
    let xdg = env::var("XDG_RUNTIME_DIR")?;
    if xdg.join("niri").exists() || glob("niri-*.sock") → Niri
    if xdg.join(format!("sway-ipc.{uid}.sock")).exists() → Sway
    // 3. 皆未命中 → Err（含 XDG_CURRENT_DESKTOP 現值，供除錯）
    bail!("無法偵測 Wayland compositor（XDG_CURRENT_DESKTOP={xdg_desktop:?}，未找到 niri/sway IPC socket）")
}
```

> socket 名稱以實機觀察為準：cybertron 有 `/run/user/1000/wayland-0`（umbriel
> 自己的），真實 niri/sway 環境的 socket 檔名需 code agent 實際確認（`ls
> $XDG_RUNTIME_DIR | grep -i -E 'niri|sway'`）——niri 官方文件：socket 慣例為
> `$XDG_RUNTIME_DIR/niri.sock`（新版）／`niri-<display>.sock`（舊版）；sway 為
> `sway-ipc.<uid>.sock`。若有出入以 niri/sway 官方 source 為準。

## 4. UI/UX

- `detect_compositor()` 失敗時：`tapedeck run` 直接 bail（明確訊息，exit 1）
- `tapedeck doctor` 新增一項：name=`compositor`，hint 說明用途，檢查方式呼叫
  `detect_compositor()`，失敗印 ❌ 原因（不 panic）

## 5. 測試策略

- `detect_compositor`：單元測試以 `env!` 變數鎖定方式測 socket 分支（ Testing
  Rust env……用 `tempfile` 建臨時 XDG_RUNTIME_DIR，不動真 env；session 變數
  部分以現行測試鎖定或傳參重將偵測函式拆為純函式以利注入）
- 誤判防呆：加一個測試「XDG_RUNTIME_DIR 指向空目錄」→ Err
- `WaitWindow` 分類邏輯（IPC vs NotFound）以單元測試 mock error 驗證
- 既有 16 dispatcher tests 不變
