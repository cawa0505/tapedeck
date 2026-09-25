# 生存日誌（供 code agent 偵察用）

## 日期
2026-09-26

## 環境
- cybertron（本機），Wayland session＝umbriel（自研 compositor，非 niri/sway）
- session socket：`/run/user/1000/wayland-0`（Hermes shell 預設無 WAYLAND_DISPLAY，需帶 env）
- 無 niri socket；`/usr/bin/niri`、`/usr/bin/swaymsg` 二進位有裝但 session 不是它們
- umbriel 協定：zwlr_screencopy v3 + ext_image_copy_capture v1（實測）

## 發現的 bug（T4 驗收）

### Bug 1：detect_compositor() 對 umbriel 誤判（src/engine/wayland/compositor.rs:280-296）
- 只看 `XDG_CURRENT_DESKTOP` / `WAYLAND_DISPLAY` 字串 contains "niri"/"sway"
- umbriel 兩變數都不含 → 靜默落 fallback `NiriCompositor`
- 之後 `niri msg --json windows` 對 umbriel 連不上 → 每次呼叫都 Err
- WaitWindow 輪詢吃掉所有 Err 直到逾時 → 報「WaitWindow 逾時：視窗「foot-t4」未出現」
- **錯誤歸因誤導**：真死因是 compositor IPC 不通，不是視窗沒出現

### Bug 2：錯誤訊息無法歸因
- `find_window_geometry` 的 `niri msg` 失敗輸出（ Anyway `output()?` 的 io error）
- 被 WaitWindow 輪詢迴圈 `Err(_) =>` 吞掉，只留逾期訊息
- 應該：第一次 Err 就抓住根因（stderr / io error），逾時訊息附上根因

## 重現
```bash
cd /tmp/t4   # t4_gui.roll：Mode GUI、Output t4_gui.gif、WaitWindow "foot-t4"、foot -t foot-t4 sleep…
env WAYLAND_DISPLAY=wayland-0 XDG_RUNTIME_DIR=/run/user/1000 tapedeck run t4_gui.roll
# → WaitWindow 逾時（EXIT=1 直接跑；pipe 下去因 tail 吃掉 exit code 才是 0）
```

## 附註
- exit code 疑慮已排除：main.rs 有 error → exit(1)，EXIT=0 是 pipe 測量假象（tail 吞 exit code）
- foot 有裝；`foot -t <title>` 可自訂 app_id/title
- wf-recorder 沒測過對 umbriel 的 screen capture（等 Bug 1 修好才能測）
