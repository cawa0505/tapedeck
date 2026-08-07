pub mod dispatcher;

use anyhow::{bail, Context, Result};
use std::path::Path;
use tokio::process::Command;

pub async fn run_vhs(script: &Path) -> Result<()> {
    let executable = std::env::var("VHS_BIN").unwrap_or_else(|_| "vhs".to_owned());
    let status = Command::new(&executable)
        .arg(script)
        .status()
        .await
        .with_context(|| format!("failed to start {executable}; install VHS or set VHS_BIN"))?;

    if !status.success() {
        bail!("VHS exited with {status}");
    }

    Ok(())
}
