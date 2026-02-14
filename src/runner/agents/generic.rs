//! Generic agent adapter.
//!
//! Runs any command-line agent that accepts a prompt via stdin or argument.

use std::process::Stdio;
use std::time::Instant;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, info};

use super::{AgentAdapter, AgentConfig, AgentError, AgentOutput, AgentType};

/// Generic adapter that works with any CLI-based agent.
pub struct GenericAdapter {
    /// Base command to run.
    command: String,
    /// Arguments template.
    args: Vec<String>,
    /// Whether to pass prompt via stdin (true) or as argument (false).
    use_stdin: bool,
}

impl GenericAdapter {
    /// Creates a new generic adapter with the given command.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            use_stdin: true,
        }
    }

    /// Creates with custom arguments.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Sets whether to use stdin for the prompt.
    pub fn with_stdin(mut self, use_stdin: bool) -> Self {
        self.use_stdin = use_stdin;
        self
    }
}

#[async_trait]
impl AgentAdapter for GenericAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Generic
    }

    async fn run(&self, config: &AgentConfig) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();

        // Determine the command to use
        let command = config
            .custom_command
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(&self.command);

        let mut cmd = Command::new(command);
        cmd.current_dir(&config.working_dir);

        // Add configured arguments
        for arg in &self.args {
            cmd.arg(arg);
        }

        // Add environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Add API key if specified
        if let Some(ref api_key) = config.api_key {
            cmd.env("OPENROUTER_API_KEY", api_key);
            cmd.env("ANTHROPIC_API_KEY", api_key);
            cmd.env("OPENAI_API_KEY", api_key);
        }

        if self.use_stdin {
            cmd.stdin(Stdio::piped());
        } else {
            // Pass prompt as argument
            cmd.arg("--prompt").arg(&config.prompt);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        info!("Starting generic agent: {} in {}", command, config.working_dir.display());

        let mut child = cmd.spawn().map_err(|e| {
            AgentError::ExecutionFailed(format!("Failed to spawn {}: {}", command, e))
        })?;

        // Write prompt to stdin if configured
        if self.use_stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(config.prompt.as_bytes())
                    .await
                    .map_err(|e| AgentError::ExecutionFailed(format!("Failed to write prompt: {}", e)))?;
                stdin.shutdown().await.ok();
            }
        }

        // Wait with timeout
        let timeout_result = tokio::time::timeout(config.timeout, child.wait_with_output()).await;

        let duration = start.elapsed();

        match timeout_result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                debug!("Agent completed with exit code {}", exit_code);

                Ok(AgentOutput::new(exit_code, stdout, stderr, duration))
            }
            Ok(Err(e)) => Err(AgentError::ExecutionFailed(format!("Process error: {}", e))),
            Err(_) => {
                // Timeout - process was already consumed by wait_with_output
                // so we can't kill it, just return the timeout error
                Err(AgentError::Timeout(config.timeout))
            }
        }
    }

    async fn is_available(&self) -> bool {
        Command::new(&self.command)
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn version(&self) -> Option<String> {
        let output = Command::new(&self.command)
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
    fn test_generic_adapter() {
        let adapter = GenericAdapter::new("echo");
        assert_eq!(adapter.agent_type(), AgentType::Generic);
    }

    #[test]
    fn test_with_args() {
        let adapter = GenericAdapter::new("test")
            .with_args(vec!["--flag".into(), "value".into()]);
        assert_eq!(adapter.args.len(), 2);
    }
}
