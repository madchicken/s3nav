mod app;
mod s3;
mod ui;

use clap::Parser;
use color_eyre::Result;
use std::env;

#[derive(Parser, Debug)]
#[command(name = "s3nav", version, about = "TUI file browser for Amazon S3")]
pub struct Args {
    /// AWS region (e.g. eu-west-1)
    #[arg(short, long, default_value = "us-east-1")]
    pub region: String,

    /// Custom S3 endpoint URL (for S3-compatible services like MinIO)
    #[arg(short, long)]
    pub endpoint_url: Option<String>,

    /// Start directly in this bucket
    #[arg(short, long)]
    pub bucket: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // Validate required env vars before doing anything else
    for var in ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"] {
        if env::var(var).is_err() {
            eprintln!("Error: environment variable {var} is not set");
            std::process::exit(1);
        }
    }

    let args = Args::parse();
    let client = s3::create_client(&args).await;

    let terminal = ratatui::init();
    let result = app::App::new(client, args.bucket).run(terminal).await;
    ratatui::restore();

    result
}
