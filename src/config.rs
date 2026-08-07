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

/// 讀取設定檔；不存在 → 建立預設設定檔並回傳預設值（REQ-6.5）
pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        ensure_default_config(&path)?;
        eprintln!("提示：已建立預設設定檔 {}", path.display());
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("讀取設定檔失敗: {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&raw).with_context(|| format!("解析設定檔失敗: {}", path.display()))?;
    Ok(cfg)
}

/// 首次執行：建立含註解範例的預設設定檔
fn ensure_default_config(path: &std::path::Path) -> Result<()> {
    let template = DEFAULT_CONFIG_TEMPLATE;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("建立設定目錄失敗: {}", parent.display()))?;
    }
    std::fs::write(path, template)
        .with_context(|| format!("寫入預設設定檔失敗: {}", path.display()))
}

/// 預設設定檔內容（註解範例，全部為選填）
const DEFAULT_CONFIG_TEMPLATE: &str = r#"# Tapedeck 設定檔（XDG Base Directory）
# 路徑：$XDG_CONFIG_HOME/tapedeck/config.toml（未設定 → ~/.config/tapedeck/config.toml）

[defaults]
# 腳本未指定 Output 時的預設輸出檔名（相對路徑解析到 ~/.cache/tapedeck/）
# output = "output.webm"
# 預設引擎：vhs / native / auto（auto 依腳本意圖自動選擇）
# engine = "auto"
# 預設 FPS（覆寫腳本未指定時）
# fps = 30
# 預設編碼器（Optimize 未指定 encoder 時）
# encoder = "av1_vaapi"

# [system.detected] 由 tapedeck doctor / 硬體探針產出後寫回
# [system.detected]
# encoders = ["av1_vaapi", "libvpx-vp9"]
# vaapi = true
# dri = true
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_creates_default_and_returns_defaults() {
        let orig = std::env::var_os("XDG_CONFIG_HOME");
        let dir = std::env::temp_dir().join(format!("tapedeck-test-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        // 首次：建立預設檔 + 回傳預設值
        let cfg = load().unwrap();
        assert_eq!(cfg.defaults.engine, None);
        assert_eq!(cfg.defaults.fps, None);
        // 檔案確實被建立
        let path = config_path();
        assert!(path.exists(), "預設設定檔應被建立: {}", path.display());
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("[defaults]"));
        // 清理
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
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
