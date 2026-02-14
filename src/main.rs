//! swe_forge CLI entry point.
//!
//! Initializes logging and delegates to the CLI module for command handling.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI arguments first to get log_level
    let cli = swe_forge::cli::parse_cli();

    // Initialize tracing with environment filter
    // Priority: RUST_LOG env var > --log-level CLI arg > default "info"
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| cli.log_level.clone());

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_filter)))
        .init();

    // Run the CLI with parsed arguments
    swe_forge::cli::run_with_cli(cli).await
}
