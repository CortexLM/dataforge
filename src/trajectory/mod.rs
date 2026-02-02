//! Trajectory collection system for recording agent execution.
//!
//! This module provides tools for recording, storing, and analyzing
//! agent execution trajectories in SARSA format.
//!
//! # Overview
//!
//! A trajectory consists of a sequence of steps, where each step contains:
//! - **State**: The environment state before the action
//! - **Action**: The tool call made by the agent
//! - **Observation**: The result of the action
//! - **Reward**: A score for this step
//! - **Done**: Whether the episode ended
//!
//! # Usage
//!
//! ```rust,ignore
//! use synth_bench::trajectory::{TrajectoryCollector, RewardCalculator, TrajectoryStorage};
//! use synth_bench::trajectory::types::*;
//!
//! // Create a collector for a new execution
//! let mut collector = TrajectoryCollector::new("task-123", "gpt-4", "react");
//!
//! // Create a reward calculator
//! let mut reward_calc = RewardCalculator::new();
//!
//! // For each step of agent execution:
//! let state = EnvironmentState::default();
//! let action = AgentAction {
//!     tool_name: "write_file".to_string(),
//!     tool_args: serde_json::json!({"path": "test.txt"}),
//!     raw_llm_output: "I'll write to the file".to_string(),
//!     thinking: None,
//! };
//! let observation = Observation {
//!     success: true,
//!     output: "File written".to_string(),
//!     error: None,
//!     state_changes: vec![],
//! };
//!
//! // Calculate reward for this step
//! let reward = reward_calc.calculate_step_reward(&state, &action, &observation);
//!
//! // Record the step
//! collector.record_step(state, action, observation, reward, false);
//!
//! // When done, finalize and save
//! let trajectory = collector.finalize(
//!     TaskResult::Success { score: 1.0 },
//!     60, // duration_seconds
//!     TokenUsage::new(100, 50),
//! );
//!
//! // Save to storage
//! let storage = TrajectoryStorage::new("/path/to/trajectories");
//! storage.save(&trajectory).await?;
//! ```

pub mod collector;
pub mod reward;
pub mod storage;
pub mod types;

// Re-export main types and structs for convenience
pub use collector::TrajectoryCollector;
pub use reward::RewardCalculator;
pub use storage::{StorageError, TrajectoryStorage};
pub use types::{
    AgentAction, ChangeType, EnvironmentState, Observation, StateChange, TaskResult, TokenUsage,
    Trajectory, TrajectoryStep,
};
