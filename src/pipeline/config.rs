//! Pipeline configuration for the orchestrator.
//!
//! This module provides configuration options for the data generation pipeline,
//! including execution limits, Docker settings, LLM options, quality filtering,
//! storage paths, and budget constraints.

use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during configuration operations.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A required environment variable is missing.
    #[error("Missing environment variable: {0}")]
    MissingEnvVar(String),

    /// An environment variable has an invalid value.
    #[error("Invalid value for {key}: {message}")]
    InvalidValue { key: String, message: String },

    /// Configuration validation failed.
    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),

    /// IO error while reading configuration.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for the pipeline orchestrator.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    // Execution settings
    /// Maximum number of tasks to run concurrently.
    pub max_concurrent_tasks: usize,
    /// Timeout for individual task execution.
    pub task_timeout: Duration,
    /// Maximum steps allowed per task before terminating.
    pub max_steps_per_task: usize,

    // Docker settings
    /// Docker image to use for task execution.
    pub docker_image: String,
    /// Memory limit for Docker containers (in MB).
    pub docker_memory_mb: u64,
    /// CPU cores allocated to Docker containers.
    pub docker_cpu_cores: f64,

    // LLM settings
    /// Default model to use for generation.
    pub default_model: String,
    /// Fallback models to try if the default fails.
    pub fallback_models: Vec<String>,
    /// Temperature for LLM generation.
    pub temperature: f64,

    // Quality filtering settings
    /// Minimum quality score to keep a trajectory.
    pub min_quality_score: f64,
    /// Whether to enable deduplication of similar trajectories.
    pub enable_deduplication: bool,
    /// Similarity threshold for deduplication (0.0-1.0).
    pub similarity_threshold: f64,

    // Storage settings
    /// PostgreSQL database connection URL.
    pub database_url: String,
    /// Path for storing artifacts (logs, files, etc.).
    pub artifact_path: PathBuf,
    /// Path for storing trajectory files.
    pub trajectory_path: PathBuf,

    // Budget settings
    /// Maximum daily spending limit in dollars.
    pub daily_budget: f64,
    /// Maximum monthly spending limit in dollars.
    pub monthly_budget: f64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            // Execution defaults
            max_concurrent_tasks: 4,
            task_timeout: Duration::from_secs(1800), // 30 minutes
            max_steps_per_task: 50,

            // Docker defaults
            docker_image: "python:3.11-slim".to_string(),
            docker_memory_mb: 2048,
            docker_cpu_cores: 2.0,

            // LLM defaults
            default_model: "gpt-4".to_string(),
            fallback_models: vec!["gpt-3.5-turbo".to_string()],
            temperature: 0.7,

            // Quality defaults
            min_quality_score: 0.6,
            enable_deduplication: true,
            similarity_threshold: 0.85,

            // Storage defaults
            database_url: "postgres://localhost/synth_bench".to_string(),
            artifact_path: PathBuf::from("./artifacts"),
            trajectory_path: PathBuf::from("./trajectories"),

            // Budget defaults
            daily_budget: 100.0,
            monthly_budget: 1000.0,
        }
    }
}

impl PipelineConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// - `PIPELINE_MAX_CONCURRENT_TASKS`: Maximum concurrent tasks (default: 4)
    /// - `PIPELINE_TASK_TIMEOUT_SECS`: Task timeout in seconds (default: 1800)
    /// - `PIPELINE_MAX_STEPS`: Maximum steps per task (default: 50)
    /// - `PIPELINE_DOCKER_IMAGE`: Docker image (default: python:3.11-slim)
    /// - `PIPELINE_DOCKER_MEMORY_MB`: Docker memory limit (default: 2048)
    /// - `PIPELINE_DOCKER_CPU_CORES`: Docker CPU cores (default: 2.0)
    /// - `PIPELINE_DEFAULT_MODEL`: Default LLM model (default: gpt-4)
    /// - `PIPELINE_FALLBACK_MODELS`: Comma-separated fallback models
    /// - `PIPELINE_TEMPERATURE`: LLM temperature (default: 0.7)
    /// - `PIPELINE_MIN_QUALITY_SCORE`: Minimum quality score (default: 0.6)
    /// - `PIPELINE_ENABLE_DEDUP`: Enable deduplication (default: true)
    /// - `PIPELINE_SIMILARITY_THRESHOLD`: Similarity threshold (default: 0.85)
    /// - `DATABASE_URL`: PostgreSQL connection URL (required)
    /// - `PIPELINE_ARTIFACT_PATH`: Artifact storage path (default: ./artifacts)
    /// - `PIPELINE_TRAJECTORY_PATH`: Trajectory storage path (default: ./trajectories)
    /// - `PIPELINE_DAILY_BUDGET`: Daily budget in dollars (default: 100.0)
    /// - `PIPELINE_MONTHLY_BUDGET`: Monthly budget in dollars (default: 1000.0)
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if required variables are missing or have invalid values.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = Self::default();

        // Execution settings
        if let Ok(val) = std::env::var("PIPELINE_MAX_CONCURRENT_TASKS") {
            config.max_concurrent_tasks = parse_env_value(&val, "PIPELINE_MAX_CONCURRENT_TASKS")?;
        }

        if let Ok(val) = std::env::var("PIPELINE_TASK_TIMEOUT_SECS") {
            let secs: u64 = parse_env_value(&val, "PIPELINE_TASK_TIMEOUT_SECS")?;
            config.task_timeout = Duration::from_secs(secs);
        }

        if let Ok(val) = std::env::var("PIPELINE_MAX_STEPS") {
            config.max_steps_per_task = parse_env_value(&val, "PIPELINE_MAX_STEPS")?;
        }

        // Docker settings
        if let Ok(val) = std::env::var("PIPELINE_DOCKER_IMAGE") {
            config.docker_image = val;
        }

        if let Ok(val) = std::env::var("PIPELINE_DOCKER_MEMORY_MB") {
            config.docker_memory_mb = parse_env_value(&val, "PIPELINE_DOCKER_MEMORY_MB")?;
        }

        if let Ok(val) = std::env::var("PIPELINE_DOCKER_CPU_CORES") {
            config.docker_cpu_cores = parse_env_value(&val, "PIPELINE_DOCKER_CPU_CORES")?;
        }

        // LLM settings
        if let Ok(val) = std::env::var("PIPELINE_DEFAULT_MODEL") {
            config.default_model = val;
        }

        if let Ok(val) = std::env::var("PIPELINE_FALLBACK_MODELS") {
            config.fallback_models = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        if let Ok(val) = std::env::var("PIPELINE_TEMPERATURE") {
            config.temperature = parse_env_value(&val, "PIPELINE_TEMPERATURE")?;
        }

        // Quality settings
        if let Ok(val) = std::env::var("PIPELINE_MIN_QUALITY_SCORE") {
            config.min_quality_score = parse_env_value(&val, "PIPELINE_MIN_QUALITY_SCORE")?;
        }

        if let Ok(val) = std::env::var("PIPELINE_ENABLE_DEDUP") {
            config.enable_deduplication = parse_env_bool(&val, "PIPELINE_ENABLE_DEDUP")?;
        }

        if let Ok(val) = std::env::var("PIPELINE_SIMILARITY_THRESHOLD") {
            config.similarity_threshold = parse_env_value(&val, "PIPELINE_SIMILARITY_THRESHOLD")?;
        }

        // Storage settings - DATABASE_URL is required
        config.database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?;

        if let Ok(val) = std::env::var("PIPELINE_ARTIFACT_PATH") {
            config.artifact_path = PathBuf::from(val);
        }

        if let Ok(val) = std::env::var("PIPELINE_TRAJECTORY_PATH") {
            config.trajectory_path = PathBuf::from(val);
        }

        // Budget settings
        if let Ok(val) = std::env::var("PIPELINE_DAILY_BUDGET") {
            config.daily_budget = parse_env_value(&val, "PIPELINE_DAILY_BUDGET")?;
        }

        if let Ok(val) = std::env::var("PIPELINE_MONTHLY_BUDGET") {
            config.monthly_budget = parse_env_value(&val, "PIPELINE_MONTHLY_BUDGET")?;
        }

        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration values.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::ValidationFailed` if any values are invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Execution validation
        if self.max_concurrent_tasks == 0 {
            return Err(ConfigError::ValidationFailed(
                "max_concurrent_tasks must be greater than 0".to_string(),
            ));
        }

        if self.max_steps_per_task == 0 {
            return Err(ConfigError::ValidationFailed(
                "max_steps_per_task must be greater than 0".to_string(),
            ));
        }

        if self.task_timeout.as_secs() == 0 {
            return Err(ConfigError::ValidationFailed(
                "task_timeout must be greater than 0".to_string(),
            ));
        }

        // Docker validation
        if self.docker_image.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "docker_image cannot be empty".to_string(),
            ));
        }

        if self.docker_memory_mb < 256 {
            return Err(ConfigError::ValidationFailed(
                "docker_memory_mb must be at least 256 MB".to_string(),
            ));
        }

        if self.docker_cpu_cores <= 0.0 {
            return Err(ConfigError::ValidationFailed(
                "docker_cpu_cores must be greater than 0".to_string(),
            ));
        }

        // LLM validation
        if self.default_model.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "default_model cannot be empty".to_string(),
            ));
        }

        if !(0.0..=2.0).contains(&self.temperature) {
            return Err(ConfigError::ValidationFailed(
                "temperature must be between 0.0 and 2.0".to_string(),
            ));
        }

        // Quality validation
        if !(0.0..=1.0).contains(&self.min_quality_score) {
            return Err(ConfigError::ValidationFailed(
                "min_quality_score must be between 0.0 and 1.0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.similarity_threshold) {
            return Err(ConfigError::ValidationFailed(
                "similarity_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Storage validation
        if self.database_url.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "database_url cannot be empty".to_string(),
            ));
        }

        // Budget validation
        if self.daily_budget < 0.0 {
            return Err(ConfigError::ValidationFailed(
                "daily_budget cannot be negative".to_string(),
            ));
        }

        if self.monthly_budget < 0.0 {
            return Err(ConfigError::ValidationFailed(
                "monthly_budget cannot be negative".to_string(),
            ));
        }

        if self.daily_budget > self.monthly_budget {
            return Err(ConfigError::ValidationFailed(
                "daily_budget cannot exceed monthly_budget".to_string(),
            ));
        }

        Ok(())
    }

    /// Builder method to set max concurrent tasks.
    pub fn with_max_concurrent_tasks(mut self, max: usize) -> Self {
        self.max_concurrent_tasks = max;
        self
    }

    /// Builder method to set task timeout.
    pub fn with_task_timeout(mut self, timeout: Duration) -> Self {
        self.task_timeout = timeout;
        self
    }

    /// Builder method to set max steps per task.
    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps_per_task = max;
        self
    }

    /// Builder method to set Docker image.
    pub fn with_docker_image(mut self, image: impl Into<String>) -> Self {
        self.docker_image = image.into();
        self
    }

    /// Builder method to set Docker memory limit.
    pub fn with_docker_memory_mb(mut self, memory: u64) -> Self {
        self.docker_memory_mb = memory;
        self
    }

    /// Builder method to set Docker CPU cores.
    pub fn with_docker_cpu_cores(mut self, cores: f64) -> Self {
        self.docker_cpu_cores = cores;
        self
    }

    /// Builder method to set default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Builder method to set fallback models.
    pub fn with_fallback_models(mut self, models: Vec<String>) -> Self {
        self.fallback_models = models;
        self
    }

    /// Builder method to set temperature.
    pub fn with_temperature(mut self, temp: f64) -> Self {
        self.temperature = temp;
        self
    }

    /// Builder method to set minimum quality score.
    pub fn with_min_quality_score(mut self, score: f64) -> Self {
        self.min_quality_score = score;
        self
    }

    /// Builder method to enable or disable deduplication.
    pub fn with_deduplication(mut self, enabled: bool) -> Self {
        self.enable_deduplication = enabled;
        self
    }

    /// Builder method to set similarity threshold.
    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Builder method to set database URL.
    pub fn with_database_url(mut self, url: impl Into<String>) -> Self {
        self.database_url = url.into();
        self
    }

    /// Builder method to set artifact path.
    pub fn with_artifact_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.artifact_path = path.into();
        self
    }

    /// Builder method to set trajectory path.
    pub fn with_trajectory_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.trajectory_path = path.into();
        self
    }

    /// Builder method to set daily budget.
    pub fn with_daily_budget(mut self, budget: f64) -> Self {
        self.daily_budget = budget;
        self
    }

    /// Builder method to set monthly budget.
    pub fn with_monthly_budget(mut self, budget: f64) -> Self {
        self.monthly_budget = budget;
        self
    }
}

/// Parse an environment variable value into a type.
fn parse_env_value<T: std::str::FromStr>(value: &str, key: &str) -> Result<T, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidValue {
        key: key.to_string(),
        message: format!("could not parse '{}'", value),
    })
}

/// Parse an environment variable as a boolean.
fn parse_env_bool(value: &str, key: &str) -> Result<bool, ConfigError> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::InvalidValue {
            key: key.to_string(),
            message: format!("expected boolean value, got '{}'", value),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PipelineConfig::default();
        assert_eq!(config.max_concurrent_tasks, 4);
        assert_eq!(config.task_timeout, Duration::from_secs(1800));
        assert_eq!(config.max_steps_per_task, 50);
        assert_eq!(config.docker_image, "python:3.11-slim");
        assert_eq!(config.docker_memory_mb, 2048);
        assert!((config.docker_cpu_cores - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.default_model, "gpt-4");
        assert!((config.temperature - 0.7).abs() < f64::EPSILON);
        assert!((config.min_quality_score - 0.6).abs() < f64::EPSILON);
        assert!(config.enable_deduplication);
    }

    #[test]
    fn test_config_builder() {
        let config = PipelineConfig::new()
            .with_max_concurrent_tasks(8)
            .with_task_timeout(Duration::from_secs(3600))
            .with_max_steps(100)
            .with_docker_image("ubuntu:22.04")
            .with_docker_memory_mb(4096)
            .with_docker_cpu_cores(4.0)
            .with_default_model("claude-3-opus")
            .with_temperature(0.5)
            .with_min_quality_score(0.8)
            .with_deduplication(false)
            .with_database_url("postgres://test/db");

        assert_eq!(config.max_concurrent_tasks, 8);
        assert_eq!(config.task_timeout, Duration::from_secs(3600));
        assert_eq!(config.max_steps_per_task, 100);
        assert_eq!(config.docker_image, "ubuntu:22.04");
        assert_eq!(config.docker_memory_mb, 4096);
        assert!((config.docker_cpu_cores - 4.0).abs() < f64::EPSILON);
        assert_eq!(config.default_model, "claude-3-opus");
        assert!((config.temperature - 0.5).abs() < f64::EPSILON);
        assert!((config.min_quality_score - 0.8).abs() < f64::EPSILON);
        assert!(!config.enable_deduplication);
        assert_eq!(config.database_url, "postgres://test/db");
    }

    #[test]
    fn test_validation_valid_config() {
        let config = PipelineConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_invalid_concurrent_tasks() {
        let config = PipelineConfig::default().with_max_concurrent_tasks(0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_concurrent_tasks"));
    }

    #[test]
    fn test_validation_invalid_max_steps() {
        let config = PipelineConfig::default().with_max_steps(0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_steps_per_task"));
    }

    #[test]
    fn test_validation_invalid_timeout() {
        let config = PipelineConfig::default().with_task_timeout(Duration::from_secs(0));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task_timeout"));
    }

    #[test]
    fn test_validation_empty_docker_image() {
        let config = PipelineConfig::default().with_docker_image("");
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("docker_image"));
    }

    #[test]
    fn test_validation_low_memory() {
        let config = PipelineConfig::default().with_docker_memory_mb(100);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("docker_memory_mb"));
    }

    #[test]
    fn test_validation_invalid_cpu() {
        let config = PipelineConfig::default().with_docker_cpu_cores(0.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("docker_cpu_cores"));
    }

    #[test]
    fn test_validation_empty_model() {
        let config = PipelineConfig::default().with_default_model("");
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("default_model"));
    }

    #[test]
    fn test_validation_invalid_temperature() {
        let config = PipelineConfig::default().with_temperature(3.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temperature"));
    }

    #[test]
    fn test_validation_invalid_quality_score() {
        let config = PipelineConfig::default().with_min_quality_score(1.5);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("min_quality_score"));
    }

    #[test]
    fn test_validation_invalid_similarity_threshold() {
        let config = PipelineConfig::default().with_similarity_threshold(-0.1);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("similarity_threshold"));
    }

    #[test]
    fn test_validation_empty_database_url() {
        let config = PipelineConfig::default().with_database_url("");
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("database_url"));
    }

    #[test]
    fn test_validation_negative_budget() {
        let config = PipelineConfig::default().with_daily_budget(-10.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("daily_budget"));
    }

    #[test]
    fn test_validation_budget_exceeds_monthly() {
        let config = PipelineConfig::default()
            .with_daily_budget(500.0)
            .with_monthly_budget(100.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("daily_budget cannot exceed"));
    }

    #[test]
    fn test_parse_env_bool() {
        assert!(parse_env_bool("true", "test").unwrap());
        assert!(parse_env_bool("1", "test").unwrap());
        assert!(parse_env_bool("yes", "test").unwrap());
        assert!(parse_env_bool("on", "test").unwrap());
        assert!(parse_env_bool("TRUE", "test").unwrap());

        assert!(!parse_env_bool("false", "test").unwrap());
        assert!(!parse_env_bool("0", "test").unwrap());
        assert!(!parse_env_bool("no", "test").unwrap());
        assert!(!parse_env_bool("off", "test").unwrap());

        assert!(parse_env_bool("invalid", "test").is_err());
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::MissingEnvVar("TEST_VAR".to_string());
        assert!(err.to_string().contains("TEST_VAR"));

        let err = ConfigError::InvalidValue {
            key: "KEY".to_string(),
            message: "bad value".to_string(),
        };
        assert!(err.to_string().contains("KEY"));
        assert!(err.to_string().contains("bad value"));

        let err = ConfigError::ValidationFailed("test failure".to_string());
        assert!(err.to_string().contains("test failure"));
    }
}
