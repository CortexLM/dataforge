//! SWE-Agent subprocess integration for external scaffold support.
//!
//! This module provides integration with SWE-Agent, a Python-based scaffold
//! designed specifically for software engineering tasks. It manages the
//! subprocess lifecycle, communication, and output parsing.

use super::bridge::{BridgeError, ProcessBridge};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Errors specific to SWE-Agent operations.
#[derive(Debug, Clone)]
pub enum SweAgentError {
    /// SWE-Agent is not installed or not found at the specified path.
    NotInstalled(String),
    /// Failed to start the SWE-Agent subprocess.
    StartupFailed(String),
    /// Communication error with the subprocess.
    BridgeError(BridgeError),
    /// Failed to parse SWE-Agent output.
    ParseError(String),
    /// SWE-Agent process exited unexpectedly.
    ProcessExited(i32),
    /// Configuration error.
    ConfigError(String),
    /// Operation timed out.
    Timeout,
    /// SWE-Agent is not initialized.
    NotInitialized,
    /// Invalid state transition.
    InvalidState(String),
}

impl fmt::Display for SweAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SweAgentError::NotInstalled(msg) => write!(f, "SWE-Agent not installed: {}", msg),
            SweAgentError::StartupFailed(msg) => {
                write!(f, "SWE-Agent startup failed: {}", msg)
            }
            SweAgentError::BridgeError(e) => write!(f, "Bridge error: {}", e),
            SweAgentError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            SweAgentError::ProcessExited(code) => {
                write!(f, "SWE-Agent process exited with code {}", code)
            }
            SweAgentError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            SweAgentError::Timeout => write!(f, "SWE-Agent operation timed out"),
            SweAgentError::NotInitialized => write!(f, "SWE-Agent not initialized"),
            SweAgentError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
        }
    }
}

impl std::error::Error for SweAgentError {}

impl From<BridgeError> for SweAgentError {
    fn from(err: BridgeError) -> Self {
        match err {
            BridgeError::Timeout => SweAgentError::Timeout,
            BridgeError::ProcessExited(code) => SweAgentError::ProcessExited(code),
            other => SweAgentError::BridgeError(other),
        }
    }
}

/// Configuration for SWE-Agent scaffold.
#[derive(Debug, Clone)]
pub struct SweAgentConfig {
    /// Path to the Python interpreter.
    pub python_path: PathBuf,
    /// Path to the SWE-Agent installation directory.
    pub swe_agent_path: PathBuf,
    /// Model to use for SWE-Agent's internal LLM calls (if any).
    pub model: String,
    /// Timeout for individual operations.
    pub timeout: Duration,
    /// Maximum number of steps before terminating.
    pub max_steps: u32,
    /// Whether to use verbose output.
    pub verbose: bool,
}

impl Default for SweAgentConfig {
    fn default() -> Self {
        Self {
            python_path: PathBuf::from("python3"),
            swe_agent_path: PathBuf::from("sweagent"),
            model: "gpt-4".to_string(),
            timeout: Duration::from_secs(300),
            max_steps: 50,
            verbose: false,
        }
    }
}

impl SweAgentConfig {
    /// Create a new SWE-Agent configuration.
    pub fn new(python_path: PathBuf, swe_agent_path: PathBuf) -> Self {
        Self {
            python_path,
            swe_agent_path,
            ..Default::default()
        }
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the timeout duration.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of steps.
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Enable or disable verbose output.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

/// SWE-Agent action parsed from ACR (Action/Thought) format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweAgentAction {
    /// The reasoning/thought process.
    pub thought: String,
    /// The action to take (tool name).
    pub action: String,
    /// Arguments for the action.
    pub action_args: Option<String>,
}

impl SweAgentAction {
    /// Check if this action is a submit/completion action.
    pub fn is_submit(&self) -> bool {
        self.action.to_lowercase() == "submit"
    }

    /// Check if this action is a bash command.
    pub fn is_bash(&self) -> bool {
        self.action.to_lowercase() == "bash"
    }

    /// Get the full command string if this is a bash action.
    pub fn bash_command(&self) -> Option<&str> {
        if self.is_bash() {
            self.action_args.as_deref()
        } else {
            None
        }
    }
}

/// State of the SWE-Agent scaffold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweAgentState {
    /// Not yet initialized.
    Uninitialized,
    /// Running and ready for commands.
    Running,
    /// Finished (submitted solution).
    Finished,
    /// Error state.
    Error,
}

/// Task specification for SWE-Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Unique identifier for the task.
    pub task_id: String,
    /// Problem statement/description.
    pub problem_statement: String,
    /// Repository or codebase path.
    pub repo_path: Option<String>,
    /// Specific hints or constraints.
    pub hints: Vec<String>,
}

/// Result of a single step in SWE-Agent execution.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// The action taken by SWE-Agent.
    pub action: SweAgentAction,
    /// The observation/result of the action.
    pub observation: String,
    /// Whether this step terminated the session.
    pub is_terminal: bool,
    /// Whether the action was successful.
    pub success: bool,
}

/// Container information for SWE-Agent.
#[derive(Debug, Clone)]
pub struct Container {
    /// Container ID.
    pub id: String,
    /// Working directory inside the container.
    pub working_dir: String,
}

/// SWE-Agent scaffold for external integration.
///
/// This scaffold manages a SWE-Agent subprocess and provides methods for:
/// - Initializing the agent with a task
/// - Stepping through actions
/// - Parsing SWE-Agent's ACR output format
/// - Cleaning up resources
pub struct SweAgentScaffold {
    config: SweAgentConfig,
    bridge: Option<ProcessBridge>,
    container_id: String,
    state: SweAgentState,
    step_count: u32,
}

impl SweAgentScaffold {
    /// Create a new SWE-Agent scaffold with the given configuration.
    pub fn new(config: SweAgentConfig) -> Self {
        Self {
            config,
            bridge: None,
            container_id: String::new(),
            state: SweAgentState::Uninitialized,
            step_count: 0,
        }
    }

    /// Generate a YAML configuration file content for SWE-Agent.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification
    ///
    /// # Returns
    ///
    /// YAML configuration string for SWE-Agent.
    pub fn generate_config(&self, task: &TaskSpec) -> String {
        let hints_yaml = if task.hints.is_empty() {
            "[]".to_string()
        } else {
            task.hints
                .iter()
                .map(|h| format!("  - \"{}\"", h.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            r#"# Generated config for SWE-Agent
environment:
  container_id: "{container_id}"

model:
  name: "{model}"

task:
  task_id: "{task_id}"
  problem_statement: |
    {problem_statement}
  hints:
{hints}

settings:
  max_steps: {max_steps}
  verbose: {verbose}
  timeout: {timeout}

commands:
  - name: open
    signature: "open <path> [<line_number>]"
    description: "Open a file and display its contents with line numbers"
  - name: goto
    signature: "goto <line_number>"
    description: "Go to a specific line in the current file"
  - name: scroll_down
    signature: "scroll_down"
    description: "Scroll down in the current file"
  - name: scroll_up
    signature: "scroll_up"
    description: "Scroll up in the current file"
  - name: search_file
    signature: "search_file <search_term> [<file>]"
    description: "Search for a term in a file"
  - name: search_dir
    signature: "search_dir <search_term> [<dir>]"
    description: "Search for a term in a directory"
  - name: find_file
    signature: "find_file <file_name> [<dir>]"
    description: "Find files by name"
  - name: edit
    signature: "edit <start_line>:<end_line>\n<replacement_text>\nend_of_edit"
    description: "Edit lines in the current file"
  - name: create
    signature: "create <file_path>"
    description: "Create a new file"
  - name: submit
    signature: "submit"
    description: "Submit the solution"
  - name: bash
    signature: "bash <command>"
    description: "Execute a bash command"
"#,
            container_id = self.container_id,
            model = self.config.model,
            task_id = task.task_id,
            problem_statement = task.problem_statement.replace('\n', "\n    "),
            hints = hints_yaml,
            max_steps = self.config.max_steps,
            verbose = self.config.verbose,
            timeout = self.config.timeout.as_secs(),
        )
    }

    /// Parse SWE-Agent's ACR (Action/Thought) output format.
    ///
    /// SWE-Agent outputs in a format like:
    /// ```text
    /// THOUGHT: <reasoning>
    /// ACTION: <action_name>
    /// <action_arguments>
    /// ```
    ///
    /// # Arguments
    ///
    /// * `output` - Raw output from SWE-Agent
    ///
    /// # Returns
    ///
    /// Parsed `SweAgentAction` or a parse error.
    pub fn parse_output(&self, output: &str) -> Result<SweAgentAction, SweAgentError> {
        let output = output.trim();

        // Look for THOUGHT: and ACTION: markers
        let thought = self.extract_section(output, "THOUGHT:", &["ACTION:", "OBSERVATION:"])?;
        let action_line = self.extract_section(output, "ACTION:", &["OBSERVATION:", "THOUGHT:"])?;

        // Parse action and arguments
        let (action, action_args) = self.parse_action_line(&action_line)?;

        Ok(SweAgentAction {
            thought,
            action,
            action_args,
        })
    }

    /// Extract a section from the output between a start marker and any of the end markers.
    fn extract_section(
        &self,
        output: &str,
        start_marker: &str,
        end_markers: &[&str],
    ) -> Result<String, SweAgentError> {
        let start_idx = output
            .find(start_marker)
            .ok_or_else(|| SweAgentError::ParseError(format!("Missing {} marker", start_marker)))?;

        let content_start = start_idx + start_marker.len();
        let remaining = &output[content_start..];

        // Find the earliest end marker
        let end_idx = end_markers
            .iter()
            .filter_map(|marker| remaining.find(marker))
            .min()
            .unwrap_or(remaining.len());

        Ok(remaining[..end_idx].trim().to_string())
    }

    /// Parse an action line into action name and arguments.
    fn parse_action_line(&self, line: &str) -> Result<(String, Option<String>), SweAgentError> {
        let line = line.trim();

        // Handle multi-line actions (like edit)
        let lines: Vec<&str> = line.lines().collect();
        if lines.is_empty() {
            return Err(SweAgentError::ParseError("Empty action line".to_string()));
        }

        let first_line = lines[0].trim();

        // Split on first space to get action name
        if let Some(space_idx) = first_line.find(' ') {
            let action = first_line[..space_idx].to_string();
            let mut args = first_line[space_idx + 1..].to_string();

            // Append remaining lines if any (for multi-line actions)
            if lines.len() > 1 {
                args.push('\n');
                args.push_str(&lines[1..].join("\n"));
            }

            Ok((action, Some(args)))
        } else {
            // No arguments
            Ok((first_line.to_string(), None))
        }
    }

    /// Start the SWE-Agent subprocess.
    ///
    /// # Arguments
    ///
    /// * `container_id` - The Docker container ID to operate in
    ///
    /// # Returns
    ///
    /// `Ok(())` if startup succeeded, or an error.
    pub async fn start(&mut self, container_id: &str) -> Result<(), SweAgentError> {
        if self.state != SweAgentState::Uninitialized {
            return Err(SweAgentError::InvalidState(format!(
                "Cannot start from state {:?}",
                self.state
            )));
        }

        self.container_id = container_id.to_string();

        // Check if SWE-Agent is available
        let python_path = self.config.python_path.to_string_lossy().to_string();
        let swe_agent_path = self.config.swe_agent_path.to_string_lossy().to_string();

        // Build the command to run SWE-Agent
        let args: Vec<&str> = vec![
            "-u", // Unbuffered output
            "-m",
            "sweagent.run",
            "--container-id",
            container_id,
            "--interactive",
        ];

        let env_vars = [("PYTHONUNBUFFERED", "1"), ("CONTAINER_ID", container_id)];

        let bridge = ProcessBridge::spawn(&python_path, &args, &env_vars, self.config.timeout)
            .await
            .map_err(|e| {
                if matches!(e, BridgeError::SpawnFailed(_)) {
                    SweAgentError::NotInstalled(format!(
                        "Could not start SWE-Agent at {}: {}",
                        swe_agent_path, e
                    ))
                } else {
                    SweAgentError::StartupFailed(e.to_string())
                }
            })?;

        self.bridge = Some(bridge);
        self.state = SweAgentState::Running;

        Ok(())
    }

    /// Initialize SWE-Agent with a task specification.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to solve
    ///
    /// # Returns
    ///
    /// Initial observation from SWE-Agent, or an error.
    pub async fn initialize_task(&mut self, task: &TaskSpec) -> Result<String, SweAgentError> {
        let bridge = self.bridge.as_mut().ok_or(SweAgentError::NotInitialized)?;

        // Send task initialization
        let init_message = format!(
            "TASK_INIT\n{}\n{}\nEND_TASK_INIT\n",
            task.task_id, task.problem_statement
        );

        bridge.send(&init_message).await?;

        // Wait for initialization response
        let lines = bridge.receive_until("READY").await?;
        Ok(lines.join("\n"))
    }

    /// Send an observation to SWE-Agent and get the next action.
    ///
    /// # Arguments
    ///
    /// * `observation` - The observation from the previous action
    ///
    /// # Returns
    ///
    /// The next action from SWE-Agent, or an error.
    pub async fn step(&mut self, observation: &str) -> Result<SweAgentAction, SweAgentError> {
        if self.state != SweAgentState::Running {
            return Err(SweAgentError::InvalidState(format!(
                "Cannot step from state {:?}",
                self.state
            )));
        }

        if self.step_count >= self.config.max_steps {
            self.state = SweAgentState::Finished;
            return Err(SweAgentError::InvalidState(
                "Maximum steps exceeded".to_string(),
            ));
        }

        let bridge = self.bridge.as_mut().ok_or(SweAgentError::NotInitialized)?;

        // Send observation
        let obs_message = format!("OBSERVATION:\n{}\nEND_OBSERVATION\n", observation);
        bridge.send(&obs_message).await?;

        // Receive action response
        let lines = bridge.receive_until("END_ACTION").await?;
        let response = lines.join("\n");

        let action = self.parse_output(&response)?;

        self.step_count += 1;

        // Check for terminal action
        if action.is_submit() {
            self.state = SweAgentState::Finished;
        }

        Ok(action)
    }

    /// Check if SWE-Agent has finished (submitted solution or error).
    pub fn is_finished(&self) -> bool {
        matches!(self.state, SweAgentState::Finished | SweAgentState::Error)
    }

    /// Get the current state of the scaffold.
    pub fn state(&self) -> SweAgentState {
        self.state
    }

    /// Get the number of steps taken.
    pub fn step_count(&self) -> u32 {
        self.step_count
    }

    /// Clean up the SWE-Agent subprocess.
    ///
    /// # Returns
    ///
    /// `Ok(())` if cleanup succeeded, or an error.
    pub async fn cleanup(&mut self) -> Result<(), SweAgentError> {
        if let Some(bridge) = self.bridge.take() {
            bridge.close().await?;
        }
        self.state = SweAgentState::Uninitialized;
        self.step_count = 0;
        Ok(())
    }
}

/// Scaffold trait for polymorphic scaffold usage.
///
/// This trait allows different scaffolds (SWE-Agent, OpenHands, custom)
/// to be used interchangeably.
#[async_trait]
pub trait Scaffold: Send + Sync {
    /// Initialize the scaffold with a task and container.
    async fn scaffold_initialize(
        &mut self,
        task: &TaskSpec,
        container: &Container,
    ) -> Result<String, ScaffoldError>;

    /// Execute a single step, returning the action result.
    async fn scaffold_step(&mut self, llm_response: &str) -> Result<StepResult, ScaffoldError>;

    /// Check if the scaffold has reached a terminal state.
    fn is_terminal(&self) -> bool;

    /// Clean up scaffold resources.
    async fn scaffold_cleanup(&mut self) -> Result<(), ScaffoldError>;
}

/// Errors from scaffold operations.
#[derive(Debug, Clone)]
pub enum ScaffoldError {
    /// Initialization failed.
    InitializationFailed(String),
    /// Step execution failed.
    StepFailed(String),
    /// Cleanup failed.
    CleanupFailed(String),
    /// Not initialized.
    NotInitialized,
    /// SWE-Agent specific error.
    SweAgentError(String),
}

impl fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaffoldError::InitializationFailed(msg) => {
                write!(f, "Initialization failed: {}", msg)
            }
            ScaffoldError::StepFailed(msg) => write!(f, "Step failed: {}", msg),
            ScaffoldError::CleanupFailed(msg) => write!(f, "Cleanup failed: {}", msg),
            ScaffoldError::NotInitialized => write!(f, "Scaffold not initialized"),
            ScaffoldError::SweAgentError(msg) => write!(f, "SWE-Agent error: {}", msg),
        }
    }
}

impl std::error::Error for ScaffoldError {}

impl From<SweAgentError> for ScaffoldError {
    fn from(err: SweAgentError) -> Self {
        ScaffoldError::SweAgentError(err.to_string())
    }
}

#[async_trait]
impl Scaffold for SweAgentScaffold {
    async fn scaffold_initialize(
        &mut self,
        task: &TaskSpec,
        container: &Container,
    ) -> Result<String, ScaffoldError> {
        self.start(&container.id)
            .await
            .map_err(|e| ScaffoldError::InitializationFailed(e.to_string()))?;

        let observation = self
            .initialize_task(task)
            .await
            .map_err(|e| ScaffoldError::InitializationFailed(e.to_string()))?;

        Ok(observation)
    }

    async fn scaffold_step(&mut self, observation: &str) -> Result<StepResult, ScaffoldError> {
        let action = self
            .step(observation)
            .await
            .map_err(|e| ScaffoldError::StepFailed(e.to_string()))?;

        let is_terminal = action.is_submit();

        Ok(StepResult {
            action,
            observation: String::new(), // Will be filled by caller after execution
            is_terminal,
            success: true,
        })
    }

    fn is_terminal(&self) -> bool {
        self.is_finished()
    }

    async fn scaffold_cleanup(&mut self) -> Result<(), ScaffoldError> {
        self.cleanup()
            .await
            .map_err(|e| ScaffoldError::CleanupFailed(e.to_string()))
    }
}

/// Check if SWE-Agent is installed and available.
///
/// # Arguments
///
/// * `python_path` - Path to Python interpreter
///
/// # Returns
///
/// `true` if SWE-Agent is available, `false` otherwise.
pub async fn is_swe_agent_available(python_path: &str) -> bool {
    use tokio::process::Command;

    let result = Command::new(python_path)
        .args(["-c", "import sweagent; print(sweagent.__version__)"])
        .output()
        .await;

    matches!(result, Ok(output) if output.status.success())
}

/// Get the SWE-Agent version if installed.
///
/// # Arguments
///
/// * `python_path` - Path to Python interpreter
///
/// # Returns
///
/// The version string, or `None` if not installed.
pub async fn get_swe_agent_version(python_path: &str) -> Option<String> {
    use tokio::process::Command;

    let output = Command::new(python_path)
        .args(["-c", "import sweagent; print(sweagent.__version__)"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        Some(version.trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swe_agent_config_default() {
        let config = SweAgentConfig::default();
        assert_eq!(config.python_path, PathBuf::from("python3"));
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_steps, 50);
        assert!(!config.verbose);
    }

    #[test]
    fn test_swe_agent_config_builder() {
        let config = SweAgentConfig::new(
            PathBuf::from("/usr/bin/python3"),
            PathBuf::from("/opt/sweagent"),
        )
        .with_model("claude-3")
        .with_timeout(Duration::from_secs(600))
        .with_max_steps(100)
        .with_verbose(true);

        assert_eq!(config.python_path, PathBuf::from("/usr/bin/python3"));
        assert_eq!(config.swe_agent_path, PathBuf::from("/opt/sweagent"));
        assert_eq!(config.model, "claude-3");
        assert_eq!(config.timeout, Duration::from_secs(600));
        assert_eq!(config.max_steps, 100);
        assert!(config.verbose);
    }

    #[test]
    fn test_swe_agent_action_is_submit() {
        let submit_action = SweAgentAction {
            thought: "Task complete".to_string(),
            action: "submit".to_string(),
            action_args: None,
        };
        assert!(submit_action.is_submit());

        let bash_action = SweAgentAction {
            thought: "Running command".to_string(),
            action: "bash".to_string(),
            action_args: Some("ls -la".to_string()),
        };
        assert!(!bash_action.is_submit());
    }

    #[test]
    fn test_swe_agent_action_is_bash() {
        let bash_action = SweAgentAction {
            thought: "List files".to_string(),
            action: "bash".to_string(),
            action_args: Some("ls -la".to_string()),
        };
        assert!(bash_action.is_bash());
        assert_eq!(bash_action.bash_command(), Some("ls -la"));

        let open_action = SweAgentAction {
            thought: "Open file".to_string(),
            action: "open".to_string(),
            action_args: Some("test.py".to_string()),
        };
        assert!(!open_action.is_bash());
        assert_eq!(open_action.bash_command(), None);
    }

    #[test]
    fn test_parse_output_basic() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        let output = "THOUGHT: I need to list the files\nACTION: bash ls -la";

        let result = scaffold.parse_output(output);
        assert!(result.is_ok());

        let action = result.unwrap();
        assert_eq!(action.thought, "I need to list the files");
        assert_eq!(action.action, "bash");
        assert_eq!(action.action_args, Some("ls -la".to_string()));
    }

    #[test]
    fn test_parse_output_multiline_thought() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        let output = r#"THOUGHT: First I need to understand the codebase.
Let me start by listing the files.
ACTION: bash ls -la"#;

        let result = scaffold.parse_output(output);
        assert!(result.is_ok());

        let action = result.unwrap();
        assert!(action.thought.contains("understand the codebase"));
        assert!(action.thought.contains("listing the files"));
    }

    #[test]
    fn test_parse_output_submit() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        let output = "THOUGHT: The fix is complete\nACTION: submit";

        let result = scaffold.parse_output(output);
        assert!(result.is_ok());

        let action = result.unwrap();
        assert!(action.is_submit());
        assert!(action.action_args.is_none());
    }

    #[test]
    fn test_parse_output_missing_thought() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        let output = "ACTION: bash ls";

        let result = scaffold.parse_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_output_missing_action() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        let output = "THOUGHT: I need to do something";

        let result = scaffold.parse_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_config() {
        let mut scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        scaffold.container_id = "test-container-123".to_string();

        let task = TaskSpec {
            task_id: "task-001".to_string(),
            problem_statement: "Fix the bug in main.py".to_string(),
            repo_path: Some("/workspace".to_string()),
            hints: vec!["Look at line 42".to_string()],
        };

        let config = scaffold.generate_config(&task);

        assert!(config.contains("container_id: \"test-container-123\""));
        assert!(config.contains("task_id: \"task-001\""));
        assert!(config.contains("Fix the bug in main.py"));
        assert!(config.contains("Look at line 42"));
        assert!(config.contains("submit"));
    }

    #[test]
    fn test_scaffold_initial_state() {
        let scaffold = SweAgentScaffold::new(SweAgentConfig::default());
        assert_eq!(scaffold.state(), SweAgentState::Uninitialized);
        assert!(!scaffold.is_finished());
        assert_eq!(scaffold.step_count(), 0);
    }

    #[test]
    fn test_swe_agent_error_display() {
        let error = SweAgentError::NotInstalled("path/not/found".to_string());
        assert!(error.to_string().contains("not installed"));

        let error = SweAgentError::Timeout;
        assert!(error.to_string().contains("timed out"));

        let error = SweAgentError::ParseError("invalid format".to_string());
        assert!(error.to_string().contains("Parse error"));
    }

    #[test]
    fn test_scaffold_error_display() {
        let error = ScaffoldError::InitializationFailed("startup error".to_string());
        assert!(error.to_string().contains("Initialization failed"));

        let error = ScaffoldError::NotInitialized;
        assert!(error.to_string().contains("not initialized"));
    }

    #[test]
    fn test_step_result() {
        let action = SweAgentAction {
            thought: "test".to_string(),
            action: "bash".to_string(),
            action_args: Some("echo hello".to_string()),
        };

        let result = StepResult {
            action: action.clone(),
            observation: "hello".to_string(),
            is_terminal: false,
            success: true,
        };

        assert!(!result.is_terminal);
        assert!(result.success);
        assert_eq!(result.observation, "hello");
    }

    #[test]
    fn test_task_spec_serialization() {
        let task = TaskSpec {
            task_id: "test-123".to_string(),
            problem_statement: "Fix the issue".to_string(),
            repo_path: Some("/repo".to_string()),
            hints: vec!["hint1".to_string(), "hint2".to_string()],
        };

        let json = serde_json::to_string(&task).unwrap();
        let parsed: TaskSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.task_id, "test-123");
        assert_eq!(parsed.hints.len(), 2);
    }

    #[test]
    fn test_container() {
        let container = Container {
            id: "abc123".to_string(),
            working_dir: "/workspace".to_string(),
        };

        assert_eq!(container.id, "abc123");
        assert_eq!(container.working_dir, "/workspace");
    }
}
