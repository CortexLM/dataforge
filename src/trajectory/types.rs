//! Trajectory data types for agent execution recording.
//!
//! This module defines the SARSA-style trajectory format used to record
//! agent execution for training and evaluation purposes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete trajectory representing an agent's execution on a task.
///
/// Contains the full sequence of state-action-observation-reward steps,
/// along with metadata about the execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    /// Unique identifier for this trajectory.
    pub id: Uuid,

    /// Identifier of the task being executed.
    pub task_id: String,

    /// Model used for the agent (e.g., "gpt-4", "claude-3").
    pub model: String,

    /// Type of scaffold used (e.g., "react", "reflexion", "basic").
    pub scaffold_type: String,

    /// Sequence of steps in the trajectory.
    pub steps: Vec<TrajectoryStep>,

    /// Final result of the task execution.
    pub final_result: TaskResult,

    /// Total reward accumulated across all steps.
    pub total_reward: f64,

    /// When the trajectory was created.
    pub created_at: DateTime<Utc>,

    /// Total duration of the execution in seconds.
    pub duration_seconds: u64,

    /// Token usage statistics for the execution.
    pub token_usage: TokenUsage,
}

/// A single step in a trajectory (SARSA format).
///
/// Represents one iteration of the agent loop: observing state,
/// taking an action, receiving an observation, and getting a reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryStep {
    /// Sequential step number (0-indexed).
    pub step_number: u32,

    /// Environment state before the action was taken.
    pub state: EnvironmentState,

    /// Action taken by the agent.
    pub action: AgentAction,

    /// Observation received after taking the action.
    pub observation: Observation,

    /// Reward received for this step.
    pub reward: f64,

    /// Whether this step ended the episode.
    pub done: bool,

    /// When this step occurred.
    pub timestamp: DateTime<Utc>,
}

/// Represents the environment state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentState {
    /// Current working directory.
    pub working_directory: String,

    /// List of files that have been modified so far.
    pub files_modified: Vec<String>,

    /// Output from the last command executed, if any.
    pub last_command_output: Option<String>,

    /// Summary of the current context/conversation.
    pub context_summary: String,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        Self {
            working_directory: String::from("."),
            files_modified: Vec::new(),
            last_command_output: None,
            context_summary: String::new(),
        }
    }
}

/// Represents an action taken by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    /// Name of the tool being called.
    pub tool_name: String,

    /// Arguments passed to the tool.
    pub tool_args: serde_json::Value,

    /// Raw output from the LLM that generated this action.
    pub raw_llm_output: String,

    /// Optional thinking/reasoning from the LLM (for chain-of-thought).
    pub thinking: Option<String>,
}

impl Default for AgentAction {
    fn default() -> Self {
        Self {
            tool_name: String::new(),
            tool_args: serde_json::Value::Null,
            raw_llm_output: String::new(),
            thinking: None,
        }
    }
}

/// Result of executing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Whether the action succeeded.
    pub success: bool,

    /// Output from the action.
    pub output: String,

    /// Error message if the action failed.
    pub error: Option<String>,

    /// Changes to the environment state caused by this action.
    pub state_changes: Vec<StateChange>,
}

impl Default for Observation {
    fn default() -> Self {
        Self {
            success: true,
            output: String::new(),
            error: None,
            state_changes: Vec::new(),
        }
    }
}

/// Represents a change to the environment state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Type of change that occurred.
    pub change_type: ChangeType,

    /// Path affected by the change.
    pub path: String,

    /// Additional details about the change.
    pub details: Option<String>,
}

/// Types of state changes that can occur.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeType {
    /// A new file was created.
    FileCreated,

    /// An existing file was modified.
    FileModified,

    /// A file was deleted.
    FileDeleted,

    /// A new directory was created.
    DirectoryCreated,

    /// A process was started.
    ProcessStarted,

    /// A process ended.
    ProcessEnded,
}

/// Final result of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskResult {
    /// Task completed successfully with a score.
    Success {
        /// Score achieved (typically 0.0 to 1.0).
        score: f64,
    },

    /// Task failed for a specific reason.
    Failure {
        /// Description of why the task failed.
        reason: String,
    },

    /// Task timed out before completion.
    Timeout,

    /// An error occurred during execution.
    Error {
        /// Error message describing what went wrong.
        message: String,
    },
}

impl Default for TaskResult {
    fn default() -> Self {
        TaskResult::Failure {
            reason: String::from("not completed"),
        }
    }
}

/// Token usage statistics for an execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompts.
    pub prompt_tokens: u32,

    /// Number of tokens in the completions.
    pub completion_tokens: u32,

    /// Total number of tokens used.
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Creates a new TokenUsage with the specified values.
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }

    /// Adds another TokenUsage to this one.
    pub fn add(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_add() {
        let mut usage = TokenUsage::new(100, 50);
        let other = TokenUsage::new(200, 100);
        usage.add(&other);
        assert_eq!(usage.prompt_tokens, 300);
        assert_eq!(usage.completion_tokens, 150);
        assert_eq!(usage.total_tokens, 450);
    }

    #[test]
    fn test_environment_state_default() {
        let state = EnvironmentState::default();
        assert_eq!(state.working_directory, ".");
        assert!(state.files_modified.is_empty());
        assert!(state.last_command_output.is_none());
        assert!(state.context_summary.is_empty());
    }

    #[test]
    fn test_task_result_serialization() {
        let success = TaskResult::Success { score: 0.95 };
        let json = serde_json::to_string(&success).expect("serialization should work");
        assert!(json.contains("Success"));
        assert!(json.contains("0.95"));

        let failure = TaskResult::Failure {
            reason: "test error".to_string(),
        };
        let json = serde_json::to_string(&failure).expect("serialization should work");
        assert!(json.contains("Failure"));
        assert!(json.contains("test error"));
    }

    #[test]
    fn test_change_type_equality() {
        assert_eq!(ChangeType::FileCreated, ChangeType::FileCreated);
        assert_ne!(ChangeType::FileCreated, ChangeType::FileModified);
    }
}
