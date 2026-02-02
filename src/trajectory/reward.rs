//! Reward calculation for trajectory steps.
//!
//! This module provides the `RewardCalculator` which computes rewards
//! based on agent actions and their outcomes.

use super::types::{
    AgentAction, ChangeType, EnvironmentState, Observation, TaskResult, Trajectory,
};

/// Default weight for success-based rewards.
const DEFAULT_SUCCESS_WEIGHT: f64 = 1.0;

/// Default weight for efficiency-based rewards.
const DEFAULT_EFFICIENCY_WEIGHT: f64 = 0.5;

/// Default weight for progress-based rewards.
const DEFAULT_PROGRESS_WEIGHT: f64 = 0.3;

/// Penalty for failed actions.
const FAILURE_PENALTY: f64 = -0.1;

/// Small reward for making progress (e.g., creating/modifying files).
const PROGRESS_REWARD: f64 = 0.1;

/// Penalty for redundant actions (e.g., reading same file twice).
const REDUNDANCY_PENALTY: f64 = -0.05;

/// Calculator for computing rewards during trajectory collection.
///
/// The reward function is designed to encourage:
/// - Successful completion of actions
/// - Efficient use of steps (not too many unnecessary actions)
/// - Making progress toward the goal (file modifications, etc.)
#[derive(Debug, Clone)]
pub struct RewardCalculator {
    /// Weight applied to success-based reward components.
    pub success_weight: f64,

    /// Weight applied to efficiency-based reward components.
    pub efficiency_weight: f64,

    /// Weight applied to progress-based reward components.
    pub progress_weight: f64,

    /// Track recently seen file paths for redundancy detection.
    recent_file_reads: Vec<String>,
}

impl Default for RewardCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardCalculator {
    /// Creates a new reward calculator with default weights.
    pub fn new() -> Self {
        Self {
            success_weight: DEFAULT_SUCCESS_WEIGHT,
            efficiency_weight: DEFAULT_EFFICIENCY_WEIGHT,
            progress_weight: DEFAULT_PROGRESS_WEIGHT,
            recent_file_reads: Vec::new(),
        }
    }

    /// Creates a reward calculator with custom weights.
    ///
    /// # Arguments
    ///
    /// * `success_weight` - Weight for action success rewards
    /// * `efficiency_weight` - Weight for efficiency-related rewards
    /// * `progress_weight` - Weight for progress-related rewards
    pub fn with_weights(success_weight: f64, efficiency_weight: f64, progress_weight: f64) -> Self {
        Self {
            success_weight,
            efficiency_weight,
            progress_weight,
            recent_file_reads: Vec::new(),
        }
    }

    /// Resets the internal state (e.g., recent file tracking).
    pub fn reset(&mut self) {
        self.recent_file_reads.clear();
    }

    /// Calculates the reward for a single step.
    ///
    /// The reward is computed based on:
    /// - Whether the action succeeded
    /// - Whether progress was made (files created/modified)
    /// - Whether the action was redundant
    ///
    /// # Arguments
    ///
    /// * `state` - Environment state before the action
    /// * `action` - The action that was taken
    /// * `observation` - The result of the action
    ///
    /// # Returns
    ///
    /// A reward value, typically in the range [-1.0, 1.0].
    pub fn calculate_step_reward(
        &mut self,
        state: &EnvironmentState,
        action: &AgentAction,
        observation: &Observation,
    ) -> f64 {
        let mut reward = 0.0;

        // Base reward/penalty for success/failure
        if observation.success {
            reward += 0.1 * self.success_weight;
        } else {
            reward += FAILURE_PENALTY * self.success_weight;
        }

        // Progress reward for state changes
        let progress_reward = self.calculate_progress_reward(observation);
        reward += progress_reward * self.progress_weight;

        // Efficiency: penalize redundant file reads
        let efficiency_reward = self.calculate_efficiency_reward(state, action, observation);
        reward += efficiency_reward * self.efficiency_weight;

        reward
    }

    /// Calculates reward based on progress made (state changes).
    fn calculate_progress_reward(&self, observation: &Observation) -> f64 {
        let mut reward = 0.0;

        for change in &observation.state_changes {
            match change.change_type {
                ChangeType::FileCreated | ChangeType::FileModified => {
                    reward += PROGRESS_REWARD;
                }
                ChangeType::DirectoryCreated => {
                    reward += PROGRESS_REWARD * 0.5;
                }
                ChangeType::ProcessStarted | ChangeType::ProcessEnded => {
                    reward += PROGRESS_REWARD * 0.25;
                }
                ChangeType::FileDeleted => {
                    // Deletion could be intentional cleanup or a mistake
                    // Give small reward if action succeeded
                    if observation.success {
                        reward += PROGRESS_REWARD * 0.25;
                    }
                }
            }
        }

        reward
    }

    /// Calculates efficiency-based reward/penalty.
    fn calculate_efficiency_reward(
        &mut self,
        _state: &EnvironmentState,
        action: &AgentAction,
        observation: &Observation,
    ) -> f64 {
        let mut reward = 0.0;

        // Check for redundant file reads
        let is_read_action = matches!(
            action.tool_name.as_str(),
            "read_file" | "read" | "cat" | "view_file"
        );

        if is_read_action {
            // Extract file path from action args
            if let Some(path) = extract_file_path(&action.tool_args) {
                if self.recent_file_reads.contains(&path) {
                    // Penalize reading the same file twice
                    reward += REDUNDANCY_PENALTY;
                } else {
                    // Track this file read
                    self.recent_file_reads.push(path);

                    // Keep the list bounded
                    if self.recent_file_reads.len() > 50 {
                        self.recent_file_reads.remove(0);
                    }
                }
            }
        }

        // Small penalty for very long outputs (might indicate inefficient exploration)
        if observation.output.len() > 10000 {
            reward -= 0.02;
        }

        reward
    }

    /// Calculates the final reward for a completed trajectory.
    ///
    /// This takes into account:
    /// - Whether the task passed
    /// - Number of steps taken
    /// - Total reward accumulated during execution
    ///
    /// # Arguments
    ///
    /// * `trajectory` - The completed trajectory
    /// * `task_passed` - Whether the task was successfully completed
    ///
    /// # Returns
    ///
    /// A final reward value.
    pub fn calculate_final_reward(&self, trajectory: &Trajectory, task_passed: bool) -> f64 {
        let mut final_reward = trajectory.total_reward;

        // Large bonus/penalty for task success/failure
        if task_passed {
            final_reward += 1.0 * self.success_weight;
        } else {
            final_reward -= 0.5 * self.success_weight;
        }

        // Efficiency bonus: fewer steps is better (up to a point)
        let step_count = trajectory.steps.len() as f64;
        if step_count > 0.0 {
            // Ideal range is 5-15 steps; penalize being too far outside
            let efficiency_bonus = if step_count < 5.0 {
                // Very few steps might indicate incomplete attempt
                -0.1 * (5.0 - step_count) / 5.0
            } else if step_count > 30.0 {
                // Many steps indicates inefficiency
                -0.1 * (step_count - 30.0).min(20.0) / 20.0
            } else {
                // Good range: small bonus
                0.1 * (1.0 - (step_count - 10.0).abs() / 20.0)
            };
            final_reward += efficiency_bonus * self.efficiency_weight;
        }

        // Consider the final result
        match &trajectory.final_result {
            TaskResult::Success { score } => {
                final_reward += score * self.success_weight;
            }
            TaskResult::Failure { .. } => {
                final_reward -= 0.2 * self.success_weight;
            }
            TaskResult::Timeout => {
                final_reward -= 0.3 * self.success_weight;
            }
            TaskResult::Error { .. } => {
                final_reward -= 0.4 * self.success_weight;
            }
        }

        final_reward
    }
}

/// Extracts a file path from tool arguments.
fn extract_file_path(args: &serde_json::Value) -> Option<String> {
    // Try common field names
    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        return Some(path.to_string());
    }
    if let Some(path) = args.get("file_path").and_then(|v| v.as_str()) {
        return Some(path.to_string());
    }
    if let Some(path) = args.get("file").and_then(|v| v.as_str()) {
        return Some(path.to_string());
    }

    // If args is just a string, use that
    if let Some(path) = args.as_str() {
        return Some(path.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{StateChange, TokenUsage, TrajectoryStep};
    use chrono::Utc;
    use serde_json::json;

    fn create_success_observation() -> Observation {
        Observation {
            success: true,
            output: "Success".to_string(),
            error: None,
            state_changes: vec![],
        }
    }

    fn create_failure_observation() -> Observation {
        Observation {
            success: false,
            output: String::new(),
            error: Some("Error occurred".to_string()),
            state_changes: vec![],
        }
    }

    fn create_observation_with_changes() -> Observation {
        Observation {
            success: true,
            output: "File written".to_string(),
            error: None,
            state_changes: vec![StateChange {
                change_type: ChangeType::FileCreated,
                path: "/test/file.txt".to_string(),
                details: None,
            }],
        }
    }

    #[test]
    fn test_calculator_new() {
        let calc = RewardCalculator::new();
        assert!((calc.success_weight - DEFAULT_SUCCESS_WEIGHT).abs() < f64::EPSILON);
        assert!((calc.efficiency_weight - DEFAULT_EFFICIENCY_WEIGHT).abs() < f64::EPSILON);
        assert!((calc.progress_weight - DEFAULT_PROGRESS_WEIGHT).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculator_with_weights() {
        let calc = RewardCalculator::with_weights(2.0, 1.0, 0.5);
        assert!((calc.success_weight - 2.0).abs() < f64::EPSILON);
        assert!((calc.efficiency_weight - 1.0).abs() < f64::EPSILON);
        assert!((calc.progress_weight - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_step_reward_success() {
        let mut calc = RewardCalculator::new();
        let state = EnvironmentState::default();
        let action = AgentAction::default();
        let observation = create_success_observation();

        let reward = calc.calculate_step_reward(&state, &action, &observation);
        assert!(reward > 0.0, "Success should have positive reward");
    }

    #[test]
    fn test_step_reward_failure() {
        let mut calc = RewardCalculator::new();
        let state = EnvironmentState::default();
        let action = AgentAction::default();
        let observation = create_failure_observation();

        let reward = calc.calculate_step_reward(&state, &action, &observation);
        assert!(reward < 0.0, "Failure should have negative reward");
    }

    #[test]
    fn test_step_reward_with_progress() {
        let mut calc = RewardCalculator::new();
        let state = EnvironmentState::default();
        let action = AgentAction::default();
        let observation = create_observation_with_changes();

        let reward_with_progress = calc.calculate_step_reward(&state, &action, &observation);

        let mut calc2 = RewardCalculator::new();
        let observation_no_progress = create_success_observation();
        let reward_no_progress =
            calc2.calculate_step_reward(&state, &action, &observation_no_progress);

        assert!(
            reward_with_progress > reward_no_progress,
            "Progress should increase reward"
        );
    }

    #[test]
    fn test_redundant_file_read_penalty() {
        let mut calc = RewardCalculator::new();
        let state = EnvironmentState::default();
        let action = AgentAction {
            tool_name: "read_file".to_string(),
            tool_args: json!({"path": "/test/file.txt"}),
            raw_llm_output: String::new(),
            thinking: None,
        };
        let observation = create_success_observation();

        let first_reward = calc.calculate_step_reward(&state, &action, &observation);
        let second_reward = calc.calculate_step_reward(&state, &action, &observation);

        assert!(
            second_reward < first_reward,
            "Reading same file twice should be penalized"
        );
    }

    #[test]
    fn test_extract_file_path() {
        assert_eq!(
            extract_file_path(&json!({"path": "/test/file.txt"})),
            Some("/test/file.txt".to_string())
        );
        assert_eq!(
            extract_file_path(&json!({"file_path": "/test/file.txt"})),
            Some("/test/file.txt".to_string())
        );
        assert_eq!(
            extract_file_path(&json!({"file": "/test/file.txt"})),
            Some("/test/file.txt".to_string())
        );
        assert_eq!(
            extract_file_path(&json!("/test/file.txt")),
            Some("/test/file.txt".to_string())
        );
        assert_eq!(extract_file_path(&json!({})), None);
    }

    #[test]
    fn test_final_reward_task_passed() {
        let calc = RewardCalculator::new();
        let trajectory = create_test_trajectory(10, true);

        let reward_passed = calc.calculate_final_reward(&trajectory, true);
        let reward_failed = calc.calculate_final_reward(&trajectory, false);

        assert!(
            reward_passed > reward_failed,
            "Passing task should have higher reward"
        );
    }

    #[test]
    fn test_final_reward_efficiency() {
        let calc = RewardCalculator::new();

        // Use same total_reward to isolate efficiency component
        let short_trajectory = create_test_trajectory_with_reward(5, true, 1.0);
        let medium_trajectory = create_test_trajectory_with_reward(10, true, 1.0);
        let long_trajectory = create_test_trajectory_with_reward(40, true, 1.0);

        let short_reward = calc.calculate_final_reward(&short_trajectory, true);
        let medium_reward = calc.calculate_final_reward(&medium_trajectory, true);
        let long_reward = calc.calculate_final_reward(&long_trajectory, true);

        // Medium should be best (efficiency sweet spot)
        assert!(
            medium_reward >= short_reward,
            "Medium trajectory should have good efficiency: medium={} short={}",
            medium_reward,
            short_reward
        );
        assert!(
            medium_reward > long_reward,
            "Long trajectory should have lower efficiency reward: medium={} long={}",
            medium_reward,
            long_reward
        );
    }

    fn create_test_trajectory(step_count: usize, success: bool) -> Trajectory {
        create_test_trajectory_with_reward(step_count, success, step_count as f64 * 0.1)
    }

    fn create_test_trajectory_with_reward(
        step_count: usize,
        success: bool,
        total_reward: f64,
    ) -> Trajectory {
        let steps: Vec<TrajectoryStep> = (0..step_count)
            .map(|i| TrajectoryStep {
                step_number: i as u32,
                state: EnvironmentState::default(),
                action: AgentAction::default(),
                observation: Observation::default(),
                reward: total_reward / step_count as f64,
                done: i == step_count - 1,
                timestamp: Utc::now(),
            })
            .collect();

        Trajectory {
            id: uuid::Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "test-model".to_string(),
            scaffold_type: "test".to_string(),
            steps,
            final_result: if success {
                TaskResult::Success { score: 1.0 }
            } else {
                TaskResult::Failure {
                    reason: "Test failure".to_string(),
                }
            },
            total_reward,
            created_at: Utc::now(),
            duration_seconds: 60,
            token_usage: TokenUsage::default(),
        }
    }

    #[test]
    fn test_reset() {
        let mut calc = RewardCalculator::new();
        let state = EnvironmentState::default();
        let action = AgentAction {
            tool_name: "read_file".to_string(),
            tool_args: json!({"path": "/test/file.txt"}),
            raw_llm_output: String::new(),
            thinking: None,
        };
        let observation = create_success_observation();

        calc.calculate_step_reward(&state, &action, &observation);
        assert!(!calc.recent_file_reads.is_empty());

        calc.reset();
        assert!(calc.recent_file_reads.is_empty());
    }
}
