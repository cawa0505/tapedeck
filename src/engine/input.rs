//! 輸入注入適配層（OQ-02 定案：uinput 優先 → wtype 回退）
//!
//! - `UinputNative`（evdev crate，userspace /dev/uinput）：鍵盤 + 滑鼠全包，
//!   走 libinput 被當真硬體接收，全 compositor 通用（含 GNOME/KDE）
//! - `Wtype`（wtype CLI，zwp_virtual_keyboard_manager_v1）：無權限需求但僅鍵盤、
//!   限 wlroots 系 compositor；滑鼠維持警告略過
//!
//! 依 Resilience 原則 1：外部工具一律經 trait 適配，不直接呼叫 CLI。
//! 依 Resilience 原則 4（lenient）：無可用後端時回退而非失敗。
//!
//! 調研：docs/ref/uinput-rust-crates.md（lib-4，2026-08-08）

use crate::engine::roll_parser::ClickType;
use anyhow::{bail, Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, KeyEvent, RelativeAxisCode};
use std::process::Command;
use std::sync::Mutex;

/// 鍵盤/滑鼠注入抽象層
///
/// `Send`：async 錄製循環可能跨 thread 傳遞 adapter。
pub trait InputAdapter: Send {
    /// Type「文字」：逐字輸入
    fn key_type(&self, text: &str) -> Result<()>;
    /// Key「具名鍵」x N：依 xkb_keysym 命名（如 Enter/Down）
    fn key_press(&self, name: &str, count: u32) -> Result<()>;
    /// Shortcut 組合鍵（如 "Ctrl+S"）
    fn shortcut(&self, combo: &str) -> Result<()>;
    /// MouseMove 相對座標
    fn mouse_move(&self, x: i32, y: i32) -> Result<()>;
    /// Click 點擊
    fn mouse_click(&self, button: ClickType) -> Result<()>;
}

/// 輸入後端選擇（Resilience 原則 1：adapter + fallback）
pub enum InputBackend {
    /// userspace uinput（evdev crate）：鍵盤 + 滑鼠，全 compositor
    UinputNative,
    /// wtype CLI：無權限、鍵盤 only、wlroots 系
    Wtype,
}

impl std::fmt::Display for InputBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UinputNative => write!(f, "UinputNative (Kernel Level Input)"),
            Self::Wtype => write!(f, "Wtype (Keyboard only)"),
        }
    }
}

impl InputBackend {
    /// 依本機能力挑選後端：/dev/uinput 可寫則 uinput 優先，否則回退 wtype。
    ///
    /// 偵測 = open 測權限（零副作用，不註冊裝置；真正註冊在 adapter() 才發生）。
    pub fn detect() -> Self {
        match std::fs::OpenOptions::new().write(true).open("/dev/uinput") {
            Ok(_) => Self::UinputNative,
            Err(_) => Self::Wtype,
        }
    }

    pub fn adapter(&self) -> Box<dyn InputAdapter + Send> {
        match self {
            Self::UinputNative => match UinputAdapter::new() {
                Ok(a) => Box::new(a),
                Err(e) => {
                    eprintln!("警告：uinput 初始化失敗，回退 wtype：{e}");
                    Box::new(WtypeAdapter::new())
                }
            },
            Self::Wtype => Box::new(WtypeAdapter::new()),
        }
    }
}

/// userspace uinput 適配器（evdev crate）
///
/// 同時持有鍵盤 + 滑鼠兩個虛擬裝置 — fd 關閉 = 裝置消失，必須保活到程式結束
/// （ref 陷阱 #4）。Mutex 提供內部可變性（VirtualDevice::emit 需 &mut，
/// 但 trait 方法只給 &self）；錄製為單執行緒順序執行，無競爭風險。
pub struct UinputAdapter {
    kbd: Mutex<VirtualDevice>,
    mouse: Mutex<VirtualDevice>,
}

impl UinputAdapter {
    /// open /dev/uinput 並註冊虛擬鍵盤 + 滑鼠。
    ///
    /// 滑鼠必須同時註冊 BTN 鍵 + REL 軸，否則 libinput/桌面忽略（ref 陷阱 #2）。
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for c in ' '..='~' {
            if let Some((code, _shift)) = ascii_key(c) {
                keys.insert(code);
            }
        }
        for code in NAMED_KEY_CODES {
            keys.insert(*code);
        }
        for code in KEY_MODS {
            keys.insert(*code);
        }
        let kbd = VirtualDevice::builder()?
            .name("tapedeck virtual keyboard")
            .with_keys(&keys)?
            .build()
            .context("uinput 鍵盤裝置註冊失敗")?;

        let mut buttons = AttributeSet::<KeyCode>::new();
        buttons.insert(KeyCode::BTN_LEFT);
        buttons.insert(KeyCode::BTN_RIGHT);
        buttons.insert(KeyCode::BTN_MIDDLE);
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_X);
        axes.insert(RelativeAxisCode::REL_Y);
        let mouse = VirtualDevice::builder()?
            .name("tapedeck virtual mouse")
            .with_keys(&buttons)?
            .with_relative_axes(&axes)?
            .build()
            .context("uinput 滑鼠裝置註冊失敗")?;

        Ok(Self {
            kbd: Mutex::new(kbd),
            mouse: Mutex::new(mouse),
        })
    }

    /// 送出單一按鍵 press+release（含 shift 需求時自動帶修飾鍵）
    fn tap(&self, code: KeyCode, shift: bool) -> Result<()> {
        if shift {
            self.emit_key(KEY_MODS[1], 1)?;
        }
        self.emit_key(code, 1)?;
        self.emit_key(code, 0)?;
        if shift {
            self.emit_key(KEY_MODS[1], 0)?;
        }
        Ok(())
    }

    fn emit_key(&self, code: KeyCode, value: i32) -> Result<()> {
        self.kbd
            .lock()
            .expect("uinput kbd lock poisoned")
            .emit(&[*KeyEvent::new(code, value)])
            .context("uinput 鍵盤事件送出失敗")
    }
}

// 官方範例（examples/virtual_keyboard.rs）用 KeyEvent::new + *deref 成 InputEvent

/// 鍵盤：ASCII 可列印字元 → (keycode, 需要 shift)
///
/// 純函式供測試。覆蓋 0x20–0x7E；對應 evdev 的 KEY_* codes（US layout）。
fn ascii_key(c: char) -> Option<(KeyCode, bool)> {
    let c = c as u32;
    match c {
        0x20 => Some((KeyCode::KEY_SPACE, false)),
        0x30..=0x39 => {
            // 數字行：0 在 KEY_0（行尾），1-9 依序
            let code = if c == 0x30 {
                KeyCode::KEY_0
            } else {
                KeyCode(KeyCode::KEY_1.0 + (c - 0x31) as u16)
            };
            Some((code, false))
        }
        0x41..=0x5a => Some((KeyCode(KeyCode::KEY_A.0 + (c - 0x41) as u16), true)),
        0x61..=0x7a => Some((KeyCode(KeyCode::KEY_A.0 + (c - 0x61) as u16), false)),
        0x21..=0x7e => {
            // 標點符號表：KEY_ 對應（未列出的符號需 shift 標記，按 US layout）
            let entry = match c {
                0x21 => (KeyCode::KEY_1, true),           // !
                0x22 => (KeyCode::KEY_APOSTROPHE, true),  // "
                0x23 => (KeyCode::KEY_3, true),           // #
                0x24 => (KeyCode::KEY_4, true),           // $
                0x25 => (KeyCode::KEY_5, true),           // %
                0x26 => (KeyCode::KEY_7, true),           // &
                0x27 => (KeyCode::KEY_APOSTROPHE, false), // '
                0x28 => (KeyCode::KEY_9, true),           // (
                0x29 => (KeyCode::KEY_0, true),           // )
                0x2a => (KeyCode::KEY_8, true),           // *
                0x2b => (KeyCode::KEY_EQUAL, true),       // +
                0x2c => (KeyCode::KEY_COMMA, false),      // ,
                0x2d => (KeyCode::KEY_MINUS, false),      // -
                0x2e => (KeyCode::KEY_DOT, false),        // .
                0x2f => (KeyCode::KEY_SLASH, false),      // /
                0x3a => (KeyCode::KEY_SEMICOLON, true),   // :
                0x3b => (KeyCode::KEY_SEMICOLON, false),  // ;
                0x3c => (KeyCode::KEY_COMMA, true),       // <
                0x3d => (KeyCode::KEY_EQUAL, false),      // =
                0x3e => (KeyCode::KEY_DOT, true),         // >
                0x3f => (KeyCode::KEY_SLASH, true),       // ?
                0x40 => (KeyCode::KEY_2, true),           // @
                0x5b => (KeyCode::KEY_LEFTBRACE, false),  // [
                0x5c => (KeyCode::KEY_BACKSLASH, false),  // \
                0x5d => (KeyCode::KEY_RIGHTBRACE, false), // ]
                0x5e => (KeyCode::KEY_6, true),           // ^
                0x5f => (KeyCode::KEY_MINUS, true),       // _
                0x60 => (KeyCode::KEY_GRAVE, false),      // `
                0x7b => (KeyCode::KEY_LEFTBRACE, true),   // {
                0x7c => (KeyCode::KEY_BACKSLASH, true),   // |
                0x7d => (KeyCode::KEY_RIGHTBRACE, true),  // }
                0x7e => (KeyCode::KEY_GRAVE, true),       // ~
                _ => return None,
            };
            Some(entry)
        }
        _ => None,
    }
}

/// 具名鍵（xkb_keysym 命名子集）→ evdev KeyCode
const NAMED_KEY_CODES: &[KeyCode] = &[
    KeyCode::KEY_ENTER,
    KeyCode::KEY_TAB,
    KeyCode::KEY_SPACE,
    KeyCode::KEY_BACKSPACE,
    KeyCode::KEY_DELETE,
    KeyCode::KEY_ESC,
    KeyCode::KEY_HOME,
    KeyCode::KEY_END,
    KeyCode::KEY_PAGEUP,
    KeyCode::KEY_PAGEDOWN,
    KeyCode::KEY_UP,
    KeyCode::KEY_DOWN,
    KeyCode::KEY_LEFT,
    KeyCode::KEY_RIGHT,
    KeyCode::KEY_F1,
    KeyCode::KEY_F2,
    KeyCode::KEY_F3,
    KeyCode::KEY_F4,
    KeyCode::KEY_F5,
    KeyCode::KEY_F6,
    KeyCode::KEY_F7,
    KeyCode::KEY_F8,
    KeyCode::KEY_F9,
    KeyCode::KEY_F10,
    KeyCode::KEY_F11,
    KeyCode::KEY_F12,
];

const KEY_MODS: &[KeyCode] = &[
    KeyCode::KEY_LEFTCTRL,
    KeyCode::KEY_LEFTSHIFT,
    KeyCode::KEY_LEFTALT,
    KeyCode::KEY_LEFTMETA,
];

/// 具名鍵解析：xkb_keysym 命名（vhs Key 指令同源）→ (evdev KeyCode, 需要 shift)
///
/// 支援：單字元 ASCII（走 ascii_key）、Enter/Tab/Space 等常見具名鍵、F1–F12。
/// 未知名稱回 Err（wtype 後端仍可完整支援；uinput 只覆蓋子集）。
fn parse_key_name(name: &str) -> Result<(KeyCode, bool)> {
    if let Some((code, shift)) = ascii_key_any(name) {
        return Ok((code, shift));
    }
    let code = match name {
        "Enter" | "Return" => KeyCode::KEY_ENTER,
        "Tab" => KeyCode::KEY_TAB,
        "Space" | "Spacebar" => KeyCode::KEY_SPACE,
        "BackSpace" | "Backspace" => KeyCode::KEY_BACKSPACE,
        "Delete" => KeyCode::KEY_DELETE,
        "Escape" | "Esc" => KeyCode::KEY_ESC,
        "Home" => KeyCode::KEY_HOME,
        "End" => KeyCode::KEY_END,
        "PageUp" => KeyCode::KEY_PAGEUP,
        "PageDown" => KeyCode::KEY_PAGEDOWN,
        "Up" => KeyCode::KEY_UP,
        "Down" => KeyCode::KEY_DOWN,
        "Left" => KeyCode::KEY_LEFT,
        "Right" => KeyCode::KEY_RIGHT,
        _ => {
            // F1–F12
            if let Some(n) = name.strip_prefix('F') {
                if let Ok(n) = n.parse::<u16>() {
                    if (1..=12).contains(&n) {
                        return Ok((KeyCode(KeyCode::KEY_F1.0 + n - 1), false));
                    }
                }
            }
            bail!("Key 不支援的按鍵名稱「{name}」（uinput 子集；wtype 後端可完整支援）");
        }
    };
    Ok((code, false))
}

/// ascii_key 的「名稱」版本：接受單字元（a/z/1/!/…）或鍵名（如 "a"）
fn ascii_key_any(name: &str) -> Option<(KeyCode, bool)> {
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    ascii_key(c)
}

impl InputAdapter for UinputAdapter {
    fn key_type(&self, text: &str) -> Result<()> {
        for c in text.chars() {
            let (code, shift) = ascii_key(c)
                .ok_or_else(|| anyhow::anyhow!("Type 不支援的字元「{c}」（uinput ASCII 子集）"))?;
            self.tap(code, shift)?;
        }
        Ok(())
    }

    fn key_press(&self, name: &str, count: u32) -> Result<()> {
        let (code, shift) = parse_key_name(name)?;
        for _ in 0..count.max(1) {
            self.tap(code, shift)?;
        }
        Ok(())
    }

    fn shortcut(&self, combo: &str) -> Result<()> {
        let (mods, key) = parse_shortcut(combo)?;
        let key_mods: Vec<KeyCode> = mods
            .iter()
            .map(|m| match m.as_str() {
                "ctrl" => KEY_MODS[0],
                "shift" => KEY_MODS[1],
                "alt" => KEY_MODS[2],
                _ => KEY_MODS[3],
            })
            .collect();
        let (code, shift) = parse_key_name(&key)?;
        for m in &key_mods {
            self.tap(*m, false)?;
        }
        self.tap(code, shift)?;
        for m in key_mods.iter().rev() {
            self.tap(*m, false)?;
        }
        Ok(())
    }

    fn mouse_move(&self, x: i32, y: i32) -> Result<()> {
        let events = [
            InputEvent::new_now(EventType::RELATIVE.0, RelativeAxisCode::REL_X.0, x),
            InputEvent::new_now(EventType::RELATIVE.0, RelativeAxisCode::REL_Y.0, y),
        ];
        self.mouse
            .lock()
            .expect("uinput mouse lock poisoned")
            .emit(&events)
            .context("uinput 滑鼠移動事件送出失敗")
    }

    fn mouse_click(&self, button: ClickType) -> Result<()> {
        let code = match button {
            ClickType::Left => KeyCode::BTN_LEFT,
            ClickType::Right => KeyCode::BTN_RIGHT,
            ClickType::Middle => KeyCode::BTN_MIDDLE,
        };
        let events = [*KeyEvent::new(code, 1), *KeyEvent::new(code, 0)];
        self.mouse
            .lock()
            .expect("uinput mouse lock poisoned")
            .emit(&events)
            .context("uinput 滑鼠點擊事件送出失敗")
    }
}

/// wtype CLI 適配器（鍵盤 only；滑鼠不可用時回 Err，由 dispatcher 警告略過）
pub struct WtypeAdapter {
    wtype: String,
}

impl WtypeAdapter {
    pub fn new() -> Self {
        Self {
            wtype: std::env::var("WTYPE").unwrap_or_else(|_| "wtype".to_owned()),
        }
    }
}

impl Default for WtypeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InputAdapter for WtypeAdapter {
    fn key_type(&self, text: &str) -> Result<()> {
        let status = Command::new(&self.wtype)
            .arg(text)
            .status()
            .with_context(|| {
                format!("failed to start {}; install wtype or set WTYPE", self.wtype)
            })?;
        if !status.success() {
            bail!("{0} 輸入「{text}」失敗（exit {status}）", self.wtype);
        }
        Ok(())
    }

    fn key_press(&self, name: &str, count: u32) -> Result<()> {
        let mut cmd = Command::new(&self.wtype);
        for _ in 0..count.max(1) {
            cmd.arg("-P").arg(name).arg("-p").arg(name);
        }
        let status = cmd.status().with_context(|| {
            format!("failed to start {}; install wtype or set WTYPE", self.wtype)
        })?;
        if !status.success() {
            bail!(
                "{0} 按鍵「{name}」x{count} 失敗（exit {status}）",
                self.wtype
            );
        }
        Ok(())
    }

    fn shortcut(&self, combo: &str) -> Result<()> {
        let (mods, key) = parse_shortcut(combo)?;
        let mut cmd = Command::new(&self.wtype);
        for m in &mods {
            cmd.arg("-M").arg(m);
        }
        cmd.arg(key);
        for m in mods.iter().rev() {
            cmd.arg("-m").arg(m);
        }
        let status = cmd.status().with_context(|| {
            format!("failed to start {}; install wtype or set WTYPE", self.wtype)
        })?;
        if !status.success() {
            bail!("{0} 組合鍵「{combo}」失敗（exit {status}）", self.wtype);
        }
        Ok(())
    }

    fn mouse_move(&self, _x: i32, _y: i32) -> Result<()> {
        bail!("滑鼠注入未支援：/dev/uinput 不可寫（wtype 後端僅鍵盤）")
    }

    fn mouse_click(&self, _button: ClickType) -> Result<()> {
        bail!("滑鼠注入未支援：/dev/uinput 不可寫（wtype 後端僅鍵盤）")
    }
}

/// 解析 "Ctrl+Shift+S" → (modifiers, key)
///
/// 純函式供測試。支援修飾鍵：Ctrl/Shift/Alt/Super。
/// wtype modifier 名：ctrl/shift/alt/super。
fn parse_shortcut(combo: &str) -> Result<(Vec<String>, String)> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    if parts.len() < 2 {
        bail!("Shortcut 格式錯誤：「{combo}」（預期如 Ctrl+S，單鍵請用 Key）");
    }
    let (mods, key) = match parts.split_last() {
        Some((k, m)) => (m.to_vec(), k.to_lowercase()),
        None => bail!("Shortcut 格式錯誤：「{combo}」（預期如 Ctrl+S）"),
    };
    let mut out = Vec::new();
    for m in mods {
        let name = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "ctrl",
            "shift" => "shift",
            "alt" => "alt",
            "super" | "win" | "meta" => "super",
            other => bail!("Shortcut 不支援的修飾鍵「{other}」（支援 Ctrl/Shift/Alt/Super）"),
        };
        out.push(name.to_string());
    }
    Ok((out, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_modifier() {
        let (mods, key) = parse_shortcut("Ctrl+S").unwrap();
        assert_eq!(mods, vec!["ctrl"]);
        assert_eq!(key, "s");
    }

    #[test]
    fn parse_multi_modifier() {
        let (mods, key) = parse_shortcut("Ctrl+Shift+T").unwrap();
        assert_eq!(mods, vec!["ctrl", "shift"]);
        assert_eq!(key, "t");
    }

    #[test]
    fn parse_case_insensitive_aliases() {
        let (mods, key) = parse_shortcut("super+ALT+f4").unwrap();
        assert_eq!(mods, vec!["super", "alt"]);
        assert_eq!(key, "f4");
    }

    #[test]
    fn parse_invalid_modifier_rejected() {
        let err = parse_shortcut("Fn+Ctrl+Q").unwrap_err();
        assert!(err.to_string().contains("不支援的修飾鍵"));
    }

    #[test]
    fn parse_bare_key_without_modifier_rejected() {
        let err = parse_shortcut("S").unwrap_err();
        assert!(err.to_string().contains("格式錯誤"));
    }

    #[test]
    fn mouse_unavailable_by_default() {
        let adapter = WtypeAdapter::new();
        assert!(adapter.mouse_click(ClickType::Left).is_err());
        assert!(adapter.mouse_move(10, 20).is_err());
    }

    #[test]
    fn ascii_mapping_basic() {
        assert_eq!(ascii_key('a'), Some((KeyCode::KEY_A, false)));
        assert_eq!(ascii_key('A'), Some((KeyCode::KEY_A, true)));
        assert_eq!(ascii_key('1'), Some((KeyCode::KEY_1, false)));
        assert_eq!(ascii_key('!'), Some((KeyCode::KEY_1, true)));
        assert_eq!(ascii_key(' '), Some((KeyCode::KEY_SPACE, false)));
        assert_eq!(ascii_key('\n'), None);
    }

    #[test]
    fn named_key_parsing() {
        assert_eq!(
            parse_key_name("Enter").unwrap(),
            (KeyCode::KEY_ENTER, false)
        );
        assert_eq!(parse_key_name("F5").unwrap(), (KeyCode::KEY_F5, false));
        assert!(parse_key_name("Ctrl").is_err());
        assert_eq!(
            parse_key_name("PageUp").unwrap(),
            (KeyCode::KEY_PAGEUP, false)
        );
    }

    /// 實機 uinput 注入測試：需 /dev/uinput 可寫的環境（CI/容器跳過）
    ///
    /// 驗證 InputBackend 選擇 uinput + 虛擬裝置建立 + 事件送出（零位移滑鼠，
    /// 無副作用）。`cargo test -- --ignored` 於本機執行。
    #[test]
    #[ignore = "需要 /dev/uinput 寫權限"]
    fn uinput_injects_relative_motion() {
        assert!(matches!(InputBackend::detect(), InputBackend::UinputNative));
        let adapter = UinputAdapter::new().expect("uinput 可寫時應能建立虛擬裝置");
        // 零位移：走完整 emit 路徑但不實際移動滑鼠
        adapter.mouse_move(0, 0).expect("emit 應成功");
    }
}
