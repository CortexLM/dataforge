use anyhow::Result;
use clap::Parser;
use file_processor::{cli::Cli, config::Config, run_processor};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    let config = Config::from_cli(&cli)?;
    
    run_processor(config).await?;
    
    Ok(())
}
