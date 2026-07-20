mod app;
mod config;
mod s3;
mod ui;

use clap::Parser;
use color_eyre::Result;

#[derive(Parser, Debug)]
#[command(name = "s3nav", version, about = "TUI file browser for Amazon S3")]
pub struct Args {
    /// AWS region (overrides profile region)
    #[arg(short, long)]
    pub region: Option<String>,

    /// AWS profile name from ~/.aws/credentials and ~/.aws/config
    #[arg(short, long)]
    pub profile: Option<String>,

    /// Custom S3 endpoint URL (for S3-compatible services like MinIO)
    #[arg(short, long)]
    pub endpoint_url: Option<String>,

    /// Start directly in this bucket, optionally followed by a prefix (e.g. `my-bucket/some/prefix`)
    #[arg(short, long)]
    pub bucket: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    let connection = s3::ConnectionParams::from_args(&args);
    let client = s3::create_client(&connection).await;

    let terminal = ratatui::init();
    let result = app::App::new(client, args.bucket).run(terminal).await;
    ratatui::restore();

    result
}
