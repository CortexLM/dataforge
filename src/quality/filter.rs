//! Main quality filtering pipeline for trajectory validation.
//!
//! Provides multi-stage filtering: basic filtering, correctness, coherence, and completeness.

use uuid::Uuid;

use crate::trajectory::types::Trajectory;

use super::coherence::CoherenceAnalyzer;
use super::completeness::CompletenessChecker;
use super::correctness::CorrectnessChecker;

/// Default weight for correctness score in overall calculation.
const DEFAULT_CORRECTNESS_WEIGHT: f64 = 0.5;

/// Default weight for coherence score in overall calculation.
const DEFAULT_COHERENCE_WEIGHT: f64 = 0.3;

/// Default weight for completeness score in overall calculation.
const DEFAULT_COMPLETENESS_WEIGHT: f64 = 0.2;

/// Quality issue severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Critical issues that cause immediate failure.
    Critical,
    /// Major issues that significantly reduce score.
    Major,
    /// Minor issues that slightly reduce score.
    Minor,
    /// Warnings that are logged but don't affect score.
    Warning,
}

impl Severity {
    /// Returns the score penalty associated with this severity.
    pub fn penalty(&self) -> f64 {
        match self {
            Severity::Critical => 1.0,
            Severity::Major => 0.3,
            Severity::Minor => 0.1,
            Severity::Warning => 0.0,
        }
    }
}

/// Types of quality issues that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityIssueType {
    /// The output is incorrect.
    IncorrectOutput,
    /// A test failed during verification.
    FailedTest,
    /// An action doesn't logically follow from the previous state.
    IncoherentAction,
    /// A step is redundant or repeated unnecessarily.
    RedundantStep,
    /// A critical step is missing from the trajectory.
    MissingStep,
    /// The syntax of a command or code is invalid.
    InvalidSyntax,
    /// The trajectory timed out before completion.
    Timeout,
    /// The trajectory is empty or has no meaningful steps.
    EmptyTrajectory,
}

impl std::fmt::Display for QualityIssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            QualityIssueType::IncorrectOutput => "IncorrectOutput",
            QualityIssueType::FailedTest => "FailedTest",
            QualityIssueType::IncoherentAction => "IncoherentAction",
            QualityIssueType::RedundantStep => "RedundantStep",
            QualityIssueType::MissingStep => "MissingStep",
            QualityIssueType::InvalidSyntax => "InvalidSyntax",
            QualityIssueType::Timeout => "Timeout",
            QualityIssueType::EmptyTrajectory => "EmptyTrajectory",
        };
        write!(f, "{}", name)
    }
}

/// A quality issue detected in a trajectory.
#[derive(Debug, Clone)]
pub struct QualityIssue {
    /// The type of quality issue.
    pub issue_type: QualityIssueType,
    /// The severity of the issue.
    pub severity: Severity,
    /// A human-readable description of the issue.
    pub description: String,
    /// The step number where the issue was found, if applicable.
    pub step_number: Option<u32>,
}

impl QualityIssue {
    /// Creates a new quality issue.
    pub fn new(
        issue_type: QualityIssueType,
        severity: Severity,
        description: impl Into<String>,
    ) -> Self {
        Self {
            issue_type,
            severity,
            description: description.into(),
            step_number: None,
        }
    }

    /// Creates a new quality issue with a step number.
    pub fn with_step(
        issue_type: QualityIssueType,
        severity: Severity,
        description: impl Into<String>,
        step_number: u32,
    ) -> Self {
        Self {
            issue_type,
            severity,
            description: description.into(),
            step_number: Some(step_number),
        }
    }
}

/// The result of quality evaluation for a trajectory.
#[derive(Debug, Clone)]
pub struct QualityResult {
    /// The ID of the evaluated trajectory.
    pub trajectory_id: Uuid,
    /// Correctness score (0.0 - 1.0).
    pub correctness_score: f64,
    /// Coherence score (0.0 - 1.0).
    pub coherence_score: f64,
    /// Completeness score (0.0 - 1.0).
    pub completeness_score: f64,
    /// Overall weighted score (0.0 - 1.0).
    pub overall_score: f64,
    /// Whether the trajectory passed quality filtering.
    pub passed: bool,
    /// List of quality issues found.
    pub issues: Vec<QualityIssue>,
}

impl QualityResult {
    /// Creates a failing result with a critical issue.
    fn fail_with_issue(trajectory_id: Uuid, issue: QualityIssue) -> Self {
        Self {
            trajectory_id,
            correctness_score: 0.0,
            coherence_score: 0.0,
            completeness_score: 0.0,
            overall_score: 0.0,
            passed: false,
            issues: vec![issue],
        }
    }
}

/// The main quality filtering pipeline.
///
/// Evaluates trajectories based on correctness, coherence, and completeness,
/// producing an overall quality score and identifying specific issues.
pub struct QualityFilterPipeline {
    correctness: CorrectnessChecker,
    coherence: CoherenceAnalyzer,
    completeness: CompletenessChecker,
    min_overall_score: f64,
    correctness_weight: f64,
    coherence_weight: f64,
    completeness_weight: f64,
}

impl QualityFilterPipeline {
    /// Creates a new quality filter pipeline with the specified minimum score threshold.
    ///
    /// # Arguments
    ///
    /// * `min_overall_score` - Minimum overall score (0.0-1.0) required to pass filtering.
    pub fn new(min_overall_score: f64) -> Self {
        Self {
            correctness: CorrectnessChecker::new(false),
            coherence: CoherenceAnalyzer::new(),
            completeness: CompletenessChecker::new(1, 100),
            min_overall_score: min_overall_score.clamp(0.0, 1.0),
            correctness_weight: DEFAULT_CORRECTNESS_WEIGHT,
            coherence_weight: DEFAULT_COHERENCE_WEIGHT,
            completeness_weight: DEFAULT_COMPLETENESS_WEIGHT,
        }
    }

    /// Creates a builder for configuring the pipeline.
    pub fn builder() -> QualityFilterPipelineBuilder {
        QualityFilterPipelineBuilder::default()
    }

    /// Sets custom weights for the quality components.
    ///
    /// Weights are normalized so they sum to 1.0.
    pub fn with_weights(mut self, correctness: f64, coherence: f64, completeness: f64) -> Self {
        let total = correctness + coherence + completeness;
        if total > 0.0 {
            self.correctness_weight = correctness / total;
            self.coherence_weight = coherence / total;
            self.completeness_weight = completeness / total;
        }
        self
    }

    /// Performs basic filtering on a trajectory.
    ///
    /// Returns `Some(QualityIssue)` if the trajectory fails basic checks,
    /// or `None` if it passes.
    pub fn basic_filter(&self, trajectory: &Trajectory) -> Option<QualityIssue> {
        // Check for empty trajectory
        if trajectory.steps.is_empty() {
            return Some(QualityIssue::new(
                QualityIssueType::EmptyTrajectory,
                Severity::Critical,
                "Trajectory has no steps",
            ));
        }

        // Check for timeout
        if matches!(
            trajectory.final_result,
            crate::trajectory::types::TaskResult::Timeout
        ) {
            return Some(QualityIssue::new(
                QualityIssueType::Timeout,
                Severity::Critical,
                "Trajectory execution timed out",
            ));
        }

        // Check for errors
        if let crate::trajectory::types::TaskResult::Error { ref message } = trajectory.final_result
        {
            return Some(QualityIssue::new(
                QualityIssueType::IncorrectOutput,
                Severity::Critical,
                format!("Trajectory ended with error: {}", message),
            ));
        }

        None
    }

    /// Evaluates a trajectory and returns a quality result.
    ///
    /// This runs all quality checks (correctness, coherence, completeness)
    /// and produces an overall quality score.
    pub async fn evaluate(&self, trajectory: &Trajectory) -> QualityResult {
        // First, apply basic filtering
        if let Some(issue) = self.basic_filter(trajectory) {
            return QualityResult::fail_with_issue(trajectory.id, issue);
        }

        let mut all_issues = Vec::new();

        // Run correctness check
        let (correctness_score, correctness_issues) = self.correctness.evaluate(trajectory);
        all_issues.extend(correctness_issues);

        // Run coherence check
        let (coherence_score, coherence_issues) = self.coherence.evaluate(trajectory);
        all_issues.extend(coherence_issues);

        // Run completeness check
        let (completeness_score, completeness_issues) = self.completeness.evaluate(trajectory);
        all_issues.extend(completeness_issues);

        // Calculate weighted overall score
        let overall_score = self.correctness_weight * correctness_score
            + self.coherence_weight * coherence_score
            + self.completeness_weight * completeness_score;

        // Check for critical issues that should fail the trajectory
        let has_critical = all_issues.iter().any(|i| i.severity == Severity::Critical);
        let passed = !has_critical && overall_score >= self.min_overall_score;

        QualityResult {
            trajectory_id: trajectory.id,
            correctness_score,
            coherence_score,
            completeness_score,
            overall_score,
            passed,
            issues: all_issues,
        }
    }
}

/// Builder for configuring a QualityFilterPipeline.
#[derive(Default)]
pub struct QualityFilterPipelineBuilder {
    min_overall_score: Option<f64>,
    correctness_weight: Option<f64>,
    coherence_weight: Option<f64>,
    completeness_weight: Option<f64>,
    strict_mode: Option<bool>,
    redundancy_threshold: Option<f64>,
    min_steps: Option<u32>,
    max_steps: Option<u32>,
}

impl QualityFilterPipelineBuilder {
    /// Sets the minimum overall score required to pass filtering.
    pub fn min_score(mut self, score: f64) -> Self {
        self.min_overall_score = Some(score.clamp(0.0, 1.0));
        self
    }

    /// Sets the weight for the correctness component.
    pub fn correctness_weight(mut self, weight: f64) -> Self {
        self.correctness_weight = Some(weight.max(0.0));
        self
    }

    /// Sets the weight for the coherence component.
    pub fn coherence_weight(mut self, weight: f64) -> Self {
        self.coherence_weight = Some(weight.max(0.0));
        self
    }

    /// Sets the weight for the completeness component.
    pub fn completeness_weight(mut self, weight: f64) -> Self {
        self.completeness_weight = Some(weight.max(0.0));
        self
    }

    /// Enables strict mode for correctness checking.
    pub fn strict_mode(mut self, enabled: bool) -> Self {
        self.strict_mode = Some(enabled);
        self
    }

    /// Sets the redundancy threshold for coherence checking.
    pub fn redundancy_threshold(mut self, threshold: f64) -> Self {
        self.redundancy_threshold = Some(threshold.clamp(0.0, 1.0));
        self
    }

    /// Sets the minimum number of steps for completeness checking.
    pub fn min_steps(mut self, steps: u32) -> Self {
        self.min_steps = Some(steps);
        self
    }

    /// Sets the maximum number of steps for completeness checking.
    pub fn max_steps(mut self, steps: u32) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Builds the QualityFilterPipeline.
    pub fn build(self) -> QualityFilterPipeline {
        let min_score = self.min_overall_score.unwrap_or(0.7);
        let correctness_weight = self
            .correctness_weight
            .unwrap_or(DEFAULT_CORRECTNESS_WEIGHT);
        let coherence_weight = self.coherence_weight.unwrap_or(DEFAULT_COHERENCE_WEIGHT);
        let completeness_weight = self
            .completeness_weight
            .unwrap_or(DEFAULT_COMPLETENESS_WEIGHT);

        // Normalize weights
        let total = correctness_weight + coherence_weight + completeness_weight;
        let (cw, chw, cmw) = if total > 0.0 {
            (
                correctness_weight / total,
                coherence_weight / total,
                completeness_weight / total,
            )
        } else {
            (
                DEFAULT_CORRECTNESS_WEIGHT,
                DEFAULT_COHERENCE_WEIGHT,
                DEFAULT_COMPLETENESS_WEIGHT,
            )
        };

        let strict = self.strict_mode.unwrap_or(false);
        let redundancy = self.redundancy_threshold.unwrap_or(0.8);
        let min_steps = self.min_steps.unwrap_or(1);
        let max_steps = self.max_steps.unwrap_or(100);

        QualityFilterPipeline {
            correctness: CorrectnessChecker::new(strict),
            coherence: CoherenceAnalyzer::with_redundancy_threshold(redundancy),
            completeness: CompletenessChecker::new(min_steps, max_steps),
            min_overall_score: min_score,
            correctness_weight: cw,
            coherence_weight: chw,
            completeness_weight: cmw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;

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

    fn create_test_step(step_number: u32, tool_name: &str, success: bool) -> TrajectoryStep {
        TrajectoryStep {
            step_number,
            state: EnvironmentState::default(),
            action: AgentAction {
                tool_name: tool_name.to_string(),
                tool_args: serde_json::json!({}),
                raw_llm_output: format!("Executing {}", tool_name),
                thinking: Some("Thinking about the action".to_string()),
            },
            observation: Observation {
                success,
                output: if success {
                    "Success".to_string()
                } else {
                    String::new()
                },
                error: if success {
                    None
                } else {
                    Some("Error occurred".to_string())
                },
                state_changes: vec![],
            },
            reward: if success { 0.1 } else { -0.1 },
            done: false,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_severity_penalty() {
        assert_eq!(Severity::Critical.penalty(), 1.0);
        assert_eq!(Severity::Major.penalty(), 0.3);
        assert_eq!(Severity::Minor.penalty(), 0.1);
        assert_eq!(Severity::Warning.penalty(), 0.0);
    }

    #[test]
    fn test_quality_issue_type_display() {
        assert_eq!(
            format!("{}", QualityIssueType::IncorrectOutput),
            "IncorrectOutput"
        );
        assert_eq!(format!("{}", QualityIssueType::Timeout), "Timeout");
    }

    #[test]
    fn test_basic_filter_empty_trajectory() {
        let pipeline = QualityFilterPipeline::new(0.7);
        let trajectory = create_test_trajectory(vec![], TaskResult::Success { score: 1.0 });

        let result = pipeline.basic_filter(&trajectory);
        assert!(result.is_some());
        let issue = result.unwrap();
        assert_eq!(issue.issue_type, QualityIssueType::EmptyTrajectory);
        assert_eq!(issue.severity, Severity::Critical);
    }

    #[test]
    fn test_basic_filter_timeout() {
        let pipeline = QualityFilterPipeline::new(0.7);
        let steps = vec![create_test_step(0, "test_tool", true)];
        let trajectory = create_test_trajectory(steps, TaskResult::Timeout);

        let result = pipeline.basic_filter(&trajectory);
        assert!(result.is_some());
        let issue = result.unwrap();
        assert_eq!(issue.issue_type, QualityIssueType::Timeout);
    }

    #[test]
    fn test_basic_filter_error() {
        let pipeline = QualityFilterPipeline::new(0.7);
        let steps = vec![create_test_step(0, "test_tool", true)];
        let trajectory = create_test_trajectory(
            steps,
            TaskResult::Error {
                message: "Test error".to_string(),
            },
        );

        let result = pipeline.basic_filter(&trajectory);
        assert!(result.is_some());
        let issue = result.unwrap();
        assert_eq!(issue.issue_type, QualityIssueType::IncorrectOutput);
    }

    #[test]
    fn test_basic_filter_pass() {
        let pipeline = QualityFilterPipeline::new(0.7);
        let steps = vec![create_test_step(0, "test_tool", true)];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let result = pipeline.basic_filter(&trajectory);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_evaluate_success() {
        let pipeline = QualityFilterPipeline::new(0.5);
        let steps = vec![
            create_test_step(0, "read_file", true),
            create_test_step(1, "edit_file", true),
            create_test_step(2, "run_tests", true),
        ];
        let trajectory = create_test_trajectory(steps, TaskResult::Success { score: 1.0 });

        let result = pipeline.evaluate(&trajectory).await;
        assert!(result.overall_score > 0.0);
    }

    #[tokio::test]
    async fn test_evaluate_empty_trajectory() {
        let pipeline = QualityFilterPipeline::new(0.7);
        let trajectory = create_test_trajectory(vec![], TaskResult::Success { score: 1.0 });

        let result = pipeline.evaluate(&trajectory).await;
        assert!(!result.passed);
        assert_eq!(result.overall_score, 0.0);
        assert!(!result.issues.is_empty());
        assert_eq!(
            result.issues[0].issue_type,
            QualityIssueType::EmptyTrajectory
        );
    }

    #[test]
    fn test_builder() {
        let pipeline = QualityFilterPipeline::builder()
            .min_score(0.8)
            .correctness_weight(0.6)
            .coherence_weight(0.2)
            .completeness_weight(0.2)
            .strict_mode(true)
            .min_steps(2)
            .max_steps(50)
            .build();

        assert!((pipeline.min_overall_score - 0.8).abs() < f64::EPSILON);
        assert!((pipeline.correctness_weight - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_weight_normalization() {
        let pipeline = QualityFilterPipeline::new(0.7).with_weights(1.0, 1.0, 1.0);

        let total =
            pipeline.correctness_weight + pipeline.coherence_weight + pipeline.completeness_weight;
        assert!((total - 1.0).abs() < f64::EPSILON);
    }
}
