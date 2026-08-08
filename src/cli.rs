use clap::{Args, Parser, Subcommand};
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
    /// 產生橫向步驟圖（filmstrip）
    Filmstrip(FilmstripArgs),
    /// 清理孤兒資產
    Clean(CleanArgs),
    /// 檢查系統依賴與硬體能力
    Doctor,
    /// MCP stdio 伺服器（JSON-RPC 2.0 over stdio）
    Mcp,
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
    /// Validate the script and show the selected backend without recording.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct LinkArgs {
    #[arg(required = true)]
    pub media_file: PathBuf,
    #[arg(long, default_value = "md")]
    pub format: String, // zola/md/html
}

#[derive(Args)]
pub struct OptimizeArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: Option<PathBuf>, // 預設依 input 副檔名推斷（XDG）
    #[arg(long)]
    pub format: Option<String>, // gif/webp；預設依 output 副檔名推斷
    #[arg(long, default_value_t = 80)]
    pub quality: u8, // 1-100（webp）
    #[arg(long, default_value_t = 10)]
    pub fps: u32, // palettegen 抽樣 fps
    #[arg(long)]
    pub dry_run: bool, // 顯示指令鏈，不執行
}

#[derive(Args)]
pub struct CleanArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct FilmstripArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(long)]
    pub roll: Option<PathBuf>, // .roll 腳本（時間點來源）
    #[arg(long, default_value_t = 8)]
    pub count: usize, // 最多取幾個操作點
    #[arg(short, long)]
    pub output: Option<PathBuf>, // 預設：<input stem>-filmstrip.png（XDG）
    #[arg(long)]
    pub dry_run: bool,
}
