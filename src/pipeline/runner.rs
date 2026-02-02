//! Task runner for executing tasks and collecting trajectories.
//!
//! This module provides the `TaskRunner` which combines Docker execution
//! with scaffold-based agent loops to execute tasks and collect trajectories.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::execution::docker_client::ContainerConfig;
use crate::execution::{Container, DockerClient, ExecutionLimits};
use crate::scaffold::{Scaffold, ScaffoldError, TaskSpec as ScaffoldTaskSpec};
use crate::trajectory::{
    AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, Trajectory,
    TrajectoryCollector,
};

/// Errors that can occur during task execution.
#[derive(Debug, Error)]
pub enum RunError {
    /// Docker-related error.
    #[error("Docker error: {0}")]
    Docker(#[from] crate::error::DockerError),

    /// Scaffold-related error.
    #[error("Scaffold error: {0}")]
    Scaffold(#[from] ScaffoldError),

    /// Task execution timed out.
    #[error("Task timed out after {0:?}")]
    Timeout(Duration),

    /// Task exceeded maximum allowed steps.
    #[error("Task exceeded maximum steps: {max_steps}")]
    MaxStepsExceeded { max_steps: usize },

    /// Container execution failed.
    #[error("Container execution failed: {0}")]
    ExecutionFailed(String),

    /// Invalid task specification.
    #[error("Invalid task specification: {0}")]
    InvalidTask(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Specification for a task to be executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Unique identifier for the task.
    pub id: String,
    /// Category of the task (e.g., "file_manipulation", "code_generation").
    pub category: String,
    /// Difficulty level (e.g., "easy", "medium", "hard").
    pub difficulty: String,
    /// The instruction/problem statement for the agent.
    pub instruction: String,
    /// Optional verification script to check task completion.
    pub verification_script: Option<String>,
    /// Optional expected output for validation.
    pub expected_output: Option<String>,
    /// Timeout for this specific task.
    pub timeout: Duration,
    /// Maximum steps allowed for this task.
    pub max_steps: usize,
}

impl TaskSpec {
    /// Creates a new task specification.
    pub fn new(id: impl Into<String>, instruction: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            category: "general".to_string(),
            difficulty: "medium".to_string(),
            instruction: instruction.into(),
            verification_script: None,
            expected_output: None,
            timeout: Duration::from_secs(1800),
            max_steps: 50,
        }
    }

    /// Sets the category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Sets the difficulty.
    pub fn with_difficulty(mut self, difficulty: impl Into<String>) -> Self {
        self.difficulty = difficulty.into();
        self
    }

    /// Sets the verification script.
    pub fn with_verification_script(mut self, script: impl Into<String>) -> Self {
        self.verification_script = Some(script.into());
        self
    }

    /// Sets the expected output.
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output = Some(output.into());
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum steps.
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Converts this task spec to the scaffold's task spec format.
    pub fn to_scaffold_task(&self) -> ScaffoldTaskSpec {
        ScaffoldTaskSpec {
            task_id: self.id.clone(),
            problem_statement: self.instruction.clone(),
            repo_path: None,
            hints: Vec::new(),
        }
    }
}

/// Result of running a task.
#[derive(Debug)]
pub struct RunResult {
    /// The collected trajectory from the execution.
    pub trajectory: Trajectory,
    /// Container logs from the execution.
    pub container_logs: String,
    /// Exit code of the container (if it exited).
    pub exit_code: Option<i32>,
}

/// Task runner that executes tasks in Docker containers with scaffolds.
///
/// The runner handles:
/// - Creating and managing Docker containers
/// - Initializing and running the scaffold (agent loop)
/// - Collecting trajectories during execution
/// - Cleaning up resources on success and failure
pub struct TaskRunner {
    docker_client: Arc<DockerClient>,
    scaffold: Box<dyn Scaffold>,
    trajectory_collector: TrajectoryCollector,
    container: Option<Container>,
}

impl TaskRunner {
    /// Creates a new task runner.
    ///
    /// # Arguments
    ///
    /// * `docker_client` - Docker client for container operations
    /// * `scaffold` - The scaffold implementation to use for the agent loop
    pub fn new(docker_client: Arc<DockerClient>, scaffold: Box<dyn Scaffold>) -> Self {
        Self {
            docker_client,
            scaffold,
            trajectory_collector: TrajectoryCollector::new("", "", ""),
            container: None,
        }
    }

    /// Executes a task and collects the trajectory.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification
    /// * `model` - The LLM model name for tracking
    ///
    /// # Returns
    ///
    /// `RunResult` containing the trajectory and execution details, or an error.
    ///
    /// # Errors
    ///
    /// Returns `RunError` if:
    /// - Container creation fails
    /// - Scaffold initialization fails
    /// - Task times out
    /// - Task exceeds maximum steps
    pub async fn run(&mut self, task: &TaskSpec, model: &str) -> Result<RunResult, RunError> {
        let start_time = Instant::now();

        // Initialize trajectory collector
        self.trajectory_collector = TrajectoryCollector::new(&task.id, model, "swe-agent");

        // Create container configuration based on task difficulty
        let container_config = self.create_container_config(task);

        // Create and start the container
        let mut container = Container::new(&self.docker_client, container_config).await?;
        container.start(&self.docker_client).await?;

        // Store container reference for cleanup
        self.container = Some(container);

        // Run the task with timeout handling
        let result = self.execute_with_timeout(task, start_time).await;

        // Always clean up the container
        let container_logs = self.cleanup_container().await;

        // Finalize and return based on result
        match result {
            Ok((trajectory_result, exit_code, total_tokens)) => {
                let elapsed = start_time.elapsed();
                let trajectory = self.trajectory_collector.finalize(
                    trajectory_result,
                    elapsed.as_secs(),
                    total_tokens,
                );

                Ok(RunResult {
                    trajectory,
                    container_logs,
                    exit_code,
                })
            }
            Err(e) => {
                // Finalize trajectory with error (trajectory is not returned on error)
                let elapsed = start_time.elapsed();
                let _trajectory = self.trajectory_collector.finalize(
                    TaskResult::Error {
                        message: e.to_string(),
                    },
                    elapsed.as_secs(),
                    TokenUsage::default(),
                );

                // Return error but include partial trajectory
                Err(match e {
                    RunError::Timeout(_) => e,
                    RunError::MaxStepsExceeded { .. } => e,
                    _ => RunError::ExecutionFailed(format!(
                        "Task failed after {} steps with {} seconds elapsed: {}",
                        self.trajectory_collector.current_step(),
                        elapsed.as_secs(),
                        e
                    )),
                })
            }
        }
    }

    /// Creates container configuration for the task.
    fn create_container_config(&self, task: &TaskSpec) -> ContainerConfig {
        let limits = get_limits_for_difficulty(&task.difficulty);

        ContainerConfig::new(
            format!(
                "task-{}-{}",
                task.id,
                Uuid::new_v4().to_string().split('-').next().unwrap_or("0")
            ),
            "python:3.11-slim",
        )
        .with_limits(limits)
        .with_working_dir("/workspace")
        .with_network_mode("none")
    }

    /// Executes the task with timeout handling.
    async fn execute_with_timeout(
        &mut self,
        task: &TaskSpec,
        start_time: Instant,
    ) -> Result<(TaskResult, Option<i32>, TokenUsage), RunError> {
        let container = self
            .container
            .as_ref()
            .ok_or_else(|| RunError::Internal("Container not initialized".to_string()))?;

        // Create scaffold container info
        let scaffold_container = crate::scaffold::Container {
            id: container.id().to_string(),
            working_dir: "/workspace".to_string(),
        };

        // Initialize the scaffold
        let scaffold_task = task.to_scaffold_task();
        let initial_observation = self
            .scaffold
            .scaffold_initialize(&scaffold_task, &scaffold_container)
            .await?;

        // Record initial state
        let initial_state = EnvironmentState {
            working_directory: "/workspace".to_string(),
            files_modified: Vec::new(),
            last_command_output: Some(initial_observation.clone()),
            context_summary: format!("Task: {}", task.instruction),
        };

        // Main agent loop
        let mut current_observation = initial_observation;
        let mut step_count = 0;
        let mut total_tokens = TokenUsage::default();
        let mut last_exit_code: Option<i32> = None;

        while !self.scaffold.is_terminal() {
            // Check timeout
            if start_time.elapsed() > task.timeout {
                return Err(RunError::Timeout(task.timeout));
            }

            // Check step limit
            if step_count >= task.max_steps {
                return Err(RunError::MaxStepsExceeded {
                    max_steps: task.max_steps,
                });
            }

            // Record current state
            let state = if step_count == 0 {
                initial_state.clone()
            } else {
                EnvironmentState {
                    working_directory: "/workspace".to_string(),
                    files_modified: self.get_modified_files(),
                    last_command_output: Some(current_observation.clone()),
                    context_summary: format!("Step {} of task: {}", step_count, task.instruction),
                }
            };

            // Get next action from scaffold
            let step_result = self.scaffold.scaffold_step(&current_observation).await?;

            // Record the action
            let action = AgentAction {
                tool_name: step_result.action.action.clone(),
                tool_args: serde_json::json!({
                    "args": step_result.action.action_args,
                }),
                raw_llm_output: step_result.action.thought.clone(),
                thinking: Some(step_result.action.thought.clone()),
            };

            // Execute the action in the container if it's a bash command
            let (observation, exit_code) = if step_result.action.is_bash() {
                self.execute_bash_command(step_result.action.bash_command().unwrap_or(""))
                    .await?
            } else if step_result.action.is_submit() {
                // Submit action - verify the task
                let verification_result = self.verify_task(task).await?;
                (verification_result, Some(0))
            } else {
                // Other actions - just record the observation
                (step_result.observation.clone(), None)
            };

            last_exit_code = exit_code;
            current_observation = observation.clone();

            // Create observation record
            let obs = Observation {
                success: exit_code.map(|c| c == 0).unwrap_or(true),
                output: observation,
                error: if exit_code.map(|c| c != 0).unwrap_or(false) {
                    Some("Command failed".to_string())
                } else {
                    None
                },
                state_changes: Vec::new(),
            };

            // Calculate reward for this step
            let reward = self.calculate_step_reward(&action, &obs, step_result.is_terminal);

            // Record the step
            self.trajectory_collector.record_step(
                state,
                action,
                obs,
                reward,
                step_result.is_terminal,
            );

            // Update token usage (estimated since we don't have actual counts)
            total_tokens.add(&TokenUsage::new(100, 50)); // Placeholder estimation

            step_count += 1;
        }

        // Verify final result
        let task_result = self.determine_task_result(task, &total_tokens).await?;

        Ok((task_result, last_exit_code, total_tokens))
    }

    /// Execute a bash command in the container.
    async fn execute_bash_command(&self, command: &str) -> Result<(String, Option<i32>), RunError> {
        let container = self
            .container
            .as_ref()
            .ok_or_else(|| RunError::Internal("Container not initialized".to_string()))?;

        let result = container
            .exec(&self.docker_client, &["bash", "-c", command])
            .await?;

        let output = if result.stderr.is_empty() {
            result.stdout
        } else {
            format!("{}\n{}", result.stdout, result.stderr)
        };

        Ok((output, Some(result.exit_code as i32)))
    }

    /// Verify the task completion.
    async fn verify_task(&self, task: &TaskSpec) -> Result<String, RunError> {
        if let Some(ref script) = task.verification_script {
            let (output, exit_code) = self.execute_bash_command(script).await?;

            if exit_code == Some(0) {
                Ok(format!("Verification passed: {}", output))
            } else {
                Ok(format!("Verification failed: {}", output))
            }
        } else {
            Ok("Task submitted (no verification script)".to_string())
        }
    }

    /// Determine the final task result.
    async fn determine_task_result(
        &self,
        task: &TaskSpec,
        _tokens: &TokenUsage,
    ) -> Result<TaskResult, RunError> {
        // If there's a verification script, run it
        if let Some(ref script) = task.verification_script {
            let (output, exit_code) = self.execute_bash_command(script).await?;

            if exit_code == Some(0) {
                // Check expected output if specified
                if let Some(ref expected) = task.expected_output {
                    if output.trim().contains(expected.trim()) {
                        return Ok(TaskResult::Success { score: 1.0 });
                    } else {
                        return Ok(TaskResult::Failure {
                            reason: format!(
                                "Output mismatch. Expected: '{}', Got: '{}'",
                                expected, output
                            ),
                        });
                    }
                }
                return Ok(TaskResult::Success { score: 1.0 });
            } else {
                return Ok(TaskResult::Failure {
                    reason: format!("Verification script failed: {}", output),
                });
            }
        }

        // No verification script - assume success if we got here
        Ok(TaskResult::Success { score: 0.8 })
    }

    /// Calculate reward for a step.
    fn calculate_step_reward(
        &self,
        action: &AgentAction,
        obs: &Observation,
        is_terminal: bool,
    ) -> f64 {
        let mut reward = 0.0;

        // Base reward for successful execution
        if obs.success {
            reward += 0.1;
        } else {
            reward -= 0.1;
        }

        // Bonus for terminal success
        if is_terminal && obs.success {
            reward += 0.5;
        }

        // Penalty for errors
        if obs.error.is_some() {
            reward -= 0.2;
        }

        // Small bonus for meaningful actions
        if !action.tool_name.is_empty() && action.tool_name != "submit" {
            reward += 0.05;
        }

        reward
    }

    /// Get list of modified files (placeholder).
    fn get_modified_files(&self) -> Vec<String> {
        // In a real implementation, this would track actual file modifications
        Vec::new()
    }

    /// Clean up the container and return logs.
    async fn cleanup_container(&mut self) -> String {
        if let Some(mut container) = self.container.take() {
            // Get logs before cleanup
            let logs = container
                .logs(&self.docker_client)
                .await
                .unwrap_or_else(|e| format!("Failed to get logs: {}", e));

            // Clean up the container
            if let Err(e) = container.cleanup(&self.docker_client).await {
                tracing::warn!(error = %e, "Failed to cleanup container");
            }

            logs
        } else {
            String::new()
        }
    }

    /// Get the current trajectory ID.
    pub fn trajectory_id(&self) -> Uuid {
        self.trajectory_collector.trajectory_id()
    }

    /// Get the current step count.
    pub fn current_step(&self) -> u32 {
        self.trajectory_collector.current_step()
    }
}

/// Get execution limits based on difficulty.
fn get_limits_for_difficulty(difficulty: &str) -> ExecutionLimits {
    match difficulty.to_lowercase().as_str() {
        "easy" | "simple" => ExecutionLimits::new(1024, 1.0, 5, 100, 600),
        "medium" | "moderate" => ExecutionLimits::new(2048, 2.0, 10, 200, 1800),
        "hard" | "difficult" | "expert" => ExecutionLimits::new(4096, 4.0, 20, 500, 3600),
        _ => ExecutionLimits::new(2048, 2.0, 10, 200, 1800),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_spec_new() {
        let task = TaskSpec::new("test-1", "Do something");
        assert_eq!(task.id, "test-1");
        assert_eq!(task.instruction, "Do something");
        assert_eq!(task.category, "general");
        assert_eq!(task.difficulty, "medium");
        assert_eq!(task.max_steps, 50);
    }

    #[test]
    fn test_task_spec_builder() {
        let task = TaskSpec::new("test-2", "Create a file")
            .with_category("file_manipulation")
            .with_difficulty("hard")
            .with_verification_script("test -f output.txt")
            .with_expected_output("success")
            .with_timeout(Duration::from_secs(3600))
            .with_max_steps(100);

        assert_eq!(task.id, "test-2");
        assert_eq!(task.category, "file_manipulation");
        assert_eq!(task.difficulty, "hard");
        assert_eq!(
            task.verification_script,
            Some("test -f output.txt".to_string())
        );
        assert_eq!(task.expected_output, Some("success".to_string()));
        assert_eq!(task.timeout, Duration::from_secs(3600));
        assert_eq!(task.max_steps, 100);
    }

    #[test]
    fn test_task_spec_to_scaffold_task() {
        let task = TaskSpec::new("task-123", "Fix the bug in main.py");
        let scaffold_task = task.to_scaffold_task();

        assert_eq!(scaffold_task.task_id, "task-123");
        assert_eq!(scaffold_task.problem_statement, "Fix the bug in main.py");
        assert!(scaffold_task.repo_path.is_none());
        assert!(scaffold_task.hints.is_empty());
    }

    #[test]
    fn test_limits_for_difficulty() {
        let easy = get_limits_for_difficulty("easy");
        assert_eq!(easy.memory_mb, 1024);
        assert!((easy.cpu_cores - 1.0).abs() < f64::EPSILON);

        let medium = get_limits_for_difficulty("medium");
        assert_eq!(medium.memory_mb, 2048);
        assert!((medium.cpu_cores - 2.0).abs() < f64::EPSILON);

        let hard = get_limits_for_difficulty("hard");
        assert_eq!(hard.memory_mb, 4096);
        assert!((hard.cpu_cores - 4.0).abs() < f64::EPSILON);

        // Unknown defaults to medium
        let unknown = get_limits_for_difficulty("unknown");
        assert_eq!(unknown.memory_mb, 2048);
    }

    #[test]
    fn test_run_error_display() {
        let err = RunError::Timeout(Duration::from_secs(300));
        assert!(err.to_string().contains("timed out"));

        let err = RunError::MaxStepsExceeded { max_steps: 50 };
        assert!(err.to_string().contains("50"));

        let err = RunError::ExecutionFailed("test failure".to_string());
        assert!(err.to_string().contains("test failure"));

        let err = RunError::InvalidTask("bad task".to_string());
        assert!(err.to_string().contains("bad task"));
    }

    #[test]
    fn test_task_spec_serialization() {
        let task = TaskSpec::new("ser-test", "Test serialization")
            .with_category("testing")
            .with_difficulty("easy");

        let json = serde_json::to_string(&task).expect("serialization should work");
        let parsed: TaskSpec = serde_json::from_str(&json).expect("deserialization should work");

        assert_eq!(parsed.id, "ser-test");
        assert_eq!(parsed.category, "testing");
        assert_eq!(parsed.difficulty, "easy");
    }
}
