//! Coherence analysis for trajectories.
//!
//! Analyzes whether actions form a logical sequence toward the goal,
//! detects redundant or looping behavior, and verifies action-observation consistency.

use std::collections::HashMap;

use crate::trajectory::types::Trajectory;

use super::filter::{QualityIssue, QualityIssueType, Severity};

/// Default threshold for detecting redundant action sequences.
const DEFAULT_REDUNDANCY_THRESHOLD: f64 = 0.8;

/// Maximum number of consecutive similar actions before flagging as redundant.
const MAX_CONSECUTIVE_SIMILAR_ACTIONS: usize = 3;

/// Maximum repetitions of an action pattern before flagging as a loop.
const MAX_PATTERN_REPETITIONS: usize = 2;

/// Analyzes the coherence of trajectory action sequences.
///
/// Detects issues like redundant steps, infinite loops, and
/// actions that don't logically follow from observations.
pub struct CoherenceAnalyzer {
    /// Threshold for considering actions as redundant (similarity score).
    redundancy_threshold: f64,
}

impl Default for CoherenceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CoherenceAnalyzer {
    /// Creates a new coherence analyzer with default settings.
    pub fn new() -> Self {
        Self {
            redundancy_threshold: DEFAULT_REDUNDANCY_THRESHOLD,
        }
    }

    /// Creates a coherence analyzer with a custom redundancy threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Similarity threshold (0.0-1.0) above which actions are considered redundant.
    pub fn with_redundancy_threshold(threshold: f64) -> Self {
        Self {
            redundancy_threshold: threshold.clamp(0.0, 1.0),
        }
    }

    /// Checks if actions follow logically from observations.
    ///
    /// Analyzes each step to see if the action makes sense given
    /// the previous observation and state.
    ///
    /// Returns a score from 0.0 (completely incoherent) to 1.0 (fully coherent).
    pub fn check_action_logic(&self, trajectory: &Trajectory) -> f64 {
        if trajectory.steps.is_empty() {
            return 0.0;
        }

        if trajectory.steps.len() == 1 {
            return 1.0;
        }

        let mut coherent_transitions = 0;
        let total_transitions = trajectory.steps.len() - 1;

        for i in 0..total_transitions {
            let current_step = &trajectory.steps[i];
            let next_step = &trajectory.steps[i + 1];

            // Check if the next action makes sense given the current observation
            if self.is_transition_coherent(current_step, next_step) {
                coherent_transitions += 1;
            }
        }

        coherent_transitions as f64 / total_transitions as f64
    }

    /// Checks if a transition between two steps is coherent.
    fn is_transition_coherent(
        &self,
        current: &crate::trajectory::types::TrajectoryStep,
        next: &crate::trajectory::types::TrajectoryStep,
    ) -> bool {
        // If the current step failed, a different action is expected
        if !current.observation.success {
            // Good: trying something different after failure
            return current.action.tool_name != next.action.tool_name
                || current.action.tool_args != next.action.tool_args;
        }

        // If thinking is present, it should relate to the action
        if let Some(ref thinking) = next.action.thinking {
            if thinking.is_empty() {
                return false;
            }
        }

        // Check for obvious anti-patterns: repeated exact same action after success
        if current.observation.success
            && current.action.tool_name == next.action.tool_name
            && current.action.tool_args == next.action.tool_args
        {
            return false;
        }

        true
    }

    /// Detects redundant or repeated actions in the trajectory.
    ///
    /// Returns a score from 0.0 (highly redundant) to 1.0 (no redundancy)
    /// along with a list of issues found.
    pub fn detect_redundancy(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        if trajectory.steps.len() < 2 {
            return (1.0, vec![]);
        }

        let mut issues = Vec::new();
        let mut consecutive_similar = 0;
        let mut redundant_count = 0;

        // Count consecutive similar actions
        for window in trajectory.steps.windows(2) {
            let similarity = self.calculate_action_similarity(&window[0].action, &window[1].action);

            if similarity >= self.redundancy_threshold {
                consecutive_similar += 1;

                if consecutive_similar >= MAX_CONSECUTIVE_SIMILAR_ACTIONS {
                    redundant_count += 1;
                    issues.push(QualityIssue::with_step(
                        QualityIssueType::RedundantStep,
                        Severity::Minor,
                        format!(
                            "Detected {} consecutive similar '{}' actions",
                            consecutive_similar + 1,
                            window[1].action.tool_name
                        ),
                        window[1].step_number,
                    ));
                }
            } else {
                consecutive_similar = 0;
            }
        }

        // Also count exact duplicate actions
        let mut action_counts: HashMap<String, usize> = HashMap::new();
        for step in &trajectory.steps {
            let key = format!(
                "{}:{}",
                step.action.tool_name,
                serde_json::to_string(&step.action.tool_args).unwrap_or_default()
            );
            *action_counts.entry(key).or_insert(0) += 1;
        }

        for (action, count) in action_counts {
            if count > 3 {
                redundant_count += count - 3;
                issues.push(QualityIssue::new(
                    QualityIssueType::RedundantStep,
                    Severity::Minor,
                    format!(
                        "Action '{}' repeated {} times",
                        action.split(':').next().unwrap_or(&action),
                        count
                    ),
                ));
            }
        }

        // Calculate score: penalize based on redundancy ratio
        let max_redundant = trajectory.steps.len() / 2;
        let redundancy_ratio = if max_redundant > 0 {
            (redundant_count as f64 / max_redundant as f64).min(1.0)
        } else {
            0.0
        };

        let score = 1.0 - (redundancy_ratio * 0.5);
        (score.max(0.0), issues)
    }

    /// Calculates similarity between two actions.
    fn calculate_action_similarity(
        &self,
        action1: &crate::trajectory::types::AgentAction,
        action2: &crate::trajectory::types::AgentAction,
    ) -> f64 {
        // Same tool name gives base similarity
        if action1.tool_name != action2.tool_name {
            return 0.0;
        }

        // Compare arguments
        let args_match = action1.tool_args == action2.tool_args;
        if args_match {
            return 1.0;
        }

        // Partial match based on argument structure
        0.5
    }

    /// Detects loops or infinite patterns in the trajectory.
    ///
    /// Returns a list of issues representing detected loops.
    pub fn detect_loops(&self, trajectory: &Trajectory) -> Vec<QualityIssue> {
        if trajectory.steps.len() < 4 {
            return vec![];
        }

        let mut issues = Vec::new();

        // Extract action sequence for pattern detection
        let actions: Vec<&str> = trajectory
            .steps
            .iter()
            .map(|s| s.action.tool_name.as_str())
            .collect();

        // Detect repeating patterns of length 2-4
        for pattern_len in 2..=4.min(actions.len() / 2) {
            if let Some(loop_issue) = self.detect_pattern_loop(&actions, pattern_len) {
                issues.push(loop_issue);
            }
        }

        // Detect back-and-forth patterns (A-B-A-B)
        issues.extend(self.detect_alternating_pattern(trajectory));

        issues
    }

    /// Detects a repeating pattern of a specific length.
    fn detect_pattern_loop(&self, actions: &[&str], pattern_len: usize) -> Option<QualityIssue> {
        if actions.len() < pattern_len * 2 {
            return None;
        }

        for start in 0..=actions.len() - pattern_len * 2 {
            let pattern = &actions[start..start + pattern_len];
            let mut repetitions = 1;

            let mut pos = start + pattern_len;
            while pos + pattern_len <= actions.len() {
                let next_segment = &actions[pos..pos + pattern_len];
                if pattern == next_segment {
                    repetitions += 1;
                    pos += pattern_len;
                } else {
                    break;
                }
            }

            if repetitions > MAX_PATTERN_REPETITIONS {
                return Some(QualityIssue::new(
                    QualityIssueType::IncoherentAction,
                    Severity::Major,
                    format!(
                        "Detected loop pattern {:?} repeated {} times starting at step {}",
                        pattern, repetitions, start
                    ),
                ));
            }
        }

        None
    }

    /// Detects alternating patterns like A-B-A-B.
    fn detect_alternating_pattern(&self, trajectory: &Trajectory) -> Vec<QualityIssue> {
        let mut issues = Vec::new();

        if trajectory.steps.len() < 4 {
            return issues;
        }

        let mut alternating_count = 0;
        let mut alt_start = 0;

        for i in 2..trajectory.steps.len() {
            let two_back = &trajectory.steps[i - 2].action;
            let current = &trajectory.steps[i].action;

            if two_back.tool_name == current.tool_name && two_back.tool_args == current.tool_args {
                if alternating_count == 0 {
                    alt_start = i - 2;
                }
                alternating_count += 1;
            } else {
                if alternating_count >= 2 {
                    issues.push(QualityIssue::with_step(
                        QualityIssueType::IncoherentAction,
                        Severity::Minor,
                        format!(
                            "Detected alternating pattern from step {} to {}",
                            alt_start,
                            i - 1
                        ),
                        alt_start as u32,
                    ));
                }
                alternating_count = 0;
            }
        }

        // Check for trailing alternating pattern
        if alternating_count >= 2 {
            issues.push(QualityIssue::with_step(
                QualityIssueType::IncoherentAction,
                Severity::Minor,
                format!(
                    "Detected alternating pattern from step {} to end",
                    alt_start
                ),
                alt_start as u32,
            ));
        }

        issues
    }

    /// Evaluates the overall coherence of a trajectory.
    ///
    /// Returns a tuple of (score, issues) where score is 0.0-1.0.
    pub fn evaluate(&self, trajectory: &Trajectory) -> (f64, Vec<QualityIssue>) {
        if trajectory.steps.is_empty() {
            return (0.0, vec![]);
        }

        let mut all_issues = Vec::new();

        // Weight factors for different coherence checks
        const ACTION_LOGIC_WEIGHT: f64 = 0.4;
        const REDUNDANCY_WEIGHT: f64 = 0.35;
        const LOOP_PENALTY_WEIGHT: f64 = 0.25;

        // Check action logic
        let action_logic_score = self.check_action_logic(trajectory);

        // Check redundancy
        let (redundancy_score, redundancy_issues) = self.detect_redundancy(trajectory);
        all_issues.extend(redundancy_issues);

        // Check for loops
        let loop_issues = self.detect_loops(trajectory);
        let loop_penalty = if loop_issues.is_empty() {
            1.0
        } else {
            // Each loop reduces the score
            let major_loops = loop_issues
                .iter()
                .filter(|i| i.severity == Severity::Major)
                .count();
            let minor_loops = loop_issues
                .iter()
                .filter(|i| i.severity == Severity::Minor)
                .count();
            (1.0 - (major_loops as f64 * 0.3 + minor_loops as f64 * 0.1)).max(0.0)
        };
        all_issues.extend(loop_issues);

        // Calculate weighted average
        let overall_score = ACTION_LOGIC_WEIGHT * action_logic_score
            + REDUNDANCY_WEIGHT * redundancy_score
            + LOOP_PENALTY_WEIGHT * loop_penalty;

        (overall_score.clamp(0.0, 1.0), all_issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(steps: Vec<TrajectoryStep>) -> Trajectory {
        Trajectory {
            id: Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "test-model".to_string(),
            scaffold_type: "basic".to_string(),
            steps,
            final_result: TaskResult::Success { score: 1.0 },
            total_reward: 1.0,
            created_at: Utc::now(),
            duration_seconds: 10,
            token_usage: TokenUsage::default(),
        }
    }

    fn create_step(step_number: u32, tool_name: &str, success: bool) -> TrajectoryStep {
        create_step_with_args(step_number, tool_name, serde_json::json!({}), success)
    }

    fn create_step_with_args(
        step_number: u32,
        tool_name: &str,
        args: serde_json::Value,
        success: bool,
    ) -> TrajectoryStep {
        TrajectoryStep {
            step_number,
            state: EnvironmentState::default(),
            action: AgentAction {
                tool_name: tool_name.to_string(),
                tool_args: args,
                raw_llm_output: format!("Executing {}", tool_name),
                thinking: Some("Reasoning about this action".to_string()),
            },
            observation: Observation {
                success,
                output: "Output".to_string(),
                error: if success {
                    None
                } else {
                    Some("Error".to_string())
                },
                state_changes: vec![],
            },
            reward: if success { 0.1 } else { -0.1 },
            done: false,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_check_action_logic_empty() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![]);
        assert_eq!(analyzer.check_action_logic(&trajectory), 0.0);
    }

    #[test]
    fn test_check_action_logic_single_step() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![create_step(0, "read_file", true)]);
        assert_eq!(analyzer.check_action_logic(&trajectory), 1.0);
    }

    #[test]
    fn test_check_action_logic_coherent_sequence() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "edit_file", true),
            create_step(2, "run_tests", true),
        ]);
        // Different actions after success should be coherent
        assert!(analyzer.check_action_logic(&trajectory) > 0.5);
    }

    #[test]
    fn test_check_action_logic_incoherent_repeat() {
        let analyzer = CoherenceAnalyzer::new();
        // Same action repeated after success is incoherent
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "read_file", true),
        ]);
        assert!(analyzer.check_action_logic(&trajectory) < 1.0);
    }

    #[test]
    fn test_detect_redundancy_no_redundancy() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "edit_file", true),
            create_step(2, "run_tests", true),
        ]);
        let (score, issues) = analyzer.detect_redundancy(&trajectory);
        assert!(score > 0.9);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_detect_redundancy_with_repetition() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "read_file", true),
            create_step(2, "read_file", true),
            create_step(3, "read_file", true),
            create_step(4, "read_file", true),
        ]);
        let (score, issues) = analyzer.detect_redundancy(&trajectory);
        assert!(score < 1.0);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_detect_loops_no_loops() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "edit_file", true),
            create_step(2, "run_tests", true),
            create_step(3, "commit", true),
        ]);
        let issues = analyzer.detect_loops(&trajectory);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_detect_loops_with_pattern() {
        let analyzer = CoherenceAnalyzer::new();
        // Pattern: read -> edit -> read -> edit -> read -> edit
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "edit_file", true),
            create_step(2, "read_file", true),
            create_step(3, "edit_file", true),
            create_step(4, "read_file", true),
            create_step(5, "edit_file", true),
        ]);
        let issues = analyzer.detect_loops(&trajectory);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_with_redundancy_threshold() {
        let analyzer = CoherenceAnalyzer::with_redundancy_threshold(0.9);
        assert!((analyzer.redundancy_threshold - 0.9).abs() < f64::EPSILON);

        // Threshold should be clamped
        let analyzer = CoherenceAnalyzer::with_redundancy_threshold(1.5);
        assert!((analyzer.redundancy_threshold - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_evaluate_empty_trajectory() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![]);
        let (score, issues) = analyzer.evaluate(&trajectory);
        assert_eq!(score, 0.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_evaluate_good_trajectory() {
        let analyzer = CoherenceAnalyzer::new();
        let trajectory = create_test_trajectory(vec![
            create_step(0, "read_file", true),
            create_step(1, "analyze", true),
            create_step(2, "edit_file", true),
            create_step(3, "run_tests", true),
        ]);
        let (score, issues) = analyzer.evaluate(&trajectory);
        assert!(score > 0.7);
        assert!(issues.is_empty() || issues.iter().all(|i| i.severity != Severity::Critical));
    }

    #[test]
    fn test_calculate_action_similarity_same() {
        let analyzer = CoherenceAnalyzer::new();
        let action = AgentAction {
            tool_name: "read_file".to_string(),
            tool_args: serde_json::json!({"path": "test.txt"}),
            raw_llm_output: "Reading file".to_string(),
            thinking: None,
        };
        let similarity = analyzer.calculate_action_similarity(&action, &action);
        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn test_calculate_action_similarity_different_tools() {
        let analyzer = CoherenceAnalyzer::new();
        let action1 = AgentAction {
            tool_name: "read_file".to_string(),
            tool_args: serde_json::json!({}),
            raw_llm_output: "".to_string(),
            thinking: None,
        };
        let action2 = AgentAction {
            tool_name: "write_file".to_string(),
            tool_args: serde_json::json!({}),
            raw_llm_output: "".to_string(),
            thinking: None,
        };
        let similarity = analyzer.calculate_action_similarity(&action1, &action2);
        assert_eq!(similarity, 0.0);
    }
}
