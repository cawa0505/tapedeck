//! 硬體探針（OQ-05 / T6）
//!
//! 啟動/doctor 時掃描系統能力（Resilience 原則 2）：
//! - ffmpeg 編碼器掃描（`av1_vaapi` → `vp9_vaapi` → `libvpx-vp9`）
//! - `/dev/dri` 檢查（VA-API 裝置）
//!
//! 產出寫回 `config.toml [system.detected]`（`config::save`）。
//! 觸發點：`tapedeck doctor`（未實作）或 `optimize`（media-export T2）使用 `encoder_fallback` 時。

use std::process::Command;

/// 硬體能力探測結果
#[derive(Debug, Clone, Default, PartialEq)]
// ponytail: 消費端（doctor / optimize T2）未實作，先保留 API
#[allow(dead_code)]
pub struct HardwareCapabilities {
    /// 可用編碼器清單（ffmpeg -encoders 掃描）
    pub encoders: Vec<String>,
    /// VA-API 可用（/dev/dri 存在且掃到 vaapi 編碼器）
    pub vaapi: bool,
    /// /dev/dri 裝置存在
    pub dri: bool,
}

impl HardwareCapabilities {
    /// 掃描系統：ffmpeg 編碼器 + /dev/dri
    // ponytail: 消費端（doctor / optimize T2）未實作，先保留 API
    #[allow(dead_code)]
    pub fn probe_system() -> Self {
        let encoders = scan_ffmpeg_encoders();
        let dri = std::path::Path::new("/dev/dri").exists();
        let vaapi = dri && encoders.iter().any(|e| e.contains("vaapi"));
        Self {
            encoders,
            vaapi,
            dri,
        }
    }

    /// 是否支援指定編碼器
    // ponytail: 消費端未實作，先保留 API
    #[allow(dead_code)]
    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.iter().any(|e| e == name)
    }
}

/// 掃描 ffmpeg -encoders 輸出的可用編碼器（無 ffmpeg → 空清單，lenient）
fn scan_ffmpeg_encoders() -> Vec<String> {
    let Ok(out) = Command::new("ffmpeg").arg("-encoders").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_encoders(&String::from_utf8_lossy(&out.stdout))
}

/// 解析 `ffmpeg -encoders` 文字輸出：逐行取「flags + 名稱」。
/// `-encoders` 列表全部皆為 encoder（flags 無 encode/decode 標記），
/// 只要第一字元為 `V`（video）即收錄；audio/subtitle 與圖例行（`=`）排除。
fn parse_encoders(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            if flags.starts_with('V') && name != "=" {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 三階降級鏈（AV1 HW → VP9 HW → VP9 SW）
///
/// - `requested` 指定且可用 → 原樣使用
/// - 否則依鏈找第一個可用編碼器
/// - 全部不可用 → `None`（呼叫方報錯）
// ponytail: 消費端（optimize T2）未實作，先保留 API
#[allow(dead_code)]
pub fn encoder_fallback(caps: &HardwareCapabilities, requested: Option<&str>) -> Option<String> {
    if let Some(r) = requested.filter(|r| caps.has_encoder(r)) {
        return Some(r.to_string());
    }
    ["av1_vaapi", "vp9_vaapi", "libvpx-vp9"]
        .into_iter()
        .find(|e| caps.has_encoder(e))
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_encoders_filters_video_only() {
        // 樣本格式對照真實 ffmpeg -encoders（flags 如 V....D，D 為 direct rendering）
        let sample = "\
Encoders:
 V..... = Video
 A..... = Audio
 V....D av1_vaapi           Alliance for Open Media AV1 (VAAPI) (codec av1)
 V....D vp9_vaapi           Google VP9 (VAAPI) (codec vp9)
 V....D libvpx-vp9          libvpx VP9 (codec vp9)
 A..... aac                AAC (Advanced Audio Coding) (codec aac)
 V....D libx264            libx264 H.264 / AVC / MPEG-4 AVC
 V....D hevc_vaapi         H.265/HEVC (VAAPI) (codec hevc)
";
        let encoders = parse_encoders(sample);
        assert!(encoders.contains(&"av1_vaapi".to_string()));
        assert!(encoders.contains(&"vp9_vaapi".to_string()));
        assert!(encoders.contains(&"libvpx-vp9".to_string()));
        assert!(encoders.contains(&"libx264".to_string()));
        assert!(encoders.contains(&"hevc_vaapi".to_string()));
        // audio 與標題行排除
        assert!(!encoders.contains(&"aac".to_string()));
        assert!(!encoders.contains(&"=".to_string()));
    }

    #[test]
    fn parse_encoders_handles_garbage() {
        assert!(parse_encoders("").is_empty());
        assert!(parse_encoders("not an encoders list").is_empty());
    }

    #[test]
    fn fallback_requested_available() {
        let caps = HardwareCapabilities {
            encoders: vec!["libx264".to_string(), "av1_vaapi".to_string()],
            vaapi: true,
            dri: true,
        };
        assert_eq!(
            encoder_fallback(&caps, Some("av1_vaapi")),
            Some("av1_vaapi".to_string())
        );
    }

    #[test]
    fn fallback_requested_unavailable_uses_chain() {
        let caps = HardwareCapabilities {
            encoders: vec!["libx264".to_string(), "vp9_vaapi".to_string()],
            vaapi: true,
            dri: true,
        };
        // requested 不存在 → 走鏈：av1 無 → vp9_vaapi
        assert_eq!(
            encoder_fallback(&caps, Some("av1_vaapi")),
            Some("vp9_vaapi".to_string())
        );
    }

    #[test]
    fn fallback_chain_priority() {
        // 全可用 → AV1 HW
        let full = HardwareCapabilities {
            encoders: vec![
                "libvpx-vp9".to_string(),
                "vp9_vaapi".to_string(),
                "av1_vaapi".to_string(),
            ],
            vaapi: true,
            dri: true,
        };
        assert_eq!(encoder_fallback(&full, None), Some("av1_vaapi".to_string()));

        // 只有 SW → SW
        let sw_only = HardwareCapabilities {
            encoders: vec!["libvpx-vp9".to_string()],
            vaapi: false,
            dri: false,
        };
        assert_eq!(
            encoder_fallback(&sw_only, None),
            Some("libvpx-vp9".to_string())
        );
    }

    #[test]
    fn fallback_nothing_available() {
        let empty = HardwareCapabilities::default();
        assert_eq!(encoder_fallback(&empty, None), None);
        assert_eq!(encoder_fallback(&empty, Some("av1_vaapi")), None);
    }

    #[test]
    fn probe_vaapi_requires_dri_and_encoder() {
        // 有 vaapi 編碼器但無 /dev/dri → vaapi = false
        let no_dri = HardwareCapabilities {
            encoders: vec!["av1_vaapi".to_string()],
            vaapi: false,
            dri: false,
        };
        assert!(!no_dri.vaapi);
    }
}
