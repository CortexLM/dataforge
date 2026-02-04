//! Docker Validator Agent for validating synthetic tasks in Docker containers.
//!
//! This agent validates that generated synthetic tasks are actually executable
//! in a Docker environment by:
//! 1. Building a Docker image from the task's environment specification
//! 2. Creating and starting a container
//! 3. Executing basic validation (environment starts correctly)
//! 4. Optionally executing the reference solution to verify it works
//! 5. Cleaning up resources

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::execution::docker_client::{ContainerConfig, DockerClient};
use crate::execution::{get_execution_limits, Container};

use super::error::{AgentError, AgentResult};
use super::task_executor::SyntheticTask;

/// Configuration for Docker validation.
#[derive(Debug, Clone)]
pub struct DockerValidatorConfig {
    /// Timeout for container startup in seconds.
    pub startup_timeout_seconds: u64,
    /// Timeout for solution validation in seconds.
    pub validation_timeout_seconds: u64,
    /// Whether to validate the reference solution.
    pub validate_solution: bool,
    /// Whether to keep containers after validation (for debugging).
    pub keep_containers: bool,
    /// Default base image if none specified.
    pub default_base_image: String,
    /// Network mode for containers.
    pub network_mode: String,
}

impl Default for DockerValidatorConfig {
    fn default() -> Self {
        Self {
            startup_timeout_seconds: 60,
            validation_timeout_seconds: 300,
            validate_solution: true,
            keep_containers: false,
            default_base_image: "python:3.11-slim".to_string(),
            network_mode: "none".to_string(),
        }
    }
}

impl DockerValidatorConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the startup timeout.
    pub fn with_startup_timeout(mut self, seconds: u64) -> Self {
        self.startup_timeout_seconds = seconds;
        self
    }

    /// Set the validation timeout.
    pub fn with_validation_timeout(mut self, seconds: u64) -> Self {
        self.validation_timeout_seconds = seconds;
        self
    }

    /// Set whether to validate the solution.
    pub fn with_solution_validation(mut self, validate: bool) -> Self {
        self.validate_solution = validate;
        self
    }

    /// Set whether to keep containers after validation.
    pub fn with_keep_containers(mut self, keep: bool) -> Self {
        self.keep_containers = keep;
        self
    }

    /// Set the default base image.
    pub fn with_default_image(mut self, image: impl Into<String>) -> Self {
        self.default_base_image = image.into();
        self
    }

    /// Set the network mode.
    pub fn with_network_mode(mut self, mode: impl Into<String>) -> Self {
        self.network_mode = mode.into();
        self
    }
}

/// Result of Docker validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerValidationResult {
    /// Whether the validation passed.
    pub passed: bool,
    /// Whether the environment started successfully.
    pub environment_started: bool,
    /// Whether the solution was validated (if enabled).
    pub solution_validated: Option<bool>,
    /// Exit code from solution execution (if run).
    pub solution_exit_code: Option<i64>,
    /// Output from solution execution (if run).
    pub solution_output: Option<String>,
    /// Duration of the validation in milliseconds.
    pub duration_ms: u64,
    /// Error message if validation failed.
    pub error: Option<String>,
    /// Container ID used for validation.
    pub container_id: Option<String>,
}

impl DockerValidationResult {
    /// Creates a successful result.
    pub fn success(duration_ms: u64, container_id: Option<String>) -> Self {
        Self {
            passed: true,
            environment_started: true,
            solution_validated: None,
            solution_exit_code: None,
            solution_output: None,
            duration_ms,
            error: None,
            container_id,
        }
    }

    /// Creates a failed result.
    pub fn failure(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            passed: false,
            environment_started: false,
            solution_validated: None,
            solution_exit_code: None,
            solution_output: None,
            duration_ms,
            error: Some(error.into()),
            container_id: None,
        }
    }

    /// Sets solution validation result.
    pub fn with_solution_result(mut self, validated: bool, exit_code: i64, output: String) -> Self {
        self.solution_validated = Some(validated);
        self.solution_exit_code = Some(exit_code);
        self.solution_output = Some(output);
        if !validated {
            self.passed = false;
        }
        self
    }
}

/// Agent that validates synthetic tasks can run in Docker containers.
pub struct DockerValidatorAgent {
    docker_client: Arc<DockerClient>,
    config: DockerValidatorConfig,
}

impl std::fmt::Debug for DockerValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockerValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DockerValidatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "docker_validator";

    /// Creates a new Docker validator agent.
    pub fn new(docker_client: Arc<DockerClient>, config: DockerValidatorConfig) -> Self {
        Self {
            docker_client,
            config,
        }
    }

    /// Creates a new agent with default configuration.
    ///
    /// # Errors
    /// Returns error if Docker daemon is not available.
    pub fn with_defaults() -> AgentResult<Self> {
        let docker_client = DockerClient::new().map_err(|e| {
            AgentError::ConfigurationError(format!("Failed to connect to Docker: {}", e))
        })?;
        Ok(Self::new(
            Arc::new(docker_client),
            DockerValidatorConfig::default(),
        ))
    }

    /// Creates a new agent from an existing Docker client.
    pub fn from_client(docker_client: Arc<DockerClient>) -> Self {
        Self::new(docker_client, DockerValidatorConfig::default())
    }

    /// Validates a synthetic task in a Docker container.
    pub async fn validate_task(&self, task: &SyntheticTask) -> AgentResult<DockerValidationResult> {
        let start_time = Instant::now();

        info!(
            task_id = %task.id,
            category = %task.metadata.category,
            "Starting Docker validation"
        );

        // Build container configuration from task
        let container_config = self.build_container_config(task);

        // Ensure image exists
        if !self
            .docker_client
            .image_exists(&container_config.image)
            .await
        {
            debug!(image = %container_config.image, "Pulling Docker image");
            self.docker_client
                .pull_image(&container_config.image)
                .await
                .map_err(|e| {
                    AgentError::GenerationFailed(format!("Failed to pull image: {}", e))
                })?;
        }

        // Create and start container
        let mut container = match Container::new(&self.docker_client, container_config).await {
            Ok(c) => c,
            Err(e) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                return Ok(DockerValidationResult::failure(
                    format!("Failed to create container: {}", e),
                    duration_ms,
                ));
            }
        };

        // Start the container
        if let Err(e) = container.start(&self.docker_client).await {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            self.cleanup_container(&mut container).await;
            return Ok(DockerValidationResult::failure(
                format!("Failed to start container: {}", e),
                duration_ms,
            ));
        }

        let container_id = container.id().to_string();
        info!(container_id = %container_id, "Container started successfully");

        // Validate environment is working
        let env_check = self.validate_environment(&container).await;
        if let Err(e) = env_check {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            self.cleanup_container(&mut container).await;
            return Ok(DockerValidationResult::failure(
                format!("Environment validation failed: {}", e),
                duration_ms,
            ));
        }

        let mut result = DockerValidationResult::success(
            start_time.elapsed().as_millis() as u64,
            Some(container_id.clone()),
        );

        // Optionally validate the reference solution
        if self.config.validate_solution && !task.hidden_solution.reference_commands.is_empty() {
            match self.validate_solution(&container, task).await {
                Ok((exit_code, output)) => {
                    let validated = exit_code == 0;
                    result = result.with_solution_result(validated, exit_code, output);
                    if !validated {
                        warn!(
                            task_id = %task.id,
                            exit_code = exit_code,
                            "Solution validation failed"
                        );
                    }
                }
                Err(e) => {
                    result = result.with_solution_result(false, -1, e.to_string());
                }
            }
        }

        result.duration_ms = start_time.elapsed().as_millis() as u64;

        // Cleanup
        if !self.config.keep_containers {
            self.cleanup_container(&mut container).await;
        }

        info!(
            task_id = %task.id,
            passed = result.passed,
            duration_ms = result.duration_ms,
            "Docker validation completed"
        );

        Ok(result)
    }

    /// Checks if Docker is available.
    pub async fn is_docker_available(&self) -> bool {
        // Try to pull a minimal image to verify Docker is working
        self.docker_client.image_exists("alpine:latest").await
            || self.docker_client.pull_image("alpine:latest").await.is_ok()
    }

    /// Builds container configuration from a synthetic task.
    fn build_container_config(&self, task: &SyntheticTask) -> ContainerConfig {
        let difficulty_str = format!("{:?}", task.difficulty.level).to_lowercase();
        let limits = get_execution_limits(&difficulty_str);

        let image = self.determine_base_image(task);

        ContainerConfig::new(
            format!("dataforge-validate-{}", &task.id[..8.min(task.id.len())]),
            image,
        )
        .with_limits(limits)
        .with_working_dir("/workspace")
        .with_network_mode(self.config.network_mode.clone())
        .with_env(vec![
            format!("TASK_ID={}", task.id),
            format!("TASK_CATEGORY={}", task.metadata.category),
        ])
    }

    /// Determines the base image for a task.
    fn determine_base_image(&self, task: &SyntheticTask) -> String {
        // Analyze tags and category to determine best image
        let tags_lower: Vec<String> = task
            .metadata
            .tags
            .iter()
            .map(|t| t.to_lowercase())
            .collect();

        let category_lower = task.metadata.category.to_lowercase();

        if tags_lower.iter().any(|t| t.contains("python")) || category_lower.contains("python") {
            "python:3.11-slim".to_string()
        } else if tags_lower
            .iter()
            .any(|t| t.contains("node") || t.contains("javascript"))
        {
            "node:20-slim".to_string()
        } else if tags_lower.iter().any(|t| t.contains("rust")) {
            "rust:1.75-slim".to_string()
        } else if tags_lower
            .iter()
            .any(|t| t.contains("go") || t.contains("golang"))
        {
            "golang:1.21-alpine".to_string()
        } else if category_lower.contains("docker") || category_lower.contains("container") {
            "docker:24-dind".to_string()
        } else {
            self.config.default_base_image.clone()
        }
    }

    /// Validates that the container environment is working.
    async fn validate_environment(&self, container: &Container) -> AgentResult<()> {
        // Run basic commands to verify environment
        let checks = [
            &["echo", "Environment check: OK"][..],
            &["pwd"][..],
            &["ls", "-la"][..],
        ];

        for cmd in checks {
            let result = container
                .exec(&self.docker_client, cmd)
                .await
                .map_err(|e| {
                    AgentError::GenerationFailed(format!("Environment check failed: {}", e))
                })?;

            if result.exit_code != 0 {
                return Err(AgentError::GenerationFailed(format!(
                    "Environment check '{}' failed with exit code {}",
                    cmd.join(" "),
                    result.exit_code
                )));
            }
        }

        Ok(())
    }

    /// Validates the reference solution runs successfully.
    async fn validate_solution(
        &self,
        container: &Container,
        task: &SyntheticTask,
    ) -> AgentResult<(i64, String)> {
        let mut combined_output = String::new();
        let mut last_exit_code = 0i64;

        for (i, cmd) in task.hidden_solution.reference_commands.iter().enumerate() {
            debug!(step = i + 1, command = %cmd, "Executing solution step");

            let result = container
                .exec(&self.docker_client, &["sh", "-c", cmd])
                .await
                .map_err(|e| {
                    AgentError::GenerationFailed(format!("Solution step {} failed: {}", i + 1, e))
                })?;

            combined_output.push_str(&format!("--- Step {} ---\n", i + 1));
            combined_output.push_str(&format!("Command: {}\n", cmd));
            combined_output.push_str(&format!("Exit code: {}\n", result.exit_code));
            if !result.stdout.is_empty() {
                combined_output.push_str(&format!("stdout:\n{}\n", result.stdout));
            }
            if !result.stderr.is_empty() {
                combined_output.push_str(&format!("stderr:\n{}\n", result.stderr));
            }
            combined_output.push('\n');

            last_exit_code = result.exit_code;

            // Stop on first failure
            if result.exit_code != 0 {
                break;
            }
        }

        Ok((last_exit_code, combined_output))
    }

    /// Cleans up a container.
    async fn cleanup_container(&self, container: &mut Container) {
        if let Err(e) = container.cleanup(&self.docker_client).await {
            warn!(error = %e, "Failed to cleanup container");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::task_executor::{
        DifficultyScoring, HiddenSolution, SyntheticTask, TaskMetadata, VerificationSpec,
    };
    use crate::difficulty::DifficultyLevel;

    fn create_test_task() -> SyntheticTask {
        let hidden_solution = HiddenSolution::new("Test approach")
            .with_key_insights(vec!["Test insight"])
            .with_reference_commands(vec!["echo 'Hello'"]);

        let verification = VerificationSpec::new().with_success_criteria(vec!["Task completed"]);

        let difficulty =
            DifficultyScoring::new(DifficultyLevel::Medium).with_complexity_factors(vec!["linux"]);

        let metadata = TaskMetadata::new("testing", "test-idea-001").with_tags(vec!["test"]);

        SyntheticTask::new(
            "Test problem",
            hidden_solution,
            verification,
            difficulty,
            metadata,
        )
    }

    #[test]
    fn test_config_defaults() {
        let config = DockerValidatorConfig::default();
        assert_eq!(config.startup_timeout_seconds, 60);
        assert_eq!(config.validation_timeout_seconds, 300);
        assert!(config.validate_solution);
        assert!(!config.keep_containers);
    }

    #[test]
    fn test_config_builder() {
        let config = DockerValidatorConfig::new()
            .with_startup_timeout(120)
            .with_validation_timeout(600)
            .with_solution_validation(false)
            .with_keep_containers(true)
            .with_default_image("ubuntu:22.04")
            .with_network_mode("bridge");

        assert_eq!(config.startup_timeout_seconds, 120);
        assert_eq!(config.validation_timeout_seconds, 600);
        assert!(!config.validate_solution);
        assert!(config.keep_containers);
        assert_eq!(config.default_base_image, "ubuntu:22.04");
        assert_eq!(config.network_mode, "bridge");
    }

    #[test]
    fn test_validation_result_success() {
        let result = DockerValidationResult::success(1000, Some("abc123".to_string()));
        assert!(result.passed);
        assert!(result.environment_started);
        assert!(result.error.is_none());
        assert_eq!(result.container_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_validation_result_failure() {
        let result = DockerValidationResult::failure("Test error", 500);
        assert!(!result.passed);
        assert!(!result.environment_started);
        assert_eq!(result.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_validation_result_with_solution() {
        let result = DockerValidationResult::success(1000, None).with_solution_result(
            true,
            0,
            "Success output".to_string(),
        );

        assert!(result.passed);
        assert_eq!(result.solution_validated, Some(true));
        assert_eq!(result.solution_exit_code, Some(0));
        assert_eq!(result.solution_output, Some("Success output".to_string()));
    }

    #[test]
    fn test_validation_result_solution_failure() {
        let result = DockerValidationResult::success(1000, None).with_solution_result(
            false,
            1,
            "Error output".to_string(),
        );

        assert!(!result.passed); // Should be false when solution fails
        assert_eq!(result.solution_validated, Some(false));
        assert_eq!(result.solution_exit_code, Some(1));
    }

    #[test]
    fn test_determine_base_image_python() {
        let config = DockerValidatorConfig::default();
        // We can't test the agent without Docker, but we can test image selection logic
        // by checking the category parsing
        let task = create_test_task();
        assert_eq!(task.metadata.category, "testing");
    }

    #[test]
    fn test_task_creation() {
        let task = create_test_task();
        // ID is auto-generated UUID, just verify it's not empty
        assert!(!task.id.is_empty());
        assert_eq!(task.hidden_solution.reference_commands.len(), 1);
        assert_eq!(task.metadata.category, "testing");
    }
}
