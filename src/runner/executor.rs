//! Agent executor - the main runner logic.

use std::fs;
use std::path::Path;
use std::time::Instant;

use tracing::{debug, error, info};
use uuid::Uuid;

use super::agents::{create_adapter, AgentConfig, AgentError};
use super::config::RunConfig;
use super::result::RunResult;
use super::sandbox::{Sandbox, SandboxConfig, SandboxError};

/// The main agent runner.
pub struct AgentRunner {
    /// Configuration for this run.
    config: RunConfig,
}

impl AgentRunner {
    /// Creates a new agent runner with the given configuration.
    pub fn new(config: RunConfig) -> Self {
        Self { config }
    }

    /// Runs the agent against the configured task.
    pub async fn run(&self) -> Result<RunResult, RunnerError> {
        let run_id = format!("run-{}", Uuid::new_v4());
        let start = Instant::now();

        info!(
            "Starting run {} with agent {} on task {}",
            run_id,
            self.config.agent_type,
            self.config.task_path.display()
        );

        // Load the task prompt
        let prompt = self.load_prompt()?;
        debug!("Loaded prompt ({} chars)", prompt.len());

        // Extract task ID from task.yaml if available
        let task_id = self.extract_task_id().unwrap_or_else(|| {
            self.config
                .task_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

        // Create output directory for this run
        let run_output_dir = self.config.output_dir.join(&run_id);
        fs::create_dir_all(&run_output_dir).map_err(|e| {
            RunnerError::Setup(format!("Failed to create output directory: {}", e))
        })?;

        // Copy task files to output directory
        copy_task_files(&self.config.task_path, &run_output_dir)?;

        let result = if self.config.use_docker {
            self.run_in_docker(&run_id, &task_id, &prompt, &run_output_dir)
                .await
        } else {
            self.run_locally(&run_id, &task_id, &prompt, &run_output_dir)
                .await
        };

        let duration = start.elapsed();

        match result {
            Ok(mut run_result) => {
                // Collect files created by the agent
                run_result.files_created = list_created_files(&run_output_dir, &self.config.task_path);
                run_result.duration = duration;

                // Save the run result
                self.save_result(&run_result, &run_output_dir)?;

                info!(
                    "Run {} completed in {:?} with status {}",
                    run_id, duration, run_result.status
                );

                Ok(run_result)
            }
            Err(e) => {
                error!("Run {} failed: {}", run_id, e);
                
                let failed_result = RunResult::failure(
                    run_id,
                    task_id,
                    self.config.agent_type,
                    duration,
                    e.to_string(),
                );

                // Try to save even failed results
                let _ = self.save_result(&failed_result, &run_output_dir);

                Ok(failed_result)
            }
        }
    }

    /// Runs the agent locally (no Docker).
    async fn run_locally(
        &self,
        run_id: &str,
        task_id: &str,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<RunResult, RunnerError> {
        let adapter = create_adapter(self.config.agent_type);

        // Check if agent is available
        if !adapter.is_available().await {
            return Err(RunnerError::AgentNotFound(format!(
                "{} is not available",
                self.config.agent_type
            )));
        }

        let agent_config = AgentConfig {
            prompt: prompt.to_string(),
            working_dir: output_dir.to_path_buf(),
            timeout: self.config.timeout,
            env_vars: self.config.env_vars.clone(),
            model: self.config.model.clone(),
            api_key: self.config.api_key.clone(),
            custom_command: None,
        };

        let start = Instant::now();
        let output = adapter.run(&agent_config).await.map_err(|e| match e {
            AgentError::Timeout(d) => RunnerError::Timeout(d),
            AgentError::NotFound(msg) => RunnerError::AgentNotFound(msg),
            other => RunnerError::Execution(other.to_string()),
        })?;

        let duration = start.elapsed();

        let mut result = if output.is_success() {
            RunResult::success(run_id, task_id, self.config.agent_type, duration, output_dir.to_path_buf())
        } else {
            RunResult::failure(
                run_id,
                task_id,
                self.config.agent_type,
                duration,
                format!("Agent exited with code {}", output.exit_code),
            )
            .with_exit_code(output.exit_code)
        };

        result = result
            .with_stdout(output.stdout)
            .with_stderr(output.stderr);

        if let Some(trace) = output.trace {
            result = result.with_trace(trace);
        }

        Ok(result)
    }

    /// Runs the agent in a Docker sandbox.
    async fn run_in_docker(
        &self,
        run_id: &str,
        task_id: &str,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<RunResult, RunnerError> {
        let image = self
            .config
            .docker_image
            .clone()
            .unwrap_or_else(|| "python:3.11-slim".to_string());

        let sandbox_config = SandboxConfig::new(&image)
            .with_memory_mb(self.config.memory_limit_mb)
            .with_cpu_limit(self.config.cpu_limit)
            .with_timeout(self.config.timeout);

        let mut sandbox = Sandbox::new(sandbox_config, output_dir);

        // Setup sandbox
        sandbox
            .setup(&self.config.task_path)
            .await
            .map_err(|e| RunnerError::Sandbox(e))?;

        // Build the command to run inside Docker
        let _adapter = create_adapter(self.config.agent_type);
        let command = match self.config.agent_type.default_command() {
            Some(cmd) => {
                let parts: Vec<String> = cmd.split_whitespace().map(String::from).collect();
                parts
            }
            None => vec!["bash".to_string(), "-c".to_string(), "cat".to_string()],
        };

        // Write prompt to a file in the sandbox
        let prompt_file = output_dir.join(".agent_prompt.txt");
        fs::write(&prompt_file, prompt).map_err(|e| {
            RunnerError::Setup(format!("Failed to write prompt file: {}", e))
        })?;

        let start = Instant::now();

        // Run Docker container
        let docker_args = sandbox.docker_run_args(&command);
        debug!("Docker command: docker {}", docker_args.join(" "));

        let docker_output = tokio::time::timeout(
            self.config.timeout,
            tokio::process::Command::new("docker")
                .args(&docker_args)
                .output(),
        )
        .await;

        // Cleanup sandbox
        sandbox.cleanup().await.ok();

        let duration = start.elapsed();

        match docker_output {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let mut result = if output.status.success() {
                    RunResult::success(run_id, task_id, self.config.agent_type, duration, output_dir.to_path_buf())
                } else {
                    RunResult::failure(
                        run_id,
                        task_id,
                        self.config.agent_type,
                        duration,
                        format!("Container exited with code {}", exit_code),
                    )
                    .with_exit_code(exit_code)
                };

                result = result.with_stdout(stdout).with_stderr(stderr);

                Ok(result)
            }
            Ok(Err(e)) => Err(RunnerError::Execution(format!("Docker error: {}", e))),
            Err(_) => {
                // Timeout - try to kill the container
                let _ = tokio::process::Command::new("docker")
                    .args(["kill", &sandbox.id])
                    .output()
                    .await;

                Err(RunnerError::Timeout(self.config.timeout))
            }
        }
    }

    /// Loads the task prompt from prompt.md.
    fn load_prompt(&self) -> Result<String, RunnerError> {
        let prompt_path = self.config.prompt_path();
        fs::read_to_string(&prompt_path).map_err(|e| {
            RunnerError::Setup(format!(
                "Failed to read prompt from {}: {}",
                prompt_path.display(),
                e
            ))
        })
    }

    /// Extracts the task ID from task.yaml.
    fn extract_task_id(&self) -> Option<String> {
        let task_yaml_path = self.config.task_yaml_path();
        let content = fs::read_to_string(&task_yaml_path).ok()?;
        
        // Simple extraction - look for "id:" line
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("id:") {
                return Some(line[3..].trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }
        None
    }

    /// Saves the run result to disk.
    fn save_result(&self, result: &RunResult, output_dir: &Path) -> Result<(), RunnerError> {
        let result_path = output_dir.join("run_result.json");
        let json = serde_json::to_string_pretty(result).map_err(|e| {
            RunnerError::Setup(format!("Failed to serialize result: {}", e))
        })?;
        fs::write(&result_path, json).map_err(|e| {
            RunnerError::Setup(format!("Failed to write result: {}", e))
        })?;
        debug!("Saved result to {}", result_path.display());
        Ok(())
    }
}

/// Copies task files to the output directory.
fn copy_task_files(task_dir: &Path, output_dir: &Path) -> Result<(), RunnerError> {
    // Copy prompt.md
    let prompt_src = task_dir.join("prompt.md");
    if prompt_src.exists() {
        fs::copy(&prompt_src, output_dir.join("prompt.md")).map_err(|e| {
            RunnerError::Setup(format!("Failed to copy prompt.md: {}", e))
        })?;
    }

    // Don't copy task.yaml (hidden from agent) but keep solution.sh reference
    Ok(())
}

/// Lists files created by the agent (not in original task).
fn list_created_files(output_dir: &Path, task_dir: &Path) -> Vec<String> {
    let mut created = Vec::new();

    fn collect_files(dir: &Path, base: &Path, files: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        files.push(rel.to_string_lossy().to_string());
                    }
                } else if path.is_dir() {
                    collect_files(&path, base, files);
                }
            }
        }
    }

    collect_files(output_dir, output_dir, &mut created);

    // Filter out files that were in the original task
    let task_files: std::collections::HashSet<_> = {
        let mut files = Vec::new();
        collect_files(task_dir, task_dir, &mut files);
        files.into_iter().collect()
    };

    created
        .into_iter()
        .filter(|f| !task_files.contains(f) && !f.starts_with('.'))
        .collect()
}

/// Error types for the runner.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("Setup error: {0}")]
    Setup(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Sandbox error: {0}")]
    Sandbox(#[from] SandboxError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_task(dir: &Path) {
        fs::write(
            dir.join("prompt.md"),
            "# Test Task\n\nCreate a file called output.txt with 'hello'",
        )
        .unwrap();
        fs::write(
            dir.join("task.yaml"),
            "id: test-task-001\ndifficulty: easy",
        )
        .unwrap();
    }

    #[test]
    fn test_runner_creation() {
        let config = RunConfig::new("./test-task");
        let runner = AgentRunner::new(config);
        assert!(runner.config.use_docker);
    }

    #[test]
    fn test_extract_task_id() {
        let temp = TempDir::new().unwrap();
        create_test_task(temp.path());

        let config = RunConfig::new(temp.path());
        let runner = AgentRunner::new(config);

        let task_id = runner.extract_task_id();
        assert_eq!(task_id, Some("test-task-001".to_string()));
    }

    #[test]
    fn test_load_prompt() {
        let temp = TempDir::new().unwrap();
        create_test_task(temp.path());

        let config = RunConfig::new(temp.path());
        let runner = AgentRunner::new(config);

        let prompt = runner.load_prompt().unwrap();
        assert!(prompt.contains("Test Task"));
    }

    #[test]
    fn test_list_created_files() {
        let task_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        // Original task file
        fs::write(task_dir.path().join("prompt.md"), "test").unwrap();

        // Copy to output and add new file
        fs::write(output_dir.path().join("prompt.md"), "test").unwrap();
        fs::write(output_dir.path().join("output.txt"), "hello").unwrap();
        fs::write(output_dir.path().join("result.json"), "{}").unwrap();

        let created = list_created_files(output_dir.path(), task_dir.path());

        assert!(created.contains(&"output.txt".to_string()));
        assert!(created.contains(&"result.json".to_string()));
        assert!(!created.contains(&"prompt.md".to_string())); // Was in original
    }
}
