# ref：Wayland 輸入注入支援度（2026）

> 來源：@librarian 調研 (2026-08-08)。存於 `docs/ref/` 供 OQ-02 與 Media Pipeline change-set 參考。

## 支援表

| tool/protocol | 協定 | niri | sway | 權限 | 狀態 |
|---------------|------|------|------|------|------|
| **wtype** | zwp_virtual_keyboard_manager_v1 | ✅ 26.04+ | ✅ | 無 | ✅ 維護中 |
| **libei** | Emulated Input | ✅ | ✅ | 無 | ✅ 2026 推薦逐漸普及 |
| **ydotool** | uinput | ❌ Wayland 限制 | ⚠️ 需 root | root/uinput group | ⚠️ less Wayland-native |
| **xdotool** | X11 only | ❌ | ❌ | root | ❌ deprecated |

## 結論（2026）
- **keyboard**: wtype（niri/sway 皆支援 virtual keyboard，無 root）
- **mouse pointer**: libei（compositor-friendly，支援 keyboard+mouse）— 推薦 2026 path
- **避開**: ydotool（root）、xdotool（X11）
- **wf-recorder** v25+ 支援 `--window <id>` 視窗標靶 — 輕量於 `-g` 幾何裁切，兩者可並用

## 來源
- [Wayland Virtual Keyboard](https://wayland.app/protocols/virtual-keyboard-unstable-v1)
- [libei docs](https://libinput.pages.freedesktop.org/libei/index.html)
- [wf-recorder](https://github.com/ammen99/wf-recorder) (`--window` v25+)

<!--
NOTE: 調研回覆中的 niri release-notes URL 為百度轉址鏈接、sway commit 指向 hickey/sway fork — 非主線來源，
但 niri 實作 zwp_virtual_keyboard_manager_v1 與 libei 支援的技術性結論仍可信。
-->
