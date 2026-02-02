//! Correctness verification for trajectories.
//!
//! Checks whether the task was completed correctly based on the task result,
//! test outputs, and final state.

use crate::trajectory::types::{TaskResult, Trajectory};

use super::filter::{QualityIssue, QualityIssueType, Severity};

/// Verifies the correctness of trajectory outcomes.
///
/// Checks task results, test outputs, and other indicators to determine
/// whether the trajectory achieved its goal correctly.
pub struct CorrectnessChecker {
    /// Whether to use strict checking (fails on any test failure).
    strict_mode: bool,
}

impl CorrectnessChecker {
    /// Creates a new correctness checker.
    ///
    /// # Arguments
    ///
    /// * `strict_mode` - If true, any test failure or partial success results in lower scores.
    pub fn new(strict_mode: bool) -> Self {
        Self { strict_mode }
    }

    /// Checks if the task result indicates success.
    ///
    /// Returns a score from 0.0 (complete failure) to 1.0 (complete success).
    pub fn check_task_result(&self, result: &TaskResult) -> f64 {
        match result {
            TaskResult::Success { score } => {
                // In strict mode, require perfect score
                if self.strict_mode {
                    if *score >= 1.0 {
                        1.0
                    } else {
                        *score * 0.8
                    }
                } else {
                    *score
                }
            }
            TaskResult::Failure { reason: _ } => 0.0,
            TaskResult::Timeout => 0.0,
            TaskResult::Error { message: _ } => 0.0,
        }
    }

    /// Verifies test outputs match expected patterns.
    ///
    /// Performs a simple substring match to check if the expected output
    /// appears anywhere in the trajectory's steps.
    ///
    /// Returns a score from 0.0 (no match) to 1.0 (full match).
    pub fn check_test_output(&self, trajectory: &Trajectory, expected: &str) -> f64 {
        if expected.is_empty() {
            return 1.0;
        }

        // Search through all steps for the expected output
        for step in &trajectory.steps {
            if step.observation.output.contains(expected) {
                return 1.0;
            }
        }

        // Partial matching: check if any parts of expected appear
        let expected_parts: Vec<&str> = expected.split_whitespace().collect();
        if expected_parts.is_empty() {
            return 1.0;
        }

        let mut matches = 0;
        for step in &trajectory.steps {
            for part in &expected_parts {
                if step.observation.output.contains(part) {
                    matches += 1;
                }
            }
        }

        let unique_matches = matches.min(expected_parts.len());
        unique_matches as f64 / expected_parts.len() as f64
    }

    /// Checks if all actions in the trajectory succeeded.
    ///
    /// Returns a score based on the ratio of successful steps.
    fn check_action_success_rate(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        if trajectory.steps.is_empty() {
            return (0.0, vec![]);
        }

        let mut issues = Vec::new();
        let mut failed_steps = 0;

        for step in &trajectory.steps {
            if !step.observation.success {
                failed_steps += 1;

                // Only report the first few failures to avoid noise
                if failed_steps <= 3 {
                    let severity = if self.strict_mode {
                        Severity::Major
                    } else {
                        Severity::Minor
                    };

                    issues.push(QualityIssue::with_step(
                        QualityIssueType::FailedTest,
                        severity,
                        format!(
                            "Step {} failed: {}",
                            step.step_number,
                            step.observation.error.as_deref().unwrap_or("unknown error")
                        ),
                        step.step_number,
                    ));
                }
            }
        }

        let success_rate = 1.0 - (failed_steps as f64 / trajectory.steps.len() as f64);

        // In strict mode, penalize failures more heavily
        let adjusted_rate = if self.strict_mode && failed_steps > 0 {
            success_rate * 0.7
        } else {
            success_rate
        };

        (adjusted_rate, issues)
    }

    /// Checks if the trajectory has positive total reward.
    fn check_reward(&self, trajectory: &Trajectory) -> f64 {
        if trajectory.total_reward > 0.0 {
            (trajectory.total_reward / (trajectory.total_reward.abs() + 1.0)).clamp(0.0, 1.0)
        } else if trajectory.total_reward == 0.0 {
            0.5
        } else {
            0.0
        }
    }

    /// Evaluates the overall correctness of a trajectory.
    ///
    /// Returns a tuple of (score, issues) where score is 0.0-1.0.
    pub fn evaluate(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        let mut all_issues = Vec::new();

        // Weight factors for different correctness checks
        const TASK_RESULT_WEIGHT: f64 = 0.5;
        const ACTION_SUCCESS_WEIGHT: f64 = 0.35;
        const REWARD_WEIGHT: f64 = 0.15;

        // Check task result
        let task_result_score = self.check_task_result(&trajectory.final_result);

        // If task completely failed, add an issue
        if task_result_score == 0.0 {
            let description = match &trajectory.final_result {
                TaskResult::Failure { reason } => format!("Task failed: {}", reason),
                TaskResult::Timeout => "Task timed out".to_string(),
                TaskResult::Error { message } => format!("Task error: {}", message),
                TaskResult::Success { score: _ } => {
                    "Task did not complete successfully".to_string()
                }
            };

            all_issues.push(QualityIssue::new(
                QualityIssueType::IncorrectOutput,
                Severity::Critical,
                description,
            ));
        }

        // Check action success rate
        let (action_score, action_issues) = self.check_action_success_rate(trajectory);
        all_issues.extend(action_issues);

        // Check reward
        let reward_score = self.check_reward(trajectory);

        // Calculate weighted average
        let overall_score = TASK_RESULT_WEIGHT * task_result_score
            + ACTION_SUCCESS_WEIGHT * action_score
            + REWARD_WEIGHT * reward_score;

        (overall_score.clamp(0.0, 1.0), all_issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{
        AgentAction, EnvironmentState, Observation, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(steps: Vec<TrajectoryStep>, result: TaskResult) -> Trajectory {
        let total_reward = steps.iter().map(|s| s.reward).sum();
        Trajectory {
            id: Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "test-model".to_string(),
            scaffold_type: "basic".to_string(),
            steps,
            final_result: result,
            total_reward,
            created_at: Utc::now(),
            duration_seconds: 10,
            token_usage: TokenUsage::default(),
        }
    }

    fn create_step(step_number: u32, success: bool, output: &str) -> TrajectoryStep {
        TrajectoryStep {
            step_number,
            state: EnvironmentState::default(),
            action: AgentAction {
                tool_name: "test_tool".to_string(),
                tool_args: serde_json::json!({}),
                raw_llm_output: "Testing".to_string(),
                thinking: None,
            },
            observation: Observation {
                success,
                output: output.to_string(),
                error: if success {
                    None
                } else {
                    Some("Test failed".to_string())
                },
                state_changes: vec![],
            },
            reward: if success { 1.0 } else { -0.5 },
            done: false,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_check_task_result_success() {
        let checker = CorrectnessChecker::new(false);

        let result = TaskResult::Success { score: 1.0 };
        assert_eq!(checker.check_task_result(&result), 1.0);

        let result = TaskResult::Success { score: 0.5 };
        assert_eq!(checker.check_task_result(&result), 0.5);
    }

    #[test]
    fn test_check_task_result_failure() {
        let checker = CorrectnessChecker::new(false);

        let result = TaskResult::Failure {
            reason: "Test".to_string(),
        };
        assert_eq!(checker.check_task_result(&result), 0.0);

        let result = TaskResult::Timeout;
        assert_eq!(checker.check_task_result(&result), 0.0);
    }

    #[test]
    fn test_check_task_result_strict_mode() {
        let checker = CorrectnessChecker::new(true);

        let result = TaskResult::Success { score: 1.0 };
        assert_eq!(checker.check_task_result(&result), 1.0);

        // Partial success is penalized in strict mode
        let result = TaskResult::Success { score: 0.8 };
        assert!((checker.check_task_result(&result) - 0.64).abs() < f64::EPSILON);
    }

    #[test]
    fn test_check_test_output_full_match() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![create_step(0, true, "All tests passed successfully")];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let score = checker.check_test_output(&trajectory, "tests passed");
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_check_test_output_no_match() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![create_step(0, true, "Error occurred")];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let score = checker.check_test_output(&trajectory, "tests passed");
        assert!(score < 1.0);
    }

    #[test]
    fn test_check_test_output_empty_expected() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![create_step(0, true, "Any output")];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let score = checker.check_test_output(&trajectory, "");
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_evaluate_successful_trajectory() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![
            create_step(0, true, "Step 1 done"),
            create_step(1, true, "Step 2 done"),
            create_step(2, true, "Step 3 done"),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let (score, issues) = checker.evaluate(&trajectory);
        assert!(score > 0.8);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_evaluate_failed_trajectory() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![create_step(0, true, "Partial progress")];
        let trajectory = create_test_trajectory(
            steps,
            TaskResult::Failure {
                reason: "Could not complete".to_string(),
            },
        );

        let (score, issues) = checker.evaluate(&trajectory);
        assert!(score < 0.5);
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .any(|i| i.issue_type == QualityIssueType::IncorrectOutput));
    }

    #[test]
    fn test_evaluate_with_failed_steps() {
        let checker = CorrectnessChecker::new(false);
        let steps = vec![
            create_step(0, true, "OK"),
            create_step(1, false, ""),
            create_step(2, true, "OK"),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 0.8 });

        let (score, issues) = checker.evaluate(&trajectory);
        assert!(score > 0.0);
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .any(|i| i.issue_type == QualityIssueType::FailedTest));
    }

    #[test]
    fn test_evaluate_strict_mode() {
        let checker = CorrectnessChecker::new(true);
        let steps = vec![create_step(0, true, "OK"), create_step(1, false, "")];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 0.9 });

        let (score, _issues) = checker.evaluate(&trajectory);
        // Strict mode should give lower scores
        let non_strict = CorrectnessChecker::new(false);
        let (non_strict_score, _) = non_strict.evaluate(&trajectory);
        assert!(score < non_strict_score);
    }
}
