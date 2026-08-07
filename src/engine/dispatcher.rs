use crate::cli::RunArgs;
use anyhow::{bail, Context, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    Tape,
    Roll,
}

impl ScriptKind {
    fn backend(self) -> &'static str {
        match self {
            Self::Tape => "VHS",
            Self::Roll => "Wayland",
        }
    }
}

pub fn script_kind(path: &Path) -> Result<ScriptKind> {
    if !path.is_file() {
        bail!("script does not exist or is not a file: {}", path.display());
    }

    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("tape") => Ok(ScriptKind::Tape),
        Some(extension) if extension.eq_ignore_ascii_case("roll") => Ok(ScriptKind::Roll),
        _ => bail!("unsupported script type; expected .tape or .roll"),
    }
}

pub async fn run(args: RunArgs) -> Result<()> {
    let kind = script_kind(&args.script_file)?;

    if args.gif && args.webp {
        bail!("--gif and --webp cannot be used together");
    }

    if args.dry_run {
        println!(
            "{} -> {} backend",
            args.script_file.display(),
            kind.backend()
        );
        return Ok(());
    }

    match kind {
        ScriptKind::Tape => super::run_vhs(&args.script_file).await,
        ScriptKind::Roll => bail!(
            ".roll validation succeeded, but the Wayland runner is not implemented yet; use --dry-run"
        ),
    }
}

pub fn media_link(path: &Path, format: &str) -> Result<String> {
    let target = path.to_string_lossy();
    let label = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("media path must have a UTF-8 file name")?;

    match format.to_ascii_lowercase().as_str() {
        "md" => Ok(format!("![{label}]({target})")),
        "html" => Ok(format!("<img src=\"{target}\" alt=\"{label}\">")),
        "zola" => Ok(format!(
            "{{{{ image(path=\"{target}\", alt=\"{label}\") }}}}"
        )),
        _ => bail!("unsupported link format; expected md, html, or zola"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_script(extension: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tapedeck-dispatcher-{}-{}.{}",
            std::process::id(),
            extension,
            extension
        ));
        fs::write(&path, "# test\n").unwrap();
        path
    }

    #[test]
    fn detects_supported_script_types() {
        let tape = temp_script("tape");
        let roll = temp_script("roll");

        assert_eq!(script_kind(&tape).unwrap(), ScriptKind::Tape);
        assert_eq!(script_kind(&roll).unwrap(), ScriptKind::Roll);

        fs::remove_file(tape).unwrap();
        fs::remove_file(roll).unwrap();
    }

    #[test]
    fn formats_media_links() {
        let path = PathBuf::from("assets/demo.webm");
        assert_eq!(
            media_link(&path, "md").unwrap(),
            "![demo.webm](assets/demo.webm)"
        );
        assert!(media_link(&path, "unknown").is_err());
    }
}
