use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 執行錄影腳本
    Run(RunArgs),
    /// 產生媒體連結語法
    Link(LinkArgs),
    /// 優化媒體檔案
    Optimize(OptimizeArgs),
    /// 清理孤兒資產
    Clean(CleanArgs),
}

#[derive(Args)]
pub struct RunArgs {
    #[arg(required = true)]
    pub script_file: PathBuf,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub fps: Option<u32>,
    #[arg(long)]
    pub max_size: Option<u32>, // MB
    #[arg(long)]
    pub gif: bool,
    #[arg(long)]
    pub webp: bool,
}

#[derive(Args)]
pub struct LinkArgs {
    #[arg(required = true)]
    pub media_file: PathBuf,
    #[arg(long, default_value = "md"]
    pub format: String, // zola/md/html
}

#[derive(Args)]
pub struct OptimizeArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(short, required = true)]
    pub output: PathBuf,
    #[arg(long, default_value_t = 80)]
    pub quality: u8, // 1-100
}

#[derive(Args)]
pub struct CleanArgs {
    #[arg(long)]
    pub dry_run: bool,
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}