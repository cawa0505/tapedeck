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
}
