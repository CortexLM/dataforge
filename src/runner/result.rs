//! Results and execution traces for agent runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::AgentType;

/// Complete result of running an agent against a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// Unique identifier for this run.
    pub run_id: String,
    /// Task identifier.
    pub task_id: String,
    /// Agent type used.
    pub agent_type: AgentType,
    /// Status of the run.
    pub status: RunStatus,
    /// Total duration of the run.
    pub duration: Duration,
    /// Timestamp when the run started.
    pub started_at: DateTime<Utc>,
    /// Timestamp when the run completed.
    pub completed_at: DateTime<Utc>,
    /// Exit code from the agent process.
    pub exit_code: i32,
    /// Path to the output directory.
    pub output_dir: PathBuf,
    /// Files created by the agent.
    pub files_created: Vec<String>,
    /// Files modified by the agent.
    pub files_modified: Vec<String>,
    /// Execution trace (if captured).
    pub trace: Option<ExecutionTrace>,
    /// Any error message if the run failed.
    pub error: Option<String>,
    /// Captured stdout (truncated if too long).
    pub stdout_summary: String,
    /// Captured stderr (truncated if too long).
    pub stderr_summary: String,
}

impl RunResult {
    /// Creates a new successful run result.
    pub fn success(
        run_id: impl Into<String>,
        task_id: impl Into<String>,
        agent_type: AgentType,
        duration: Duration,
        output_dir: PathBuf,
    ) -> Self {
        let now = Utc::now();
        Self {
            run_id: run_id.into(),
            task_id: task_id.into(),
            agent_type,
            status: RunStatus::Completed,
            duration,
            started_at: now - chrono::Duration::from_std(duration).unwrap_or_default(),
            completed_at: now,
            exit_code: 0,
            output_dir,
            files_created: Vec::new(),
            files_modified: Vec::new(),
            trace: None,
            error: None,
            stdout_summary: String::new(),
            stderr_summary: String::new(),
        }
    }

    /// Creates a failed run result.
    pub fn failure(
        run_id: impl Into<String>,
        task_id: impl Into<String>,
        agent_type: AgentType,
        duration: Duration,
        error: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            run_id: run_id.into(),
            task_id: task_id.into(),
            agent_type,
            status: RunStatus::Failed,
            duration,
            started_at: now - chrono::Duration::from_std(duration).unwrap_or_default(),
            completed_at: now,
            exit_code: -1,
            output_dir: PathBuf::new(),
            files_created: Vec::new(),
            files_modified: Vec::new(),
            trace: None,
            error: Some(error.into()),
            stdout_summary: String::new(),
            stderr_summary: String::new(),
        }
    }

    /// Sets the exit code.
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Sets the files created.
    pub fn with_files_created(mut self, files: Vec<String>) -> Self {
        self.files_created = files;
        self
    }

    /// Sets the execution trace.
    pub fn with_trace(mut self, trace: ExecutionTrace) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Sets stdout summary.
    pub fn with_stdout(mut self, stdout: impl Into<String>) -> Self {
        self.stdout_summary = truncate_string(stdout.into(), 10000);
        self
    }

    /// Sets stderr summary.
    pub fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
        self.stderr_summary = truncate_string(stderr.into(), 10000);
        self
    }

    /// Returns true if the run completed successfully.
    pub fn is_success(&self) -> bool {
        self.status == RunStatus::Completed && self.exit_code == 0
    }
}

/// Status of an agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run is pending (not started yet).
    Pending,
    /// Run is currently executing.
    Running,
    /// Run completed (successfully or not - check exit_code).
    Completed,
    /// Run failed to start or crashed.
    Failed,
    /// Run was terminated due to timeout.
    Timeout,
    /// Run was cancelled by user.
    Cancelled,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Pending => write!(f, "pending"),
            RunStatus::Running => write!(f, "running"),
            RunStatus::Completed => write!(f, "completed"),
            RunStatus::Failed => write!(f, "failed"),
            RunStatus::Timeout => write!(f, "timeout"),
            RunStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Detailed execution trace capturing agent actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Individual steps taken by the agent.
    pub steps: Vec<TraceStep>,
    /// Total token usage (if available).
    pub token_usage: Option<TokenUsage>,
    /// Model used (if available).
    pub model: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ExecutionTrace {
    /// Creates a new empty trace.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            token_usage: None,
            model: None,
            metadata: HashMap::new(),
        }
    }

    /// Adds a step to the trace.
    pub fn add_step(&mut self, step: TraceStep) {
        self.steps.push(step);
    }

    /// Sets the token usage.
    pub fn with_token_usage(mut self, usage: TokenUsage) -> Self {
        self.token_usage = Some(usage);
        self
    }

    /// Sets the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Returns the total number of steps.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Returns the total duration across all steps.
    pub fn total_duration(&self) -> Duration {
        self.steps.iter().map(|s| s.duration).sum()
    }
}

impl Default for ExecutionTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// A single step in the execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    /// Step number (1-indexed).
    pub step_number: u32,
    /// Timestamp of this step.
    pub timestamp: DateTime<Utc>,
    /// Duration of this step.
    pub duration: Duration,
    /// Type of action taken.
    pub action_type: String,
    /// Details of the action.
    pub action_details: String,
    /// Result of the action.
    pub result: String,
    /// Whether this step was successful.
    pub success: bool,
}

impl TraceStep {
    /// Creates a new trace step.
    pub fn new(
        step_number: u32,
        action_type: impl Into<String>,
        action_details: impl Into<String>,
        result: impl Into<String>,
        duration: Duration,
        success: bool,
    ) -> Self {
        Self {
            step_number,
            timestamp: Utc::now(),
            duration,
            action_type: action_type.into(),
            action_details: action_details.into(),
            result: result.into(),
            success,
        }
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input/prompt tokens.
    pub input_tokens: u64,
    /// Output/completion tokens.
    pub output_tokens: u64,
    /// Cached tokens (if applicable).
    pub cached_tokens: u64,
}

impl TokenUsage {
    /// Creates new token usage stats.
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            cached_tokens: 0,
        }
    }

    /// Returns total tokens used.
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Truncates a string to a maximum length.
fn truncate_string(s: String, max_len: usize) -> String {
    if s.len() <= max_len {
        s
    } else {
        format!("{}... [truncated]", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_result_success() {
        let result = RunResult::success(
            "run-001",
            "task-001",
            AgentType::BaseAgent,
            Duration::from_secs(60),
            PathBuf::from("./output"),
        );
        assert!(result.is_success());
        assert_eq!(result.status, RunStatus::Completed);
    }

    #[test]
    fn test_run_result_failure() {
        let result = RunResult::failure(
            "run-002",
            "task-001",
            AgentType::Generic,
            Duration::from_secs(30),
            "Agent crashed",
        );
        assert!(!result.is_success());
        assert_eq!(result.status, RunStatus::Failed);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_execution_trace() {
        let mut trace = ExecutionTrace::new();
        trace.add_step(TraceStep::new(
            1,
            "command",
            "ls -la",
            "file1.txt\nfile2.txt",
            Duration::from_millis(100),
            true,
        ));
        trace.add_step(TraceStep::new(
            2,
            "write",
            "output.txt",
            "File written",
            Duration::from_millis(50),
            true,
        ));

        assert_eq!(trace.step_count(), 2);
        assert_eq!(trace.total_duration(), Duration::from_millis(150));
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::new(1000, 500);
        assert_eq!(usage.total(), 1500);
    }

    #[test]
    fn test_run_status_display() {
        assert_eq!(RunStatus::Completed.to_string(), "completed");
        assert_eq!(RunStatus::Timeout.to_string(), "timeout");
    }
}
