//! Configuration for agent runs.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::AgentType;

/// Configuration for running an agent against a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    /// Path to the task directory containing prompt.md and task.yaml.
    pub task_path: PathBuf,
    /// Type of agent to run.
    pub agent_type: AgentType,
    /// Maximum execution time.
    pub timeout: Duration,
    /// Output directory for agent results.
    pub output_dir: PathBuf,
    /// Whether to use Docker isolation.
    pub use_docker: bool,
    /// Docker image to use (if use_docker is true).
    pub docker_image: Option<String>,
    /// Environment variables to pass to the agent.
    pub env_vars: Vec<(String, String)>,
    /// Memory limit in MB (for Docker).
    pub memory_limit_mb: u64,
    /// CPU limit (number of cores, for Docker).
    pub cpu_limit: f64,
    /// Whether to capture detailed execution trace.
    pub capture_trace: bool,
    /// Model to use for the agent (if applicable).
    pub model: Option<String>,
    /// API key for the agent (if applicable).
    pub api_key: Option<String>,
}

impl RunConfig {
    /// Creates a new run configuration with defaults.
    pub fn new(task_path: impl Into<PathBuf>) -> Self {
        Self {
            task_path: task_path.into(),
            agent_type: AgentType::Generic,
            timeout: Duration::from_secs(1800), // 30 minutes default
            output_dir: PathBuf::from("./outputs"),
            use_docker: true,
            docker_image: Some("python:3.11-slim".to_string()),
            env_vars: Vec::new(),
            memory_limit_mb: 4096,
            cpu_limit: 2.0,
            capture_trace: true,
            model: None,
            api_key: None,
        }
    }

    /// Sets the agent type.
    pub fn with_agent(mut self, agent_type: AgentType) -> Self {
        self.agent_type = agent_type;
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the output directory.
    pub fn with_output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = dir.into();
        self
    }

    /// Disables Docker isolation (run locally).
    pub fn without_docker(mut self) -> Self {
        self.use_docker = false;
        self
    }

    /// Sets the Docker image.
    pub fn with_docker_image(mut self, image: impl Into<String>) -> Self {
        self.docker_image = Some(image.into());
        self
    }

    /// Adds an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Sets memory limit in MB.
    pub fn with_memory_limit(mut self, mb: u64) -> Self {
        self.memory_limit_mb = mb;
        self
    }

    /// Sets CPU limit.
    pub fn with_cpu_limit(mut self, cores: f64) -> Self {
        self.cpu_limit = cores;
        self
    }

    /// Enables or disables trace capture.
    pub fn with_trace(mut self, capture: bool) -> Self {
        self.capture_trace = capture;
        self
    }

    /// Sets the model for the agent.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the API key for the agent.
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Gets the prompt file path.
    pub fn prompt_path(&self) -> PathBuf {
        self.task_path.join("prompt.md")
    }

    /// Gets the task.yaml path.
    pub fn task_yaml_path(&self) -> PathBuf {
        self.task_path.join("task.yaml")
    }
}

impl Default for RunConfig {
    fn default() -> Self {
        Self::new("./task")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_config_defaults() {
        let config = RunConfig::new("./test-task");
        assert_eq!(config.task_path, PathBuf::from("./test-task"));
        assert_eq!(config.timeout, Duration::from_secs(1800));
        assert!(config.use_docker);
        assert!(config.capture_trace);
    }

    #[test]
    fn test_run_config_builder() {
        let config = RunConfig::new("./task")
            .with_agent(AgentType::BaseAgent)
            .with_timeout(Duration::from_secs(600))
            .without_docker()
            .with_env("MY_VAR", "value")
            .with_model("gpt-4");

        assert!(matches!(config.agent_type, AgentType::BaseAgent));
        assert_eq!(config.timeout, Duration::from_secs(600));
        assert!(!config.use_docker);
        assert_eq!(config.env_vars.len(), 1);
        assert_eq!(config.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_paths() {
        let config = RunConfig::new("./tasks/my-task");
        assert_eq!(config.prompt_path(), PathBuf::from("./tasks/my-task/prompt.md"));
        assert_eq!(config.task_yaml_path(), PathBuf::from("./tasks/my-task/task.yaml"));
    }
}
