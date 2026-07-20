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

    let (configs, load_err) = match config::load() {
        Ok(c) => (c.profiles, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    let cli_flags = args.profile.is_some()
        || args.region.is_some()
        || args.endpoint_url.is_some()
        || args.bucket.is_some();
    let start_in_selector = !cli_flags && !configs.is_empty();

    let connection = s3::ConnectionParams::from_args(&args);
    let client = s3::create_client(&connection).await;

    let terminal = ratatui::init();
    let mut app = app::App::new(client, connection, args.bucket, configs);
    if start_in_selector {
        app.view = app::View::ConfigSelector;
    }
    app.error = load_err;
    let result = app.run(terminal).await;
    ratatui::restore();

    result
}
