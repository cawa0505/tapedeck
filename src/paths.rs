//! XDG 路徑解析（REQ-6）：三個目錄共用 `$VAR` / `$HOME` fallback 邏輯
//!
//! | 用途   | var               | fallback     | sub                    |
//! |--------|-------------------|--------------|------------------------|
//! | 輸出   | `XDG_CACHE_HOME`  | `.cache`     | `tapedeck`             |
//! | config | `XDG_CONFIG_HOME` | `.config`    | `tapedeck/config.toml` |
//! | state  | `XDG_STATE_HOME`  | `.local/state` | `tapedeck`            |

use anyhow::Result;
use std::path::{Path, PathBuf};

/// `$XDG_*_HOME/<sub>` 或 `$HOME/<fallback>/<sub>`
pub fn xdg_dir(var: &str, fallback: &str, sub: &str) -> PathBuf {
    let base = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(fallback))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
    });
    base.join(sub)
}

/// 輸出快取目錄（REQ-6.1）
pub fn cache_dir() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache", "tapedeck")
}

/// config 檔路徑（REQ-6.5）
pub fn config_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config", "tapedeck/config.toml")
}

/// state 目錄（REQ-6.6）：SQLite DB 與錄製歷程
pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state", "tapedeck")
}

/// 輸出路徑解析（REQ-6.1）：CLI 覆寫/絕對路徑照原樣；相對 → XDG cache
pub fn resolve_output_path(script_output: &str, cli_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = cli_override {
        return Ok(p.to_path_buf()); // CLI 顯式覆寫，照原樣
    }
    let p = Path::new(script_output);
    if p.is_absolute() {
        return Ok(p.to_path_buf()); // 絕對路徑照原樣
    }
    Ok(cache_dir().join(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 環境變數測試統一走 crate::TEST_ENV_LOCK（跨模組序列化，std::env 全域）
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::TEST_ENV_LOCK.lock().unwrap()
    }

    #[test]
    fn output_path_relative_uses_xdg_cache() {
        let _g = lock();
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-cache");
        std::env::set_var("HOME", "/home/user");
        let p = resolve_output_path("assets/demo.webm", None).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/xdg-cache/tapedeck/assets/demo.webm"));
    }

    #[test]
    fn output_path_xdg_unset_uses_home_fallback() {
        let _g = lock();
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::set_var("HOME", "/home/user");
        let p = resolve_output_path("assets/demo.webm", None).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/home/user/.cache/tapedeck/assets/demo.webm")
        );
    }

    #[test]
    fn output_path_absolute_kept() {
        let p = resolve_output_path("/var/tmp/demo.webm", None).unwrap();
        assert_eq!(p, PathBuf::from("/var/tmp/demo.webm"));
    }

    #[test]
    fn output_path_cli_override_wins() {
        let _g = lock();
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-cache");
        let p =
            resolve_output_path("assets/demo.webm", Some(Path::new("/cli/override.webm"))).unwrap();
        assert_eq!(p, PathBuf::from("/cli/override.webm"));
    }

    #[test]
    fn config_path_uses_xdg_config_home() {
        let _g = lock();
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-config");
        std::env::set_var("HOME", "/home/user");
        assert_eq!(
            config_path(),
            PathBuf::from("/tmp/xdg-config/tapedeck/config.toml")
        );
    }

    #[test]
    fn config_path_xdg_unset_uses_home_fallback() {
        let _g = lock();
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::set_var("HOME", "/home/user");
        assert_eq!(
            config_path(),
            PathBuf::from("/home/user/.config/tapedeck/config.toml")
        );
    }
}
