use clap::Parser;
use std::path::PathBuf;

/// File Processor - A CLI tool for processing and transforming files
#[derive(Parser, Debug, Clone)]
#[command(name = "file-processor")]
#[command(author = "File Processor Team")]
#[command(version = "0.1.0")]
#[command(about = "Process and transform files with various operations")]
pub struct Cli {
    /// Input files to process
    #[arg(short, long, required = true, num_args = 1..)]
    pub input: Vec<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, default_value = "./output")]
    pub output: PathBuf,

    /// Enable verbose output
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// Processing mode: transform, validate, or hash
    #[arg(short, long, default_value = "transform")]
    pub mode: String,

    /// Configuration file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Maximum file size to process (in bytes)
    #[arg(long, default_value_t = 10_000_000)]
    pub max_size: usize,

    /// Number of worker threads
    #[arg(long, default_value_t = 4)]
    pub workers: usize,

    /// Enable encryption for output files
    #[arg(long, default_value_t = false)]
    pub encrypt: bool,

    /// Encryption key (required if --encrypt is set)
    #[arg(long)]
    pub key: Option<String>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.input.is_empty() {
            return Err("At least one input file is required".to_string());
        }

        if self.encrypt && self.key.is_none() {
            return Err("Encryption key is required when --encrypt is enabled".to_string());
        }

        let valid_modes = ["transform", "validate", "hash"];
        if !valid_modes.contains(&self.mode.as_str()) {
            return Err(format!(
                "Invalid mode '{}'. Valid modes: {:?}",
                self.mode, valid_modes
            ));
        }

        Ok(())
    }
}
