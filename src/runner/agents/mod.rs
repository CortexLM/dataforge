//! Agent adapters for different AI coding agents.
//!
//! Each adapter knows how to:
//! 1. Launch the agent with a prompt
//! 2. Capture its output
//! 3. Handle its specific protocols (if any)

pub mod baseagent;
pub mod generic;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::result::ExecutionTrace;

/// Supported agent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// BaseAgent (Python-based autonomous agent).
    BaseAgent,
    /// Claude Code (Anthropic's coding agent).
    ClaudeCode,
    /// Aider (AI pair programming tool).
    Aider,
    /// Generic agent via stdin/stdout.
    Generic,
    /// Custom agent with specified command.
    Custom,
}

impl AgentType {
    /// Returns the display name for this agent type.
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentType::BaseAgent => "BaseAgent",
            AgentType::ClaudeCode => "Claude Code",
            AgentType::Aider => "Aider",
            AgentType::Generic => "Generic",
            AgentType::Custom => "Custom",
        }
    }

    /// Returns the default command for this agent type.
    pub fn default_command(&self) -> Option<&'static str> {
        match self {
            AgentType::BaseAgent => Some("python -m baseagent"),
            AgentType::ClaudeCode => Some("claude"),
            AgentType::Aider => Some("aider"),
            AgentType::Generic => None,
            AgentType::Custom => None,
        }
    }
}

impl Default for AgentType {
    fn default() -> Self {
        Self::Generic
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for AgentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "baseagent" | "base-agent" | "base" => Ok(AgentType::BaseAgent),
            "claude" | "claude-code" | "claudecode" => Ok(AgentType::ClaudeCode),
            "aider" => Ok(AgentType::Aider),
            "generic" => Ok(AgentType::Generic),
            "custom" => Ok(AgentType::Custom),
            other => Err(format!("Unknown agent type: {}", other)),
        }
    }
}

/// Result of running an agent.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// Exit code from the agent process.
    pub exit_code: i32,
    /// Standard output captured.
    pub stdout: String,
    /// Standard error captured.
    pub stderr: String,
    /// Files created or modified by the agent.
    pub files_changed: Vec<String>,
    /// Execution duration.
    pub duration: Duration,
    /// Detailed execution trace (if captured).
    pub trace: Option<ExecutionTrace>,
}

impl AgentOutput {
    /// Creates a new agent output.
    pub fn new(exit_code: i32, stdout: String, stderr: String, duration: Duration) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
            files_changed: Vec::new(),
            duration,
            trace: None,
        }
    }

    /// Checks if the agent completed successfully (exit code 0).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Adds a file to the list of changed files.
    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.files_changed.push(path.into());
        self
    }

    /// Adds an execution trace.
    pub fn with_trace(mut self, trace: ExecutionTrace) -> Self {
        self.trace = Some(trace);
        self
    }
}

/// Configuration passed to an agent adapter.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// The prompt/instruction for the agent.
    pub prompt: String,
    /// Working directory for the agent.
    pub working_dir: std::path::PathBuf,
    /// Timeout for execution.
    pub timeout: Duration,
    /// Environment variables.
    pub env_vars: Vec<(String, String)>,
    /// Model to use (if applicable).
    pub model: Option<String>,
    /// API key (if applicable).
    pub api_key: Option<String>,
    /// Custom command (for Custom agent type).
    pub custom_command: Option<String>,
}

/// Trait for agent adapters.
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Returns the agent type.
    fn agent_type(&self) -> AgentType;

    /// Runs the agent with the given configuration.
    async fn run(&self, config: &AgentConfig) -> Result<AgentOutput, AgentError>;

    /// Checks if this agent is available on the system.
    async fn is_available(&self) -> bool;

    /// Returns the version of the agent (if available).
    async fn version(&self) -> Option<String>;
}

/// Error type for agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    NotFound(String),

    #[error("Agent execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Agent timed out after {0:?}")]
    Timeout(Duration),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub use baseagent::BaseAgentAdapter;
pub use generic::GenericAdapter;

/// Creates an adapter for the given agent type.
pub fn create_adapter(agent_type: AgentType) -> Box<dyn AgentAdapter> {
    match agent_type {
        AgentType::BaseAgent => Box::new(BaseAgentAdapter::new()),
        AgentType::ClaudeCode => Box::new(GenericAdapter::new("claude")),
        AgentType::Aider => Box::new(GenericAdapter::new("aider")),
        AgentType::Generic => Box::new(GenericAdapter::new("agent")),
        AgentType::Custom => Box::new(GenericAdapter::new("custom")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display() {
        assert_eq!(AgentType::BaseAgent.display_name(), "BaseAgent");
        assert_eq!(AgentType::Generic.display_name(), "Generic");
    }

    #[test]
    fn test_agent_type_from_str() {
        assert_eq!("baseagent".parse::<AgentType>().unwrap(), AgentType::BaseAgent);
        assert_eq!("claude-code".parse::<AgentType>().unwrap(), AgentType::ClaudeCode);
        assert_eq!("generic".parse::<AgentType>().unwrap(), AgentType::Generic);
        assert!("unknown".parse::<AgentType>().is_err());
    }

    #[test]
    fn test_agent_output() {
        let output = AgentOutput::new(0, "stdout".into(), "stderr".into(), Duration::from_secs(10));
        assert!(output.is_success());

        let failed = AgentOutput::new(1, "".into(), "error".into(), Duration::from_secs(5));
        assert!(!failed.is_success());
    }
}
