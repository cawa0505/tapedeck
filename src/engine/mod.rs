// Engine abstraction for VHS Tape process management

use std::process::Command;

pub mod vhs;
pub mod wayland;

pub trait Recorder {
    fn record(&self, output: &str, duration: u64) -> anyhow::Result<()>;
}

pub mod vhs {
    use anyhow::Result;

    pub async fn run_tape_file(tape_path: &str, output_path: &str) -> Result<()> {
        let vhs_exe = std::env::var("VHS_BIN").unwrap_or_else(|_| "vhs".to_string());

        let status = Command::new(&vhs_exe)
            .arg(tape_path)
            .arg("-o")
            .arg(output_path)
            .arg("--format")
            .arg("gif")
            .status()?;

        if !status.success() {
            anyhow::bail!("VHS recording failed (exit code: {})", status);
        }
        Ok(())
    }
}

pub mod wayland {
    use anyhow::Result;

    pub async fn record_screen(output_path: &str, duration: u64) -> Result<()> {
        let wf_exe = std::env::var("WF_RECORDER").unwrap_or_else(|_| "wf-recorder".to_string());

        let status = Command::new(&wf_exe)
            .arg("-o")
            .arg(output_path)
            .arg("-d")
            .arg(duration)
            .arg("--wayland")
            .status()?;

        if !status.success() {
            anyhow::bail!("Wayland recording failed (exit code: {})", status);
        }
        Ok(())
    }
}