mod cli;
mod config;
mod db;
mod doctor;
mod engine;
mod paths;

// 測試共用：序列化改 process-wide XDG env 的測試（Rust 預設並行，env var 是全域）
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use anyhow::{bail, Result};
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => engine::dispatcher::run(args).await?,
        Commands::Link(args) => {
            // link 同時登錄資產圖譜（Pillar 2：三層關聯的 asset 層）
            let tracker = db::AssetTracker::open()?;
            tracker.register(&args.media_file, None)?;
            println!(
                "{}",
                engine::dispatcher::media_link(&args.media_file, &args.format)?
            );
        }
        Commands::Optimize(_) => bail!("optimize is not implemented yet"),
        Commands::Clean(args) => {
            let tracker = db::AssetTracker::open()?;
            let orphans = tracker.orphans(&std::env::current_dir()?)?;
            if orphans.is_empty() {
                println!("無孤兒資產");
            } else {
                for asset in &orphans {
                    tracker.remove(asset, args.dry_run)?;
                }
            }
        }
        Commands::Doctor => doctor::run_doctor(),
    }

    Ok(())
}
