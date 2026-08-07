//! FfmpegAdapter — ffmpeg CLI 適配器（Resilience 原則 1）
//!
//! 以子程序呼叫 ffmpeg，不新增 crate 依賴（design.md）。
//! `probe()` 偵測 palettegen filter 與 libwebp encoder（Resilience 原則 2）。
//! 解析邏輯抽成純函式，單元測試直接餵 mock 輸出字串（Resilience 原則 4）。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

/// 探測結果：media 能力
#[derive(Debug, Clone, PartialEq)]
pub struct MediaCapabilities {
    pub has_palettegen: bool,
    pub has_libwebp: bool,
    pub ffmpeg_version: String,
}

/// ffmpeg 適配器 trait（design.md 定案 API）
// ponytail: palettegen/paletteuse/to_webp/extract_frame/hstack 由 T2/T4 實作，先定義契約
#[allow(dead_code)]
pub trait FfmpegAdapter {
    /// 版本與能力探針：確認 palettegen/libwebp 可用
    fn probe(&self) -> Result<MediaCapabilities>;
    /// pass1：生成調色盤
    fn palettegen(&self, input: &Path, fps: u32) -> Result<PathBuf>;
    /// pass2：套用調色盤輸出 GIF
    fn paletteuse(&self, input: &Path, palette: &Path, output: &Path, fps: u32) -> Result<()>;
    /// WebP 輸出
    fn to_webp(&self, input: &Path, output: &Path, quality: u8) -> Result<()>;
    /// 指定時間點抽單幀 PNG（ffmpeg -ss）
    fn extract_frame(&self, input: &Path, ts_ms: u64, out: &Path) -> Result<()>;
    /// 橫向拼接多張 PNG（hstack）
    fn hstack(&self, frames: &[PathBuf], output: &Path) -> Result<()>;
}

/// 預設實作（T1：僅 probe；其餘 T2/T4）
#[derive(Debug, Default)]
// ponytail: 消費端（optimize T2 / filmstrip T4）未實作，先保留 API
#[allow(dead_code)]
pub struct FfmpegV1Adapter;

impl FfmpegV1Adapter {
    // ponytail: 消費端（optimize T2）未實作，先保留
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}

impl FfmpegAdapter for FfmpegV1Adapter {
    fn probe(&self) -> Result<MediaCapabilities> {
        Ok(MediaCapabilities {
            has_palettegen: scan_filters().contains(&"palettegen".to_string()),
            has_libwebp: scan_encoders().contains(&"libwebp".to_string()),
            ffmpeg_version: scan_version(),
        })
    }

    fn palettegen(&self, _input: &Path, _fps: u32) -> Result<PathBuf> {
        unimplemented!("T2 optimize")
    }

    fn paletteuse(&self, _input: &Path, _palette: &Path, _output: &Path, _fps: u32) -> Result<()> {
        unimplemented!("T2 optimize")
    }

    fn to_webp(&self, _input: &Path, _output: &Path, _quality: u8) -> Result<()> {
        unimplemented!("T2 optimize")
    }

    fn extract_frame(&self, _input: &Path, _ts_ms: u64, _out: &Path) -> Result<()> {
        unimplemented!("T4 filmstrip")
    }

    fn hstack(&self, _frames: &[PathBuf], _output: &Path) -> Result<()> {
        unimplemented!("T4 filmstrip")
    }
}

/// 執行 `ffmpeg <args>` 並回傳 stdout（失敗 → None，lenient）
fn run_ffmpeg(args: &[&str]) -> Option<String> {
    let out = Command::new("ffmpeg").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 掃描 filter 清單（`ffmpeg -filters`），回傳名稱集合
fn scan_filters() -> Vec<String> {
    run_ffmpeg(&["-hide_banner", "-filters"])
        .map(|out| parse_filter_lines(&out))
        .unwrap_or_default()
}

/// 掃描 encoder 清單（`ffmpeg -encoders`），回傳名稱集合
fn scan_encoders() -> Vec<String> {
    run_ffmpeg(&["-hide_banner", "-encoders"])
        .map(|out| parse_encoder_lines(&out))
        .unwrap_or_default()
}

/// ffmpeg 版本字串（`ffmpeg -version` 首行第三欄）
fn scan_version() -> String {
    run_ffmpeg(&["-version"])
        .map(|out| parse_version_line(&out))
        .unwrap_or_default()
}

/// 解析 `ffmpeg -filters` 行：flags（2 字元）+ 名稱（第二欄）
fn parse_filter_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
        .collect()
}

/// 解析 `ffmpeg -encoders` 行：flags（6 字元）+ 名稱（第二欄）
fn parse_encoder_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            if flags.starts_with('V') {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 解析 `ffmpeg -version` 首行：`ffmpeg version <ver> ...`
fn parse_version_line(output: &str) -> String {
    output
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("ffmpeg version "))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_lines_extract_names() {
        let out = "\
 .. palettegen        V->V       Find the optimal palette for a given stream.
 .. paletteuse        VV->V      Use a palette to downsample an input video stream.
 .S hstack            N->V       Stack video inputs horizontally.
";
        let names = parse_filter_lines(out);
        assert!(names.contains(&"palettegen".to_string()));
        assert!(names.contains(&"hstack".to_string()));
        assert!(!names.contains(&"nonexistent".to_string()));
    }

    #[test]
    fn encoder_lines_only_video() {
        let out = "\
 V....D libwebp_anim         libwebp WebP image (codec webp)
 A....D libopus              libopus Opus (codec opus)
";
        let names = parse_encoder_lines(out);
        assert!(names.contains(&"libwebp_anim".to_string()));
        assert!(!names.contains(&"libopus".to_string()), "audio 不應收錄");
    }

    #[test]
    fn version_line_extracts_third_token() {
        let out = "ffmpeg version n8.1.2 Copyright (c) 2000-2026 the FFmpeg developers\n";
        assert_eq!(parse_version_line(out), "n8.1.2");
    }

    #[test]
    fn probe_against_mock_outputs() {
        // 餵固定 mock 輸出，驗證 probe 彙整（不跑真實 ffmpeg）
        let filters = parse_filter_lines(" .. palettegen V->V palette\n");
        let encoders = parse_encoder_lines(" V....D libwebp libwebp\n");
        let version = parse_version_line("ffmpeg version 9.9.9 mock\n");
        assert_eq!(
            MediaCapabilities {
                has_palettegen: filters.contains(&"palettegen".to_string()),
                has_libwebp: encoders.contains(&"libwebp".to_string()),
                ffmpeg_version: version,
            },
            MediaCapabilities {
                has_palettegen: true,
                has_libwebp: true,
                ffmpeg_version: "9.9.9".to_string(),
            }
        );
    }
}
