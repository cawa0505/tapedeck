//! FfmpegAdapter — ffmpeg CLI 適配器（Resilience 原則 1）
//!
//! 以子程序呼叫 ffmpeg，不新增 crate 依賴（design.md）。
//! `probe()` 偵測 palettegen filter 與 libwebp encoder（Resilience 原則 2）。
//! 指令組裝抽成 `*_cmd` 純函式，供 optimize 的 dry-run 顯示與單元測試
//! 直接斷言（Resilience 原則 4，Mock Subprocess）。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

use crate::paths;

/// 探測結果：media 能力
#[derive(Debug, Clone, PartialEq)]
pub struct MediaCapabilities {
    pub has_palettegen: bool,
    pub has_libwebp: bool,
    pub ffmpeg_version: String,
}

/// ffmpeg 適配器 trait（design.md 定案 API）
// ponytail: extract_frame/hstack 由 T4 實作，先定義契約
#[allow(dead_code)]
pub trait FfmpegAdapter {
    /// 版本與能力探針：確認 palettegen/libwebp 可用
    fn probe(&self) -> Result<MediaCapabilities>;
    /// pass1：生成調色盤（回傳調色盤路徑）
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

/// 預設實作
#[derive(Debug, Default)]
pub struct FfmpegV1Adapter;

impl FfmpegV1Adapter {
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

    fn palettegen(&self, input: &Path, fps: u32) -> Result<PathBuf> {
        let palette = palette_path(input);
        run_ffmpeg_strict(&palettegen_cmd(input, fps, &palette), "palettegen")?;
        Ok(palette)
    }

    fn paletteuse(&self, input: &Path, palette: &Path, output: &Path, fps: u32) -> Result<()> {
        run_ffmpeg_strict(&paletteuse_cmd(input, palette, output, fps), "paletteuse")
    }

    fn to_webp(&self, input: &Path, output: &Path, quality: u8) -> Result<()> {
        run_ffmpeg_strict(&to_webp_cmd(input, output, quality), "libwebp")
    }

    fn extract_frame(&self, _input: &Path, _ts_ms: u64, _out: &Path) -> Result<()> {
        unimplemented!("T4 filmstrip")
    }

    fn hstack(&self, _frames: &[PathBuf], _output: &Path) -> Result<()> {
        unimplemented!("T4 filmstrip")
    }
}

/// 調色盤暫存路徑：`~/.cache/tapedeck/<input-stem>.palette.png`（非輸出，用完即刪）
pub(crate) fn palette_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "palette".to_string());
    paths::cache_dir().join(format!("{stem}.palette.png"))
}

/// pass1 指令：`ffmpeg -y -i <in> -vf "fps=<n>,palettegen=max_colors=256" <palette>`
pub(crate) fn palettegen_cmd(input: &Path, fps: u32, palette: &Path) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        format!("fps={fps},palettegen=max_colors=256"),
        palette.to_string_lossy().into_owned(),
    ]
}

/// pass2 指令：`ffmpeg -y -i <in> -i <palette> -lavfi "fps=<n> [x];[x][1:v] paletteuse=dither=bayer:bayer_scale=5" <out>`
pub(crate) fn paletteuse_cmd(input: &Path, palette: &Path, output: &Path, fps: u32) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-i".into(),
        palette.to_string_lossy().into_owned(),
        "-lavfi".into(),
        format!("fps={fps} [x];[x][1:v] paletteuse=dither=bayer:bayer_scale=5"),
        output.to_string_lossy().into_owned(),
    ]
}

/// WebP 指令：`ffmpeg -y -i <in> -vf mpdecimate,setpts=N/FRAME_RATE/TB -c:v libwebp -quality <q> <out>`
///
/// D1 定案：多幀差異化 — `mpdecimate` 濾除重複幀（TUI 錄製大量靜態
/// 重複幀是 webp 體積爆增主因，實測 30fps 全幀 58x、fps 抽幀後仍 8x，
/// mpdecimate 後靜態素材 368KB、動畫素材 −18%）；`setpts` 重整
/// 時間戳保留原始節奏。終端展演 12~15fps 對 mpdecimate 是自然結果。
pub(crate) fn to_webp_cmd(input: &Path, output: &Path, quality: u8) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        "mpdecimate,setpts=N/FRAME_RATE/TB".into(),
        "-c:v".into(),
        "libwebp".into(),
        "-quality".into(),
        quality.to_string(),
        output.to_string_lossy().into_owned(),
    ]
}

/// 同格式壓縮指令（T4b `--max-size`）：依輸出副檔名選降參數策略
///
/// - webm → `-c:v libvpx-vp9 -crf <q>`（q 越高越小；0-63）
/// 依格式重編碼壓縮：
/// - webm：crf（quality 遞增）
/// - gif：fps 遞減
/// - webp：quality 遞減 + mpdecimate 多幀差異化（webp 動畫體積的主導因子）
pub(crate) fn recompress_cmd(
    input: &Path,
    output: &Path,
    format: &str,
    quality: u8,
    fps: Option<u32>,
) -> Vec<String> {
    let mut cmd = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
    ];
    match format {
        "webm" => {
            cmd.extend([
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-crf".into(),
                quality.to_string(),
                // realtime deadline：壓縮迴圈每輪重編碼，good mode 慢到不實用
                "-deadline".into(),
                "realtime".into(),
            ]);
        }
        "gif" => {
            cmd.extend([
                "-vf".into(),
                format!("fps={}", fps.unwrap_or(quality as u32)),
            ]);
        }
        "webp" => {
            cmd.extend([
                "-vf".into(),
                "mpdecimate,setpts=N/FRAME_RATE/TB".into(),
                "-c:v".into(),
                "libwebp".into(),
                "-quality".into(),
                quality.to_string(),
            ]);
        }
        _ => {}
    }
    cmd.push(output.to_string_lossy().into_owned());
    cmd
}

/// 執行 ffmpeg，失敗回傳錯誤（含 stderr 摘要）
/// 執行 ffmpeg（filmstrip/hstack 等步驟共用）
pub fn run_ffmpeg_exec(args: &[String]) -> Result<()> {
    run_ffmpeg_strict(args, "ffmpeg")
}

/// 探測影片時長（ms）。`ffmpeg -i` 的 stderr `Duration:` 行解析；失敗回 None（lenient）。
pub fn probe_duration_ms(input: &Path) -> Option<u64> {
    let out = Command::new("ffmpeg").arg("-i").arg(input).output().ok()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Duration:"))?;
    // "  Duration: 00:00:04.83, start: ..." → 取 HH:MM:SS.xx
    let rest = line.split("Duration:").nth(1)?.trim();
    let hms = rest.split(',').next()?.trim();
    let mut parts = hms.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    Some(((h * 3600.0 + m * 60.0 + s) * 1000.0) as u64)
}

fn run_ffmpeg_strict(args: &[String], step: &str) -> Result<()> {
    let out = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("無法執行 ffmpeg（{step}）：{e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "ffmpeg {step} 失敗：{}",
            stderr.trim().lines().last().unwrap_or_default()
        );
    }
    Ok(())
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
