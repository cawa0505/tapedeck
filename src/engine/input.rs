//! Wayland 輸入注入適配層（OQ-02 定案：wtype 鍵盤 + libei 滑鼠）
//!
//! - 鍵盤（Type/Key/Shortcut）：wtype CLI（zwp_virtual_keyboard_manager_v1，無 root）
//! - 滑鼠（Click/MouseMove）：libei 為 C 函式庫且本機無 dev headers →
//!   trait 預設回 Err，由 dispatcher 警告略過（能力偵測：無可用注入器即略過）
//!
//! 依 Resilience 原則 1：外部工具一律經 trait 適配，不直接呼叫 CLI。

use crate::engine::roll_parser::ClickType;
use anyhow::{bail, Context, Result};
use std::process::Command;

/// 鍵盤/滑鼠注入抽象層
pub trait InputAdapter {
    /// Type「文字」：逐字輸入
    fn key_type(&self, text: &str) -> Result<()>;
    /// Key「具名鍵」x N：依 xkb_keysym 命名（如 Enter/Down）
    fn key_press(&self, name: &str, count: u32) -> Result<()>;
    /// Shortcut 組合鍵（如 "Ctrl+S"）
    fn shortcut(&self, combo: &str) -> Result<()>;
    /// MouseMove 相對座標（OQ-02 滑鼠；無 libei 注入器時略過）
    fn mouse_move(&self, _x: i32, _y: i32) -> Result<()> {
        bail!("滑鼠注入未支援：本機無可用 libei 注入器（OQ-02）")
    }
    /// Click 點擊（同 mouse_move 能力偵測）
    fn mouse_click(&self, _button: ClickType) -> Result<()> {
        bail!("滑鼠注入未支援：本機無可用 libei 注入器（OQ-02）")
    }
}

/// wtype CLI 適配器（鍵盤）
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
}
