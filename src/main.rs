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
        Commands::Reroll(args) => {
            if args.dry_run {
                engine::reroll::run_dry_run(&args)?
            } else if engine::reroll_batch::run(&args).await? {
                std::process::exit(1);
            }
        }
        Commands::Doctor => doctor::run_doctor(),
        Commands::Mcp => mcp::server::serve().await?,
        #[cfg(feature = "nexushub")]
        Commands::Register(args) => {
            let token = match args.token {
                Some(t) => t,
                None => {
                    std::env::var("NEXUS_TOKEN")
                        .or_else(|_| {
                            let home = std::env::var("HOME").unwrap_or_default();
                            std::fs::read_to_string(format!("{}/.config/nexus/nexus-token", home))
                                .map(|s| s.trim().to_string())
                        })
                        .map_err(|_| anyhow::anyhow!("未提供 NEXUS_TOKEN，請透過 --token 指定或設定環境變數"))?
                }
            };

            let mut builder = nexus_mcp_sdk::NexusApp::builder("tapedeck")?
                .version(env!("CARGO_PKG_VERSION"));

            // 註冊所有工具並以 SDK 防禦檢查 64 字元與命名
            for tool in mcp::tools::list() {
                builder = builder.register_tool(nexus_mcp_sdk::Tool::new(tool.name, tool.description))?;
            }

            let app = builder.build()?;
            println!("正在向 NexusHub 註冊 tapedeck (hub: {})...", args.hub);
            let resp = app.register(&args.hub, &token, None, vec!["mcp".into()]).await?;
            println!("{resp}");
        }
    }

    Ok(())
}
