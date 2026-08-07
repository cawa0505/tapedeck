mod cli;
mod config;
mod engine;
mod paths;

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
            println!(
                "{}",
                engine::dispatcher::media_link(&args.media_file, &args.format)?
            );
        }
        Commands::Optimize(_) => bail!("optimize is not implemented yet"),
        Commands::Clean(_) => bail!("clean is not implemented yet"),
    }

    Ok(())
}
