use crate::cli::Cli;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub input_files: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub verbose: bool,
    pub mode: ProcessingMode,
    pub max_file_size: usize,
    pub worker_count: usize,
    pub encryption_enabled: bool,
    pub encryption_key: Option<String>,
    pub transforms: Vec<TransformConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessingMode {
    Transform,
    Validate,
    Hash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformConfig {
    pub name: String,
    pub enabled: bool,
    pub options: std::collections::HashMap<String, String>,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        cli.validate().map_err(|e| anyhow::anyhow!(e))?;

        let base_config = if let Some(config_path) = &cli.config {
            Self::load_from_file(config_path)?
        } else {
            Self::default()
        };

        let mode = match cli.mode.as_str() {
            "transform" => ProcessingMode::Transform,
            "validate" => ProcessingMode::Validate,
            "hash" => ProcessingMode::Hash,
            _ => ProcessingMode::Transform,
        };

        Ok(Config {
            input_files: cli.input.clone(),
            output_dir: cli.output.clone(),
            verbose: cli.verbose,
            mode,
            max_file_size: cli.max_size,
            worker_count: cli.workers,
            encryption_enabled: cli.encrypt,
            encryption_key: cli.key.clone(),
            transforms: base_config.transforms,
        })
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    pub fn get_transform_option(&self, transform_name: &str, key: &str) -> Option<&String> {
        self.transforms
            .iter()
            .find(|t| t.name == transform_name)
            .and_then(|t| t.options.get(key))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            input_files: Vec::new(),
            output_dir: PathBuf::from("./output"),
            verbose: false,
            mode: ProcessingMode::Transform,
            max_file_size: 10_000_000,
            worker_count: 4,
            encryption_enabled: false,
            encryption_key: None,
            transforms: vec![
                TransformConfig {
                    name: "uppercase".to_string(),
                    enabled: true,
                    options: std::collections::HashMap::new(),
                },
                TransformConfig {
                    name: "trim".to_string(),
                    enabled: true,
                    options: std::collections::HashMap::new(),
                },
            ],
        }
    }
}
