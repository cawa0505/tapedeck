//! XDG config 讀寫（REQ-6.5）
//!
//! - 路徑：`$XDG_CONFIG_HOME/tapedeck/config.toml`（未設定 → `$HOME/.config/tapedeck/config.toml`）
//! - 檔案不存在 → 回傳預設值並提示路徑
//! - `[system.detected]` 由硬體探針（engine/probe.rs）產出後寫回

use crate::paths::config_path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 設定檔結構（TOML）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// 錄製預設值（腳本未指定時套用）
    #[serde(default)]
    pub defaults: Defaults,
    /// 系統偵測結果（probe 產出，doctor/run 讀取）
    #[serde(default)]
    pub system: System,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    /// 預設輸出檔名（腳本無 Output 時）
    pub output: Option<String>,
    /// 預設引擎（vhs / native / auto）
    pub engine: Option<String>,
    /// 預設 FPS
    pub fps: Option<u32>,
    /// 預設編碼器
    pub encoder: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct System {
    #[serde(default, rename = "detected")]
    pub detected: Option<Detected>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Detected {
    /// 探測到的可用編碼器清單
    pub encoders: Vec<String>,
    /// VA-API 可用（/dev/dri 存在）
    pub vaapi: bool,
    /// /dev/dri 裝置存在
    pub dri: bool,
}

/// 讀取設定檔；不存在 → 預設值 + 提示
pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        eprintln!("提示：無設定檔 {}，使用預設值", path.display());
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("讀取設定檔失敗: {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&raw).with_context(|| format!("解析設定檔失敗: {}", path.display()))?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_returns_defaults() {
        let orig = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/does-not-exist-tapedeck");
        let cfg = load().unwrap();
        assert_eq!(cfg.defaults.engine, None);
        assert_eq!(cfg.defaults.fps, None);
        if let Some(o) = orig {
            std::env::set_var("XDG_CONFIG_HOME", o);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
[defaults]
output = "assets/demo.webm"
engine = "native"
fps = 30
encoder = "av1_vaapi"

[system.detected]
encoders = ["av1_vaapi", "libvpx-vp9"]
vaapi = true
dri = true
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.defaults.engine.as_deref(), Some("native"));
        assert_eq!(cfg.defaults.fps, Some(30));
        let d = cfg.system.detected.unwrap();
        assert_eq!(d.encoders.len(), 2);
        assert!(d.vaapi);
    }
}
