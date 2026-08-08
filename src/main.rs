mod cli;
mod config;
mod db;
mod doctor;
mod engine;
mod mcp;
mod media;
mod paths;

// 測試共用：序列化改 process-wide XDG env 的測試（Rust 預設並行，env var 是全域）
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use anyhow::Result;
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
        Commands::Optimize(args) => {
            let opts = media::optimize::OptimizeOptions {
                input: args.input,
                output: args.output,
                format: args.format,
                quality: args.quality,
                fps: args.fps,
                dry_run: args.dry_run,
            };
            media::optimize::optimize(&opts)?;
        }
        Commands::Filmstrip(args) => {
            let output = match &args.output {
                Some(o) => Ok(o.clone()),
                None => {
                    let stem = args
                        .input
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "recording".into());
                    crate::paths::resolve_output_path(&format!("{}-filmstrip.png", stem), None)
                }
            }?;
            let opts = media::filmstrip::FilmstripOptions {
                input: args.input,
                roll: args.roll,
                count: args.count,
                output,
                dry_run: args.dry_run,
            };
            media::filmstrip::filmstrip(&opts)?;
        }
        Commands::Clean(args) => {
            let tracker = db::AssetTracker::open()?;
            let orphans = tracker.orphans(&std::env::current_dir()?)?;
            if orphans.is_empty() {
                println!("無孤兒資產");
            } else {
                for asset in &orphans {
                    println!("{}", tracker.remove(asset, args.dry_run)?);
                }
            }
        }
        Commands::Doctor => doctor::run_doctor(),
        Commands::Mcp => mcp::server::serve()?,
    }

    Ok(())
}
