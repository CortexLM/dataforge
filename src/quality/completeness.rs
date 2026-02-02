//! Completeness checking for trajectories.
//!
//! Verifies that trajectories have all necessary steps, proper state changes,
//! and complete observations for all actions.

use crate::trajectory::types::{ChangeType, Trajectory};

use super::filter::{QualityIssue, QualityIssueType, Severity};

/// Default minimum number of steps expected in a trajectory.
const DEFAULT_MIN_STEPS: u32 = 1;

/// Default maximum number of steps expected in a trajectory.
const DEFAULT_MAX_STEPS: u32 = 100;

/// Checks the completeness of trajectories.
///
/// Ensures trajectories have a reasonable number of steps,
/// all necessary state changes, and complete observations.
pub struct CompletenessChecker {
    /// Minimum expected steps for a valid trajectory.
    min_steps: u32,
    /// Maximum expected steps before considering trajectory too long.
    max_steps: u32,
}

impl CompletenessChecker {
    /// Creates a new completeness checker with the specified step bounds.
    ///
    /// # Arguments
    ///
    /// * `min_steps` - Minimum number of steps expected.
    /// * `max_steps` - Maximum number of steps before penalty.
    pub fn new(min_steps: u32, max_steps: u32) -> Self {
        Self {
            min_steps: min_steps.max(1),
            max_steps: max_steps.max(min_steps),
        }
    }

    /// Creates a completeness checker with default settings.
    pub fn default_checker() -> Self {
        Self::new(DEFAULT_MIN_STEPS, DEFAULT_MAX_STEPS)
    }

    /// Checks if the trajectory has a reasonable number of steps.
    ///
    /// Returns a tuple of (score, issues) where score is 0.0-1.0.
    pub fn check_step_count(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        let step_count = trajectory.steps.len() as u32;
        let mut issues = Vec::new();

        // Empty trajectory
        if step_count == 0 {
            issues.push(QualityIssue::new(
                QualityIssueType::EmptyTrajectory,
                Severity::Critical,
                "Trajectory has no steps",
            ));
            return (0.0, issues);
        }

        // Below minimum
        if step_count < self.min_steps {
            issues.push(QualityIssue::new(
                QualityIssueType::MissingStep,
                Severity::Major,
                format!(
                    "Trajectory has only {} steps, expected at least {}",
                    step_count, self.min_steps
                ),
            ));
            let ratio = step_count as f64 / self.min_steps as f64;
            return (ratio.min(1.0), issues);
        }

        // Above maximum (not critical, but penalized)
        if step_count > self.max_steps {
            let over_count = step_count - self.max_steps;
            let severity = if over_count > self.max_steps / 2 {
                Severity::Major
            } else {
                Severity::Minor
            };

            issues.push(QualityIssue::new(
                QualityIssueType::RedundantStep,
                severity,
                format!(
                    "Trajectory has {} steps, which exceeds maximum of {}",
                    step_count, self.max_steps
                ),
            ));

            // Penalty increases with excess steps
            let excess_ratio = (step_count - self.max_steps) as f64 / self.max_steps as f64;
            let score = (1.0 - excess_ratio * 0.3).max(0.5);
            return (score, issues);
        }

        // Within bounds - full score
        (1.0, issues)
    }

    /// Checks if the trajectory has necessary state changes.
    ///
    /// Verifies that for trajectories with file-related operations,
    /// there are corresponding state changes recorded.
    ///
    /// Returns a score from 0.0 to 1.0.
    pub fn check_state_changes(&self, trajectory: &Trajectory) -> f64 {
        if trajectory.steps.is_empty() {
            return 0.0;
        }

        // Identify steps that should have state changes
        let mut expected_changes = 0;
        let mut actual_changes = 0;

        for step in &trajectory.steps {
            let tool = step.action.tool_name.to_lowercase();

            // These tools typically produce state changes
            let should_have_change = tool.contains("write")
                || tool.contains("edit")
                || tool.contains("create")
                || tool.contains("delete")
                || tool.contains("move")
                || tool.contains("copy")
                || tool.contains("mkdir");

            if should_have_change && step.observation.success {
                expected_changes += 1;
                if !step.observation.state_changes.is_empty() {
                    actual_changes += 1;
                }
            }
        }

        // If no state changes expected, consider it complete
        if expected_changes == 0 {
            return 1.0;
        }

        actual_changes as f64 / expected_changes as f64
    }

    /// Checks if all tool calls have corresponding observations.
    ///
    /// Verifies that every action has a proper observation response.
    ///
    /// Returns a tuple of (score, issues) where score is 0.0-1.0.
    pub fn check_observation_completeness(
        &self,
        trajectory: &Trajectory,
    ) -> (f64, Vec<QualityIssue>) {
        if trajectory.steps.is_empty() {
            return (0.0, vec![]);
        }

        let mut issues = Vec::new();
        let mut complete_observations = 0;

        for step in &trajectory.steps {
            let observation = &step.observation;

            // Check for empty observations on non-successful actions
            let is_complete = if observation.success {
                // Successful actions should have some output
                !observation.output.is_empty() || !observation.state_changes.is_empty()
            } else {
                // Failed actions should have an error message
                observation.error.is_some()
            };

            if is_complete {
                complete_observations += 1;
            } else {
                let severity = if observation.success {
                    Severity::Minor
                } else {
                    Severity::Major
                };

                issues.push(QualityIssue::with_step(
                    QualityIssueType::MissingStep,
                    severity,
                    format!(
                        "Step {} has incomplete observation: success={} but no output/error",
                        step.step_number, observation.success
                    ),
                    step.step_number,
                ));
            }
        }

        let score = complete_observations as f64 / trajectory.steps.len() as f64;
        (score, issues)
    }

    /// Checks if the trajectory ends properly.
    ///
    /// Verifies that the last step indicates completion and
    /// the trajectory has a valid final result.
    fn check_trajectory_termination(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        if trajectory.steps.is_empty() {
            return (0.0, vec![]);
        }

        let mut issues = Vec::new();

        // Check if any step is marked as done
        let has_done_step = trajectory.steps.iter().any(|s| s.done);
        let last_step = trajectory.steps.last().expect("steps is not empty");

        // Ideally, the last step should be marked as done
        if !last_step.done && !has_done_step {
            // Check if the final result indicates success
            if matches!(
                trajectory.final_result,
                crate::trajectory::types::TaskResult::Success { .. }
            ) {
                // Success without explicit done marker is a minor issue
                issues.push(QualityIssue::with_step(
                    QualityIssueType::MissingStep,
                    Severity::Warning,
                    "Trajectory ended without explicit completion marker",
                    last_step.step_number,
                ));
                return (0.9, issues);
            }
        }

        (1.0, issues)
    }

    /// Checks for required state changes based on the task type.
    fn check_required_state_changes(&self, trajectory: &Trajectory) -> Vec<QualityIssue> {
        let mut issues = Vec::new();

        // Collect all state changes
        let all_changes: Vec<_> = trajectory
            .steps
            .iter()
            .flat_map(|s| s.observation.state_changes.iter())
            .collect();

        // Check for common patterns that indicate incomplete work
        let has_file_created = all_changes
            .iter()
            .any(|c| c.change_type == ChangeType::FileCreated);
        let has_file_modified = all_changes
            .iter()
            .any(|c| c.change_type == ChangeType::FileModified);

        // If there are edits in the actions but no corresponding state changes
        let has_edit_actions = trajectory
            .steps
            .iter()
            .any(|s| s.action.tool_name.to_lowercase().contains("edit"));

        if has_edit_actions && !has_file_modified && !has_file_created {
            issues.push(QualityIssue::new(
                QualityIssueType::MissingStep,
                Severity::Minor,
                "Trajectory has edit actions but no file modification state changes recorded",
            ));
        }

        issues
    }

    /// Evaluates the overall completeness of a trajectory.
    ///
    /// Returns a tuple of (score, issues) where score is 0.0-1.0.
    pub fn evaluate(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        let mut all_issues = Vec::new();

        // Weight factors for different completeness checks
        const STEP_COUNT_WEIGHT: f64 = 0.3;
        const STATE_CHANGES_WEIGHT: f64 = 0.2;
        const OBSERVATION_WEIGHT: f64 = 0.35;
        const TERMINATION_WEIGHT: f64 = 0.15;

        // Check step count
        let (step_score, step_issues) = self.check_step_count(trajectory);
        all_issues.extend(step_issues);

        // If step count is 0, return early
        if step_score == 0.0 {
            return (0.0, all_issues);
        }

        // Check state changes
        let state_score = self.check_state_changes(trajectory);

        // Check observation completeness
        let (obs_score, obs_issues) = self.check_observation_completeness(trajectory);
        all_issues.extend(obs_issues);

        // Check termination
        let (term_score, term_issues) = self.check_trajectory_termination(trajectory);
        all_issues.extend(term_issues);

        // Check required state changes
        let required_issues = self.check_required_state_changes(trajectory);
        all_issues.extend(required_issues);

        // Calculate weighted average
        let overall_score = STEP_COUNT_WEIGHT * step_score
            + STATE_CHANGES_WEIGHT * state_score
            + OBSERVATION_WEIGHT * obs_score
            + TERMINATION_WEIGHT * term_score;

        (overall_score.clamp(0.0, 1.0), all_issues)
    }
}

impl Default for CompletenessChecker {
    fn default() -> Self {
        Self::default_checker()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{
        AgentAction, ChangeType, EnvironmentState, Observation, StateChange, TaskResult,
        TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(steps: Vec<TrajectoryStep>, result: TaskResult) -> Trajectory {
        Trajectory {
            id: Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "test-model".to_string(),
            scaffold_type: "basic".to_string(),
            steps,
            final_result: result,
            total_reward: 1.0,
            created_at: Utc::now(),
            duration_seconds: 10,
            token_usage: TokenUsage::default(),
        }
    }

    fn create_step(
        step_number: u32,
        tool_name: &str,
        output: &str,
        state_changes: Vec<StateChange>,
    ) -> TrajectoryStep {
        TrajectoryStep {
            step_number,
            state: EnvironmentState::default(),
            action: AgentAction {
                tool_name: tool_name.to_string(),
                tool_args: serde_json::json!({}),
                raw_llm_output: "Output".to_string(),
                thinking: None,
            },
            observation: Observation {
                success: true,
                output: output.to_string(),
                error: None,
                state_changes,
            },
            reward: 0.1,
            done: false,
            timestamp: Utc::now(),
        }
    }

    fn create_failed_step(step_number: u32, error: Option<&str>) -> TrajectoryStep {
        TrajectoryStep {
            step_number,
            state: EnvironmentState::default(),
            action: AgentAction {
                tool_name: "test_tool".to_string(),
                tool_args: serde_json::json!({}),
                raw_llm_output: "".to_string(),
                thinking: None,
            },
            observation: Observation {
                success: false,
                output: String::new(),
                error: error.map(|e| e.to_string()),
                state_changes: vec![],
            },
            reward: -0.1,
            done: false,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_check_step_count_empty() {
        let checker = CompletenessChecker::new(1, 100);
        let trajectory = create_test_trajectory(vec![], TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_step_count(&trajectory);
        assert_eq!(score, 0.0);
        assert!(!issues.is_empty());
        assert_eq!(issues[0].issue_type, QualityIssueType::EmptyTrajectory);
    }

    #[test]
    fn test_check_step_count_below_minimum() {
        let checker = CompletenessChecker::new(5, 100);
        let steps = vec![
            create_step(0, "read", "output", vec![]),
            create_step(1, "write", "output", vec![]),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_step_count(&trajectory);
        assert!(score < 1.0);
        assert!(!issues.is_empty());
        assert_eq!(issues[0].issue_type, QualityIssueType::MissingStep);
    }

    #[test]
    fn test_check_step_count_above_maximum() {
        let checker = CompletenessChecker::new(1, 3);
        let steps = vec![
            create_step(0, "a", "o", vec![]),
            create_step(1, "b", "o", vec![]),
            create_step(2, "c", "o", vec![]),
            create_step(3, "d", "o", vec![]),
            create_step(4, "e", "o", vec![]),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_step_count(&trajectory);
        assert!(score < 1.0);
        assert!(!issues.is_empty());
        assert_eq!(issues[0].issue_type, QualityIssueType::RedundantStep);
    }

    #[test]
    fn test_check_step_count_valid() {
        let checker = CompletenessChecker::new(2, 10);
        let steps = vec![
            create_step(0, "read", "output", vec![]),
            create_step(1, "write", "output", vec![]),
            create_step(2, "test", "output", vec![]),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_step_count(&trajectory);
        assert_eq!(score, 1.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_state_changes_with_changes() {
        let checker = CompletenessChecker::new(1, 100);
        let state_change = StateChange {
            change_type: ChangeType::FileModified,
            path: "test.rs".to_string(),
            details: None,
        };
        let steps = vec![create_step(0, "edit_file", "edited", vec![state_change])];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let score = checker.check_state_changes(&trajectory);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_check_state_changes_missing() {
        let checker = CompletenessChecker::new(1, 100);
        let steps = vec![create_step(0, "edit_file", "edited", vec![])];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let score = checker.check_state_changes(&trajectory);
        assert!(score < 1.0);
    }

    #[test]
    fn test_check_observation_completeness_complete() {
        let checker = CompletenessChecker::new(1, 100);
        let steps = vec![
            create_step(0, "read", "file contents here", vec![]),
            create_step(1, "write", "written successfully", vec![]),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_observation_completeness(&trajectory);
        assert_eq!(score, 1.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_observation_completeness_with_failed_step() {
        let checker = CompletenessChecker::new(1, 100);
        let steps = vec![
            create_step(0, "read", "output", vec![]),
            create_failed_step(1, Some("File not found")),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_observation_completeness(&trajectory);
        // Both steps are complete: one with output, one with error
        assert_eq!(score, 1.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_observation_completeness_missing_error() {
        let checker = CompletenessChecker::new(1, 100);
        let steps = vec![
            create_step(0, "read", "output", vec![]),
            create_failed_step(1, None), // Failed but no error message
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.check_observation_completeness(&trajectory);
        assert!(score < 1.0);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_evaluate_complete_trajectory() {
        let checker = CompletenessChecker::new(2, 10);
        let state_change = StateChange {
            change_type: ChangeType::FileModified,
            path: "test.rs".to_string(),
            details: None,
        };
        let mut steps = vec![
            create_step(0, "read_file", "contents", vec![]),
            create_step(1, "edit_file", "edited", vec![state_change]),
        ];
        // Mark last step as done
        steps[1].done = true;

        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.evaluate(&trajectory);
        assert!(score > 0.8);
        assert!(issues.is_empty() || issues.iter().all(|i| i.severity == Severity::Warning));
    }

    #[test]
    fn test_evaluate_empty_trajectory() {
        let checker = CompletenessChecker::new(1, 100);
        let trajectory = create_test_trajectory(vec![], TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.evaluate(&trajectory);
        assert_eq!(score, 0.0);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_default_checker() {
        let checker = CompletenessChecker::default();
        assert_eq!(checker.min_steps, DEFAULT_MIN_STEPS);
        assert_eq!(checker.max_steps, DEFAULT_MAX_STEPS);
    }

    #[test]
    fn test_new_clamps_min_steps() {
        let checker = CompletenessChecker::new(0, 100);
        assert_eq!(checker.min_steps, 1);
    }

    #[test]
    fn test_new_clamps_max_steps() {
        let checker = CompletenessChecker::new(50, 30);
        assert_eq!(checker.max_steps, 50);
    }
}
