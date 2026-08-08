//! optimize — 壓製影片/動圖（P1 media-export / design.md 3）
//!
//! - GIF：palettegen 雙 Pass（pass1 調色盤 → pass2 套用輸出）
//! - WebP：libwebp 直接轉換
//! - 輸出格式由副檔名決定（`.gif` / `.webp`），與 vhs 的容器選擇慣例一致。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::ffmpeg::{self, FfmpegAdapter, FfmpegV1Adapter};

/// optimize 參數（由 cli.rs 組裝）
pub struct OptimizeOptions {
    pub input: PathBuf,
    /// CLI `--output`；None → 依 input 副檔名推斷（XDG）
    pub output: Option<PathBuf>,
    /// CLI `--format`；None → 依 output 副檔名推斷
    pub format: Option<String>,
    pub quality: u8,
    pub fps: u32,
    pub dry_run: bool,
}

/// 執行 optimize（probe 先行，缺能力明確報錯 — design.md 2）
pub fn optimize(opts: &OptimizeOptions) -> Result<()> {
    let adapter = FfmpegV1Adapter::new();
    let caps = adapter
        .probe()
        .context("無法執行 ffmpeg — 請確認已安裝（sudo pacman -S ffmpeg）")?;

    let (input, output, format) = resolve(opts)?;
    let cmds = build_commands(&input, &output, &format, opts.fps, opts.quality, &caps)?;

    if opts.dry_run {
        println!("input:  {}", input.display());
        println!("output: {}", output.display());
        println!("format: {format}");
        for cmd in &cmds {
            println!("$ ffmpeg {}", cmd.0.join(" "));
        }
        return Ok(());
    }

    for (cmd, step) in &cmds {
        let out = std::process::Command::new("ffmpeg")
            .args(cmd)
            .output()
            .with_context(|| format!("無法執行 ffmpeg（{step}）"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "ffmpeg {step} 失敗：{}",
                stderr.trim().lines().last().unwrap_or_default()
            );
        }
    }
    Ok(())
}

/// T4b `--max-size`：錄製完成後檢查輸出大小，超過則同格式重編碼壓縮
///
/// 策略依輸出副檔名：webm → crf 遞增；gif → fps 遞減；webp → quality 遞減。
/// 每輪壓縮後檢查，符合或達下限即停；回傳 (原大小, 最終大小)。
pub fn compress_to_fit(output: &Path, max_mb: u32) -> Result<Option<(u64, u64)>> {
    let max_bytes = u64::from(max_mb) * 1024 * 1024;
    let size = std::fs::metadata(output)?.len();
    if size <= max_bytes {
        return Ok(None);
    }
    let format = ext_of(output).unwrap_or_default();
    let temp = output.with_extension(format!("tmp.{format}"));

    // (初始參數, 每輪步進 ±, 終止參數)：webm crf 遞增；gif fps 遞減；webp quality 遞減
    let (mut param, step, limit): (u8, i8, u8) = match format.as_str() {
        "webm" => (40, 10, 63),
        "gif" => (10, -2, 2),
        "webp" => (50, -10, 20),
        _ => bail!("--max-size 僅支援 webm/gif/webp 輸出（目前：{format}）"),
    };

    let mut current = size;
    loop {
        let cmd = ffmpeg::recompress_cmd(output, &temp, &format, param);
        let out = std::process::Command::new("ffmpeg")
            .args(&cmd)
            .output()
            .context("無法執行 ffmpeg — 請確認已安裝（sudo pacman -S ffmpeg）")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "ffmpeg 壓縮失敗（param={param}）：{}",
                stderr.trim().lines().last().unwrap_or_default()
            );
        }
        let new_size = std::fs::metadata(&temp)?.len();
        if new_size <= max_bytes || param == limit || new_size >= current {
            // 符合、達終止參數、或壓縮無效 — 採用這輪結果
            std::fs::rename(&temp, output)?;
            return Ok(Some((size, new_size)));
        }
        current = new_size;
        param = (i16::from(param) + i16::from(step)).clamp(0, 63) as u8;
    }
}

/// 解析 input/output/format（CLI > output 副檔名 > input 副檔名）
fn resolve(opts: &OptimizeOptions) -> Result<(PathBuf, PathBuf, String)> {
    let input = opts.input.clone();

    // format：CLI 優先；其次 output 副檔名；最後 input 副檔名
    let format = match &opts.format {
        Some(f) => f.to_lowercase(),
        None => {
            let from_output = opts.output.as_ref().and_then(|p| ext_of(p));
            let from_input = ext_of(&input);
            from_output
                .or(from_input)
                .unwrap_or_else(|| "webp".to_string())
        }
    };
    if format != "gif" && format != "webp" {
        bail!("不支援的格式：{format}（僅 gif / webp）");
    }

    // output：CLI 優先；否則依 input 換副檔名
    let output = match &opts.output {
        Some(p) => p.clone(),
        None => {
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "output".to_string());
            crate::paths::cache_dir().join(format!("{stem}.{format}"))
        }
    };
    Ok((input, output, format))
}

fn ext_of(p: &Path) -> Option<String> {
    p.extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// 依格式組裝指令（dry-run 與執行共用）
fn build_commands(
    input: &Path,
    output: &Path,
    format: &str,
    fps: u32,
    quality: u8,
    caps: &ffmpeg::MediaCapabilities,
) -> Result<Vec<(Vec<String>, &'static str)>> {
    match format {
        "gif" => {
            if !caps.has_palettegen {
                bail!("ffmpeg 缺少 palettegen filter（版本過舊）— 請升級 ffmpeg");
            }
            let palette = ffmpeg::palette_path(input);
            Ok(vec![
                (ffmpeg::palettegen_cmd(input, fps, &palette), "palettegen"),
                (
                    ffmpeg::paletteuse_cmd(input, &palette, output, fps),
                    "paletteuse",
                ),
            ])
        }
        "webp" => {
            if !caps.has_libwebp {
                bail!("ffmpeg 缺少 libwebp encoder — 請確認 ffmpeg 為完整版（非 minimal）");
            }
            Ok(vec![(
                ffmpeg::to_webp_cmd(input, output, quality),
                "libwebp",
            )])
        }
        _ => unreachable!("resolve 已過濾格式"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(input: &str, output: Option<&str>, format: Option<&str>) -> OptimizeOptions {
        OptimizeOptions {
            input: PathBuf::from(input),
            output: output.map(PathBuf::from),
            format: format.map(str::to_string),
            quality: 80,
            fps: 10,
            dry_run: true,
        }
    }

    #[test]
    fn format_precedence_cli_over_ext() {
        let o = opts("demo.webm", Some("out.gif"), Some("webp"));
        let (_, _, f) = resolve(&o).unwrap();
        assert_eq!(f, "webp");
    }

    #[test]
    fn format_from_output_ext() {
        let o = opts("demo.webm", Some("out.gif"), None);
        let (_, _, f) = resolve(&o).unwrap();
        assert_eq!(f, "gif");
    }

    #[test]
    fn format_from_input_ext_fallback() {
        let o = opts("demo.gif", None, None);
        let (_, _, f) = resolve(&o).unwrap();
        assert_eq!(f, "gif");
    }

    #[test]
    fn default_output_in_cache_dir() {
        let o = opts("demo.gif", None, None);
        let (_, out, _) = resolve(&o).unwrap();
        assert!(out.ends_with("demo.gif"));
        assert!(out.starts_with(crate::paths::cache_dir()));
    }

    #[test]
    fn unsupported_format_rejected() {
        let o = opts("demo.gif", None, Some("mp4"));
        assert!(resolve(&o).is_err());
    }

    #[test]
    fn gif_chain_has_two_passes() {
        let o = opts("demo.webm", Some("out.gif"), None);
        let (input, output, format) = resolve(&o).unwrap();
        let caps = ffmpeg::MediaCapabilities {
            has_palettegen: true,
            has_libwebp: true,
            ffmpeg_version: "n8".into(),
        };
        let cmds = build_commands(&input, &output, &format, 10, 80, &caps).unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(cmds[0].0.iter().any(|a| a.contains("palettegen")));
        assert!(cmds[1].0.iter().any(|a| a.contains("paletteuse")));
        // paletteuse 應引用 pass1 產生的 palette 路徑
        let palette_arg = ffmpeg::palette_path(&input);
        assert!(cmds[1]
            .0
            .iter()
            .any(|a| a == &palette_arg.to_string_lossy()));
    }

    #[test]
    fn gif_chain_rejects_missing_palettegen() {
        let o = opts("demo.webm", Some("out.gif"), None);
        let (input, output, format) = resolve(&o).unwrap();
        let caps = ffmpeg::MediaCapabilities {
            has_palettegen: false,
            has_libwebp: true,
            ffmpeg_version: "old".into(),
        };
        assert!(build_commands(&input, &output, &format, 10, 80, &caps).is_err());
    }

    #[test]
    fn webp_chain_single_cmd() {
        let o = opts("demo.gif", Some("out.webp"), None);
        let (input, output, format) = resolve(&o).unwrap();
        let caps = ffmpeg::MediaCapabilities {
            has_palettegen: true,
            has_libwebp: true,
            ffmpeg_version: "n8".into(),
        };
        let cmds = build_commands(&input, &output, &format, 10, 80, &caps).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].0.iter().any(|a| a == "libwebp"));
    }

    #[test]
    fn recompress_webm_uses_vp9_crf() {
        let cmd = ffmpeg::recompress_cmd(Path::new("in.webm"), Path::new("out.webm"), "webm", 50);
        assert!(cmd.windows(2).any(|w| w == ["-c:v", "libvpx-vp9"]));
        assert!(cmd.windows(2).any(|w| w == ["-crf", "50"]));
    }

    #[test]
    fn recompress_gif_lowers_fps() {
        let cmd = ffmpeg::recompress_cmd(Path::new("in.gif"), Path::new("out.gif"), "gif", 4);
        assert!(cmd.windows(2).any(|w| w == ["-vf", "fps=4"]));
    }

    #[test]
    fn recompress_webp_lowers_quality() {
        let cmd = ffmpeg::recompress_cmd(Path::new("in.webp"), Path::new("out.webp"), "webp", 30);
        assert!(cmd.windows(2).any(|w| w == ["-quality", "30"]));
    }

    /// 真實 ffmpeg 迴圈壓縮（需 ffmpeg 且耗時，手動跑：cargo test -- --ignored）
    #[test]
    #[ignore]
    fn compress_to_fit_shrinks_real_webm() {
        let dir = std::env::temp_dir().join("tapedeck-maxsize-test");
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.webm");
        let _ = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=duration=6:size=1280x720:rate=30",
                "-c:v",
                "libvpx-vp9",
                "-b:v",
                "8M",
                "-deadline",
                "realtime",
            ])
            .arg(&big)
            .status()
            .expect("ffmpeg 產生測試檔");
        let before = std::fs::metadata(&big).unwrap().len();
        assert!(before > 1_000_000, "測試檔應 >1MB（實際 {before}）");

        let (orig, final_size) = compress_to_fit(&big, 1).unwrap().unwrap();
        assert!(
            final_size <= 1_048_576,
            "壓縮後應 ≤1MB（實際 {final_size}，原 {orig}）"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
