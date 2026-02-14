//! BaseAgent adapter.
//!
//! Integrates with the BaseAgent Python-based autonomous coding agent.

use std::process::Stdio;
use std::time::Instant;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::{AgentAdapter, AgentConfig, AgentError, AgentOutput, AgentType};

/// Adapter for BaseAgent.
pub struct BaseAgentAdapter {
    /// Path to the baseagent executable or module.
    command: String,
}

impl BaseAgentAdapter {
    /// Creates a new BaseAgent adapter.
    pub fn new() -> Self {
        Self {
            command: "python".to_string(),
        }
    }

    /// Creates with a custom command path.
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }
}

impl Default for BaseAgentAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for BaseAgentAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::BaseAgent
    }

    async fn run(&self, config: &AgentConfig) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();

        // Build the command
        let mut cmd = Command::new(&self.command);
        cmd.arg("-m")
            .arg("baseagent")
            .arg("--instruction")
            .arg(&config.prompt)
            .current_dir(&config.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add model if specified
        if let Some(ref model) = config.model {
            cmd.arg("--model").arg(model);
        }

        // Add environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Add API key if specified
        if let Some(ref api_key) = config.api_key {
            cmd.env("OPENROUTER_API_KEY", api_key);
            cmd.env("LITELLM_API_KEY", api_key);
        }

        info!(
            "Starting BaseAgent in {}",
            config.working_dir.display()
        );

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            AgentError::ExecutionFailed(format!("Failed to spawn baseagent: {}", e))
        })?;

        let stdout = child.stdout.take().expect("stdout not captured");
        let stderr = child.stderr.take().expect("stderr not captured");

        // Read output streams
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let mut stdout_lines = stdout_reader.lines();
        let mut stderr_lines = stderr_reader.lines();

        let mut stdout_content = String::new();
        let mut stderr_content = String::new();

        // Read with timeout
        let timeout = tokio::time::timeout(config.timeout, async {
            loop {
                tokio::select! {
                    line = stdout_lines.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                debug!("[baseagent stdout] {}", l);
                                stdout_content.push_str(&l);
                                stdout_content.push('\n');
                            }
                            Ok(None) => break,
                            Err(e) => {
                                warn!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    line = stderr_lines.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                debug!("[baseagent stderr] {}", l);
                                stderr_content.push_str(&l);
                                stderr_content.push('\n');
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!("Error reading stderr: {}", e);
                            }
                        }
                    }
                }
            }

            child.wait().await
        });

        let exit_status = match timeout.await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                return Err(AgentError::ExecutionFailed(format!(
                    "Process error: {}",
                    e
                )));
            }
            Err(_) => {
                // Timeout - kill the process
                let _ = child.kill().await;
                return Err(AgentError::Timeout(config.timeout));
            }
        };

        let duration = start.elapsed();
        let exit_code = exit_status.code().unwrap_or(-1);

        info!(
            "BaseAgent completed in {:?} with exit code {}",
            duration, exit_code
        );

        Ok(AgentOutput::new(
            exit_code,
            stdout_content,
            stderr_content,
            duration,
        ))
    }

    async fn is_available(&self) -> bool {
        Command::new(&self.command)
            .arg("-m")
            .arg("baseagent")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn version(&self) -> Option<String> {
        let output = Command::new(&self.command)
            .arg("-m")
            .arg("baseagent")
            .arg("--version")
            .output()
            .await
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation() {
        let adapter = BaseAgentAdapter::new();
        assert_eq!(adapter.agent_type(), AgentType::BaseAgent);
    }

    #[test]
    fn test_custom_command() {
        let adapter = BaseAgentAdapter::with_command("/usr/bin/python3");
        assert_eq!(adapter.command, "/usr/bin/python3");
    }
}
