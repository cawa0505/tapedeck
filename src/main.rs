mod cli;
mod engine;
mod mcp;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Mcp) => {
            // Start MCP Server (stdio)
            mcp::run_server().await?;
        }
        Some(Commands::Run { tape, output, wayland }) => {
            if *wayland {
                engine::wayland::record_screen(output, 5).await?;
            } else if let Some(tape_path) = tape {
                engine::vhs::run_tape_file(tape_path, output).await?;
            }
        }
        Some(Commands::Tui) | None => {
            // Default direct tapedeck opens fzf-style TUI
            tui::run_app().await?;
        }
    }

    Ok(())
}