use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "tapedeck")]
#[command(about = "Three-seat (MCP/CLI/TUI) ultra-fast media recording toolbox for geeks and AI Code Agents", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start fzf-style TUI interface
    Tui,
    /// Start stdio MCP Server (for OpenCode / Cursor integration)
    Mcp,
    /// Run tape or Wayland screen recording
    Run {
        /// Path to .tape file
        #[arg(short, long)]
        tape: Option<std::path::PathBuf>,
        /// Output file path
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// Record via Wayland (wf-recorder)
        #[arg(long)]
        wayland: bool,
    },
}