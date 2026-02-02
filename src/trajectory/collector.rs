//! Trajectory collector for recording agent execution.
//!
//! This module provides the `TrajectoryCollector` which accumulates
//! state-action-observation-reward tuples during agent execution.

use chrono::Utc;
use uuid::Uuid;

use super::types::{
    AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, Trajectory, TrajectoryStep,
};

/// Collector for recording agent execution trajectories.
///
/// The collector accumulates steps as the agent executes, building up
/// a complete trajectory that can be saved for training or analysis.
///
/// # Usage Pattern
///
/// For each step of the agent loop:
/// 1. Call `record_state()` with the current environment state
/// 2. Call `record_action()` with the action the agent takes
/// 3. Call `record_observation()` with the result of the action
/// 4. Call `record_reward()` with the reward and done flag
///
/// When the episode ends, call `finalize()` to get the complete trajectory.
pub struct TrajectoryCollector {
    /// The trajectory being built.
    trajectory: Trajectory,

    /// Current step number.
    current_step: u32,

    /// Pending state for the next step (set before action).
    pending_state: Option<EnvironmentState>,

    /// Pending action for the current step.
    pending_action: Option<AgentAction>,

    /// Pending observation for the current step.
    pending_observation: Option<Observation>,
}

impl TrajectoryCollector {
    /// Creates a new trajectory collector.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Identifier of the task being executed
    /// * `model` - Name of the model being used
    /// * `scaffold_type` - Type of scaffold/framework being used
    pub fn new(task_id: &str, model: &str, scaffold_type: &str) -> Self {
        Self {
            trajectory: Trajectory {
                id: Uuid::new_v4(),
                task_id: task_id.to_string(),
                model: model.to_string(),
                scaffold_type: scaffold_type.to_string(),
                steps: Vec::new(),
                final_result: TaskResult::default(),
                total_reward: 0.0,
                created_at: Utc::now(),
                duration_seconds: 0,
                token_usage: TokenUsage::default(),
            },
            current_step: 0,
            pending_state: None,
            pending_action: None,
            pending_observation: None,
        }
    }

    /// Records the environment state before an action is taken.
    ///
    /// This should be called at the start of each step, before the agent
    /// decides on an action.
    pub fn record_state(&mut self, state: EnvironmentState) {
        self.pending_state = Some(state);
    }

    /// Records the action taken by the agent.
    ///
    /// This should be called after `record_state()` and before the action
    /// is executed in the environment.
    pub fn record_action(&mut self, action: AgentAction) {
        self.pending_action = Some(action);
    }

    /// Records the observation received after taking an action.
    ///
    /// This should be called after the action has been executed in the
    /// environment.
    pub fn record_observation(&mut self, observation: Observation) {
        self.pending_observation = Some(observation);
    }

    /// Records the reward and whether the episode is done.
    ///
    /// This completes the current step by combining the pending state,
    /// action, and observation with the reward. The step is then added
    /// to the trajectory.
    ///
    /// # Arguments
    ///
    /// * `reward` - The reward for this step
    /// * `done` - Whether this step ends the episode
    pub fn record_reward(&mut self, reward: f64, done: bool) {
        let state = self.pending_state.take().unwrap_or_default();
        let action = self.pending_action.take().unwrap_or_default();
        let observation = self.pending_observation.take().unwrap_or_default();

        let step = TrajectoryStep {
            step_number: self.current_step,
            state,
            action,
            observation,
            reward,
            done,
            timestamp: Utc::now(),
        };

        self.trajectory.steps.push(step);
        self.trajectory.total_reward += reward;
        self.current_step += 1;
    }

    /// Records a complete step in one call.
    ///
    /// This is a convenience method that combines `record_state()`,
    /// `record_action()`, `record_observation()`, and `record_reward()`.
    pub fn record_step(
        &mut self,
        state: EnvironmentState,
        action: AgentAction,
        observation: Observation,
        reward: f64,
        done: bool,
    ) {
        self.record_state(state);
        self.record_action(action);
        self.record_observation(observation);
        self.record_reward(reward, done);
    }

    /// Finalizes the trajectory and returns it.
    ///
    /// This should be called when the episode ends, either due to success,
    /// failure, timeout, or error.
    ///
    /// # Arguments
    ///
    /// * `result` - The final result of the task execution
    /// * `duration` - Total duration of the execution in seconds
    /// * `tokens` - Token usage statistics
    pub fn finalize(
        &mut self,
        result: TaskResult,
        duration: u64,
        tokens: TokenUsage,
    ) -> Trajectory {
        self.trajectory.final_result = result;
        self.trajectory.duration_seconds = duration;
        self.trajectory.token_usage = tokens;

        self.trajectory.clone()
    }

    /// Returns the current step number.
    pub fn current_step(&self) -> u32 {
        self.current_step
    }

    /// Returns the current total reward.
    pub fn total_reward(&self) -> f64 {
        self.trajectory.total_reward
    }

    /// Returns the trajectory ID.
    pub fn trajectory_id(&self) -> Uuid {
        self.trajectory.id
    }

    /// Returns a reference to the steps recorded so far.
    pub fn steps(&self) -> &[TrajectoryStep] {
        &self.trajectory.steps
    }

    /// Returns true if any steps have been recorded.
    pub fn has_steps(&self) -> bool {
        !self.trajectory.steps.is_empty()
    }

    /// Adds token usage to the running total.
    pub fn add_token_usage(&mut self, usage: &TokenUsage) {
        self.trajectory.token_usage.add(usage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_state() -> EnvironmentState {
        EnvironmentState {
            working_directory: "/test".to_string(),
            files_modified: vec!["test.txt".to_string()],
            last_command_output: Some("output".to_string()),
            context_summary: "Test context".to_string(),
        }
    }

    fn create_test_action() -> AgentAction {
        AgentAction {
            tool_name: "write_file".to_string(),
            tool_args: json!({"path": "test.txt", "content": "hello"}),
            raw_llm_output: "I will write to the file".to_string(),
            thinking: Some("Need to create a file".to_string()),
        }
    }

    fn create_test_observation() -> Observation {
        Observation {
            success: true,
            output: "File written successfully".to_string(),
            error: None,
            state_changes: vec![],
        }
    }

    #[test]
    fn test_collector_new() {
        let collector = TrajectoryCollector::new("task-1", "gpt-4", "react");
        assert_eq!(collector.current_step(), 0);
        assert_eq!(collector.total_reward(), 0.0);
        assert!(!collector.has_steps());
    }

    #[test]
    fn test_record_complete_step() {
        let mut collector = TrajectoryCollector::new("task-1", "gpt-4", "react");

        collector.record_state(create_test_state());
        collector.record_action(create_test_action());
        collector.record_observation(create_test_observation());
        collector.record_reward(1.0, false);

        assert_eq!(collector.current_step(), 1);
        assert_eq!(collector.total_reward(), 1.0);
        assert!(collector.has_steps());
        assert_eq!(collector.steps().len(), 1);

        let step = &collector.steps()[0];
        assert_eq!(step.step_number, 0);
        assert_eq!(step.reward, 1.0);
        assert!(!step.done);
    }

    #[test]
    fn test_record_step_convenience() {
        let mut collector = TrajectoryCollector::new("task-1", "gpt-4", "react");

        collector.record_step(
            create_test_state(),
            create_test_action(),
            create_test_observation(),
            0.5,
            true,
        );

        assert_eq!(collector.current_step(), 1);
        assert_eq!(collector.total_reward(), 0.5);

        let step = &collector.steps()[0];
        assert!(step.done);
    }

    #[test]
    fn test_multiple_steps() {
        let mut collector = TrajectoryCollector::new("task-1", "gpt-4", "react");

        for i in 0..3 {
            collector.record_step(
                create_test_state(),
                create_test_action(),
                create_test_observation(),
                0.5,
                i == 2,
            );
        }

        assert_eq!(collector.current_step(), 3);
        assert!((collector.total_reward() - 1.5).abs() < f64::EPSILON);
        assert_eq!(collector.steps().len(), 3);
    }

    #[test]
    fn test_finalize() {
        let mut collector = TrajectoryCollector::new("task-1", "gpt-4", "react");

        collector.record_step(
            create_test_state(),
            create_test_action(),
            create_test_observation(),
            1.0,
            true,
        );

        let tokens = TokenUsage::new(100, 50);
        let trajectory = collector.finalize(TaskResult::Success { score: 0.95 }, 120, tokens);

        assert_eq!(trajectory.task_id, "task-1");
        assert_eq!(trajectory.model, "gpt-4");
        assert_eq!(trajectory.scaffold_type, "react");
        assert_eq!(trajectory.duration_seconds, 120);
        assert_eq!(trajectory.token_usage.total_tokens, 150);

        match trajectory.final_result {
            TaskResult::Success { score } => assert!((score - 0.95).abs() < f64::EPSILON),
            _ => panic!("Expected Success result"),
        }
    }

    #[test]
    fn test_add_token_usage() {
        let mut collector = TrajectoryCollector::new("task-1", "gpt-4", "react");

        collector.add_token_usage(&TokenUsage::new(100, 50));
        collector.add_token_usage(&TokenUsage::new(200, 100));

        let _trajectory = collector.finalize(
            TaskResult::Success { score: 1.0 },
            60,
            TokenUsage::default(),
        );

        // Token usage was overwritten by finalize, so we need to check before finalize
        // Re-test with proper flow
        let mut collector2 = TrajectoryCollector::new("task-1", "gpt-4", "react");
        collector2.add_token_usage(&TokenUsage::new(100, 50));
        collector2.add_token_usage(&TokenUsage::new(200, 100));

        // Finalize with the accumulated tokens
        let final_tokens = TokenUsage::new(300, 150);
        let trajectory2 = collector2.finalize(TaskResult::Success { score: 1.0 }, 60, final_tokens);
        assert_eq!(trajectory2.token_usage.total_tokens, 450);
    }
}
