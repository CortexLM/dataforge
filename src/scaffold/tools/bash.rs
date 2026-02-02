//! Bash tool for executing shell commands in a container.
//!
//! This tool allows the agent to execute arbitrary shell commands within
//! the Docker container environment.

use async_trait::async_trait;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

use super::{ExecutionContext, Tool, ToolError, ToolResult};

/// Maximum output length to prevent memory issues.
const MAX_OUTPUT_LENGTH: usize = 100_000;

/// Default timeout for command execution in seconds.
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// Parameters for the bash tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BashParams {
    /// The shell command to execute.
    command: String,
    /// Optional timeout in seconds (defaults to 30).
    #[serde(default)]
    timeout_seconds: Option<u32>,
}

/// Tool for executing shell commands in a container.
pub struct BashTool {
    /// Docker client for container interaction.
    docker: Option<Docker>,
}

impl BashTool {
    /// Create a new BashTool instance.
    pub fn new() -> Self {
        // Try to connect to Docker daemon
        let docker = Docker::connect_with_local_defaults().ok();
        Self { docker }
    }

    /// Create a BashTool with a specific Docker client.
    pub fn with_docker(docker: Docker) -> Self {
        Self {
            docker: Some(docker),
        }
    }

    /// Execute a command in the container.
    async fn exec_in_container(
        &self,
        container_id: &str,
        command: &str,
        working_dir: &str,
        timeout_seconds: u64,
    ) -> Result<ToolResult, ToolError> {
        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| ToolError::DockerError("Docker client not available".to_string()))?;

        // Create exec instance
        let exec_config = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["sh", "-c", command]),
            working_dir: Some(working_dir),
            ..Default::default()
        };

        let exec = docker
            .create_exec(container_id, exec_config)
            .await
            .map_err(|e| ToolError::DockerError(format!("Failed to create exec: {}", e)))?;

        // Start exec with timeout
        let exec_future = docker.start_exec(&exec.id, None);

        let result = timeout(Duration::from_secs(timeout_seconds), exec_future)
            .await
            .map_err(|_| ToolError::Timeout {
                seconds: timeout_seconds,
            })?
            .map_err(|e| ToolError::DockerError(format!("Failed to start exec: {}", e)))?;

        // Collect output
        let mut stdout = String::new();
        let mut stderr = String::new();

        match result {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(bollard::container::LogOutput::StdOut { message }) => {
                            let text = String::from_utf8_lossy(&message);
                            if stdout.len() + text.len() <= MAX_OUTPUT_LENGTH {
                                stdout.push_str(&text);
                            }
                        }
                        Ok(bollard::container::LogOutput::StdErr { message }) => {
                            let text = String::from_utf8_lossy(&message);
                            if stderr.len() + text.len() <= MAX_OUTPUT_LENGTH {
                                stderr.push_str(&text);
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            return Err(ToolError::ExecutionFailed(format!(
                                "Error reading output: {}",
                                e
                            )));
                        }
                    }
                }
            }
            StartExecResults::Detached => {
                return Err(ToolError::ExecutionFailed(
                    "Unexpected detached execution".to_string(),
                ));
            }
        }

        // Get exit code
        let inspect = docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| ToolError::DockerError(format!("Failed to inspect exec: {}", e)))?;

        let exit_code = inspect.exit_code.unwrap_or(-1);

        // Format output
        let mut output = stdout;
        if !stderr.is_empty() {
            if !output.is_empty() {
                output.push_str("\n--- stderr ---\n");
            }
            output.push_str(&stderr);
        }

        if exit_code == 0 {
            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult::partial(
                output,
                format!("Command exited with code {}", exit_code),
            ))
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the container. Use for running programs, installing packages, checking status, etc."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_seconds": {
                    "type": "integer",
                    "description": "Maximum execution time in seconds (default: 30)",
                    "default": 30,
                    "minimum": 1,
                    "maximum": 300
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError> {
        // Parse parameters
        let params: BashParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid parameters: {}", e)))?;

        // Validate command is not empty
        if params.command.trim().is_empty() {
            return Err(ToolError::InvalidParameters(
                "Command cannot be empty".to_string(),
            ));
        }

        // Get timeout (use provided, context default, or tool default)
        let timeout_seconds = params
            .timeout_seconds
            .map(|t| t as u64)
            .unwrap_or(ctx.default_timeout.max(DEFAULT_TIMEOUT_SECONDS));

        // Execute command
        self.exec_in_container(
            &ctx.container_id,
            &params.command,
            &ctx.working_dir,
            timeout_seconds,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_tool_name() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
    }

    #[test]
    fn test_bash_tool_description() {
        let tool = BashTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_bash_tool_parameters_schema() {
        let tool = BashTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["properties"]["timeout_seconds"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&Value::String("command".to_string())));
    }

    #[test]
    fn test_bash_params_deserialization() {
        let json = serde_json::json!({
            "command": "ls -la",
            "timeout_seconds": 60
        });
        let params: BashParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.command, "ls -la");
        assert_eq!(params.timeout_seconds, Some(60));
    }

    #[test]
    fn test_bash_params_default_timeout() {
        let json = serde_json::json!({
            "command": "echo hello"
        });
        let params: BashParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.command, "echo hello");
        assert!(params.timeout_seconds.is_none());
    }

    #[tokio::test]
    async fn test_bash_tool_empty_command() {
        let tool = BashTool::new();
        let ctx = ExecutionContext::new("test-container", "/workspace");
        let args = serde_json::json!({ "command": "   " });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }

    #[tokio::test]
    async fn test_bash_tool_no_docker() {
        // BashTool without Docker should fail gracefully
        let tool = BashTool { docker: None };
        let ctx = ExecutionContext::new("test-container", "/workspace");
        let args = serde_json::json!({ "command": "echo hello" });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::DockerError(_))));
    }
}
