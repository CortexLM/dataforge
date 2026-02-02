//! Core types for the multi-agent validation system.
//!
//! Defines status enums, message types, validation scores, and report structures
//! used throughout the agent-based task validation pipeline.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::difficulty::DifficultyLevel;
use crate::generator::GeneratedInstance;

/// Status of an agent in the validation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is waiting to be executed.
    Pending,
    /// Agent is currently running.
    Running,
    /// Agent completed successfully.
    Completed,
    /// Agent failed during execution.
    Failed,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Pending => write!(f, "pending"),
            AgentStatus::Running => write!(f, "running"),
            AgentStatus::Completed => write!(f, "completed"),
            AgentStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Message type for inter-agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique identifier for this message.
    pub id: String,
    /// Name of the sending agent.
    pub from_agent: String,
    /// Name of the receiving agent (or "broadcast" for all).
    pub to_agent: String,
    /// Message type/category.
    pub message_type: MessageType,
    /// Message payload as JSON.
    pub payload: serde_json::Value,
    /// Timestamp when the message was created.
    pub timestamp: DateTime<Utc>,
}

impl AgentMessage {
    /// Creates a new agent message.
    pub fn new(
        from_agent: impl Into<String>,
        to_agent: impl Into<String>,
        message_type: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from_agent: from_agent.into(),
            to_agent: to_agent.into(),
            message_type,
            payload,
            timestamp: Utc::now(),
        }
    }

    /// Creates a broadcast message to all agents.
    pub fn broadcast(
        from_agent: impl Into<String>,
        message_type: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(from_agent, "broadcast", message_type, payload)
    }
}

/// Types of messages that can be sent between agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Task generation request.
    GenerationRequest,
    /// Task generation completed.
    GenerationComplete,
    /// Validation request.
    ValidationRequest,
    /// Validation completed.
    ValidationComplete,
    /// Status update.
    StatusUpdate,
    /// Error notification.
    Error,
    /// Pipeline control message.
    PipelineControl,
}

/// Score from a validation agent with reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationScore {
    /// Numeric score from 0.0 to 1.0.
    pub score: f64,
    /// Reasoning explaining the score.
    pub reasoning: String,
    /// List of specific issues found.
    pub issues: Vec<String>,
}

impl ValidationScore {
    /// Creates a new validation score.
    pub fn new(score: f64, reasoning: impl Into<String>) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            issues: Vec::new(),
        }
    }

    /// Creates a validation score with issues.
    pub fn with_issues(score: f64, reasoning: impl Into<String>, issues: Vec<String>) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            issues,
        }
    }

    /// Returns true if the score passes a threshold.
    pub fn passes_threshold(&self, threshold: f64) -> bool {
        self.score >= threshold
    }

    /// Returns true if there are no issues.
    pub fn has_no_issues(&self) -> bool {
        self.issues.is_empty()
    }
}

impl Default for ValidationScore {
    fn default() -> Self {
        Self {
            score: 0.0,
            reasoning: String::new(),
            issues: Vec::new(),
        }
    }
}

/// Result from a single validation agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationResult {
    /// Validation succeeded.
    Success {
        /// Summary message.
        message: String,
        /// Optional detailed information.
        details: Option<String>,
        /// Optional numeric score (0.0 to 1.0).
        score: Option<f64>,
        /// Name of the agent that performed validation.
        agent_name: String,
        /// Timestamp when validation completed.
        timestamp: DateTime<Utc>,
    },
    /// Validation failed.
    Failure {
        /// Summary message.
        message: String,
        /// Optional detailed information.
        details: Option<String>,
        /// Name of the agent that performed validation.
        agent_name: String,
        /// Timestamp when validation completed.
        timestamp: DateTime<Utc>,
    },
}

impl ValidationResult {
    /// Creates a successful validation result.
    pub fn success(message: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self::Success {
            message: message.into(),
            details: None,
            score: None,
            agent_name: agent_name.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a successful validation result with a score.
    pub fn success_with_score(message: impl Into<String>, score: f64) -> Self {
        Self::Success {
            message: message.into(),
            details: None,
            score: Some(score),
            agent_name: String::new(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a successful validation result with details and score.
    pub fn success_full(
        message: impl Into<String>,
        details: impl Into<String>,
        score: f64,
        agent_name: impl Into<String>,
    ) -> Self {
        Self::Success {
            message: message.into(),
            details: Some(details.into()),
            score: Some(score),
            agent_name: agent_name.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a failed validation result.
    pub fn failure(message: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self::Failure {
            message: message.into(),
            details: None,
            agent_name: agent_name.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a failed validation result with details.
    pub fn failure_with_details(
        message: impl Into<String>,
        details: impl Into<String>,
        agent_name: impl Into<String>,
    ) -> Self {
        Self::Failure {
            message: message.into(),
            details: Some(details.into()),
            agent_name: agent_name.into(),
            timestamp: Utc::now(),
        }
    }

    /// Returns true if this is a successful result.
    pub fn is_success(&self) -> bool {
        matches!(self, ValidationResult::Success { .. })
    }

    /// Returns the summary message.
    pub fn summary(&self) -> &str {
        match self {
            ValidationResult::Success { message, .. } => message,
            ValidationResult::Failure { message, .. } => message,
        }
    }

    /// Returns the agent name.
    pub fn agent_name(&self) -> &str {
        match self {
            ValidationResult::Success { agent_name, .. } => agent_name,
            ValidationResult::Failure { agent_name, .. } => agent_name,
        }
    }

    /// Returns the score if available.
    pub fn score(&self) -> Option<f64> {
        match self {
            ValidationResult::Success { score, .. } => *score,
            ValidationResult::Failure { .. } => None,
        }
    }

    /// Returns the details if available.
    pub fn details(&self) -> Option<&str> {
        match self {
            ValidationResult::Success { details, .. } => details.as_deref(),
            ValidationResult::Failure { details, .. } => details.as_deref(),
        }
    }

    /// Returns the timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            ValidationResult::Success { timestamp, .. } => *timestamp,
            ValidationResult::Failure { timestamp, .. } => *timestamp,
        }
    }

    /// Creates from ValidationScore (for compatibility with internal agents).
    pub fn from_score(passed: bool, score: ValidationScore, agent_name: impl Into<String>) -> Self {
        if passed {
            Self::Success {
                message: score.reasoning.clone(),
                details: if score.issues.is_empty() {
                    None
                } else {
                    Some(score.issues.join("; "))
                },
                score: Some(score.score),
                agent_name: agent_name.into(),
                timestamp: Utc::now(),
            }
        } else {
            Self::Failure {
                message: score.reasoning.clone(),
                details: if score.issues.is_empty() {
                    None
                } else {
                    Some(score.issues.join("; "))
                },
                agent_name: agent_name.into(),
                timestamp: Utc::now(),
            }
        }
    }
}

/// Complete validation report aggregating all agent results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskValidationReport {
    /// Unique identifier for this report.
    pub report_id: String,
    /// The task ID being validated.
    pub task_id: String,
    /// The generated task being validated.
    pub task: GeneratedTask,
    /// Individual validation results by agent name.
    pub validations: HashMap<String, ValidationResult>,
    /// Overall pass/fail status.
    pub overall_passed: bool,
    /// Whether the task is approved (alias for overall_passed).
    pub approved: bool,
    /// Aggregated overall score.
    pub overall_score: f64,
    /// Summary of the validation pipeline.
    pub summary: String,
    /// Timestamp when the report was generated.
    pub generated_at: DateTime<Utc>,
    /// Duration of the validation pipeline in milliseconds.
    pub duration_ms: u64,
}

impl TaskValidationReport {
    /// Creates a new empty validation report.
    pub fn new(task: GeneratedTask) -> Self {
        let task_id = task.task_id.clone();
        Self {
            report_id: uuid::Uuid::new_v4().to_string(),
            task_id,
            task,
            validations: HashMap::new(),
            overall_passed: false,
            approved: false,
            overall_score: 0.0,
            summary: String::new(),
            generated_at: Utc::now(),
            duration_ms: 0,
        }
    }

    /// Adds a validation result to the report.
    pub fn add_validation(&mut self, result: ValidationResult) {
        let agent_name = result.agent_name().to_string();
        self.validations.insert(agent_name, result);
    }

    /// Calculates and sets the overall score and pass status.
    pub fn finalize(&mut self, summary: impl Into<String>, duration_ms: u64) {
        self.summary = summary.into();
        self.duration_ms = duration_ms;
        self.generated_at = Utc::now();

        if self.validations.is_empty() {
            self.overall_passed = false;
            self.approved = false;
            self.overall_score = 0.0;
            return;
        }

        // Calculate average score from successful validations
        let scores: Vec<f64> = self
            .validations
            .values()
            .filter_map(|v| v.score())
            .collect();

        self.overall_score = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };

        // All validations must pass for overall to pass
        self.overall_passed = self.validations.values().all(|v| v.is_success());
        self.approved = self.overall_passed;
    }

    /// Returns the validation result for a specific agent.
    pub fn get_validation(&self, agent_name: &str) -> Option<&ValidationResult> {
        self.validations.get(agent_name)
    }

    /// Returns all issues from all validations.
    pub fn all_issues(&self) -> Vec<String> {
        self.validations
            .values()
            .filter_map(|v| v.details().map(|d| d.to_string()))
            .collect()
    }
}

/// Generated task with metadata for validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTask {
    /// Unique task identifier.
    pub task_id: String,
    /// Template used to generate the task.
    pub template_id: String,
    /// Seed used for generation.
    pub seed: u64,
    /// Target difficulty level.
    pub difficulty: DifficultyLevel,
    /// Rendered instruction text.
    pub instruction: String,
    /// Category of the task.
    pub category: String,
    /// Subcategory of the task.
    pub subcategory: String,
    /// Parameters used to generate the task.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Path to the generated task directory.
    pub task_dir: Option<String>,
    /// Timestamp when the task was generated.
    pub generated_at: DateTime<Utc>,
}

impl GeneratedTask {
    /// Creates a new generated task from an instance.
    pub fn from_instance(
        instance: &GeneratedInstance,
        template_id: impl Into<String>,
        difficulty: DifficultyLevel,
        instruction: impl Into<String>,
        category: impl Into<String>,
        subcategory: impl Into<String>,
    ) -> Self {
        Self {
            task_id: instance.task_id.clone(),
            template_id: template_id.into(),
            seed: instance
                .task_id
                .rsplit('-')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            difficulty,
            instruction: instruction.into(),
            category: category.into(),
            subcategory: subcategory.into(),
            parameters: instance.parameters.clone(),
            task_dir: Some(instance.task_dir.to_string_lossy().to_string()),
            generated_at: Utc::now(),
        }
    }

    /// Creates a minimal generated task for testing.
    pub fn minimal(
        task_id: impl Into<String>,
        template_id: impl Into<String>,
        difficulty: DifficultyLevel,
        instruction: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            template_id: template_id.into(),
            seed: 0,
            difficulty,
            instruction: instruction.into(),
            category: String::new(),
            subcategory: String::new(),
            parameters: HashMap::new(),
            task_dir: None,
            generated_at: Utc::now(),
        }
    }
}

/// Stages in the validation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    /// Task generation stage.
    TaskGeneration,
    /// Difficulty validation stage.
    DifficultyValidation,
    /// Feasibility validation stage.
    FeasibilityValidation,
    /// Final approval stage.
    FinalApproval,

    // Synthetic task generation stages
    /// Task ideation with high temperature LLM.
    SyntheticIdeation,
    /// Validating task complexity and memorization risk.
    SyntheticValidation,
    /// Creating full task with hidden solution.
    SyntheticExecution,
    /// Quality check for synthetic task.
    SyntheticQualityCheck,
}

impl PipelineStage {
    /// Returns all standard validation pipeline stages in order.
    pub fn all_stages() -> Vec<PipelineStage> {
        vec![
            PipelineStage::TaskGeneration,
            PipelineStage::DifficultyValidation,
            PipelineStage::FeasibilityValidation,
            PipelineStage::FinalApproval,
        ]
    }

    /// Returns all synthetic pipeline stages in order.
    pub fn all_synthetic_stages() -> Vec<PipelineStage> {
        vec![
            PipelineStage::SyntheticIdeation,
            PipelineStage::SyntheticValidation,
            PipelineStage::SyntheticExecution,
            PipelineStage::SyntheticQualityCheck,
        ]
    }

    /// Returns the display name for this stage.
    pub fn display_name(&self) -> &'static str {
        match self {
            PipelineStage::TaskGeneration => "Task Generation",
            PipelineStage::DifficultyValidation => "Difficulty Validation",
            PipelineStage::FeasibilityValidation => "Feasibility Validation",
            PipelineStage::FinalApproval => "Final Approval",
            PipelineStage::SyntheticIdeation => "Task Ideation",
            PipelineStage::SyntheticValidation => "Complexity Validation",
            PipelineStage::SyntheticExecution => "Task Execution",
            PipelineStage::SyntheticQualityCheck => "Quality Check",
        }
    }
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Events emitted during the validation pipeline for TUI updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum PipelineEvent {
    /// A pipeline stage has started.
    StageStarted {
        /// The stage that started.
        stage: PipelineStage,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A pipeline stage has completed successfully.
    StageCompleted {
        /// The stage that completed.
        stage: PipelineStage,
        /// Result of the stage.
        result: ValidationResult,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A pipeline stage has failed.
    StageFailed {
        /// The stage that failed.
        stage: PipelineStage,
        /// Error message.
        error: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// Agent reasoning during a stage.
    AgentReasoning {
        /// The stage the agent is working on.
        stage: PipelineStage,
        /// Reasoning text from the agent.
        reasoning: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// The entire pipeline has completed.
    PipelineCompleted {
        /// Final validation report.
        report: TaskValidationReport,
    },
    /// The pipeline has failed.
    PipelineFailed {
        /// Error message.
        error: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
}

impl PipelineEvent {
    /// Creates a stage started event.
    pub fn stage_started(stage: PipelineStage) -> Self {
        PipelineEvent::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    /// Creates a stage completed event.
    pub fn stage_completed(stage: PipelineStage, result: ValidationResult) -> Self {
        PipelineEvent::StageCompleted {
            stage,
            result,
            timestamp: Utc::now(),
        }
    }

    /// Creates a stage failed event.
    pub fn stage_failed(stage: PipelineStage, error: impl Into<String>) -> Self {
        PipelineEvent::StageFailed {
            stage,
            error: error.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates an agent reasoning event.
    pub fn agent_reasoning(stage: PipelineStage, reasoning: impl Into<String>) -> Self {
        PipelineEvent::AgentReasoning {
            stage,
            reasoning: reasoning.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a pipeline completed event.
    pub fn pipeline_completed(report: TaskValidationReport) -> Self {
        PipelineEvent::PipelineCompleted { report }
    }

    /// Creates a pipeline failed event.
    pub fn pipeline_failed(error: impl Into<String>) -> Self {
        PipelineEvent::PipelineFailed {
            error: error.into(),
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Pending.to_string(), "pending");
        assert_eq!(AgentStatus::Running.to_string(), "running");
        assert_eq!(AgentStatus::Completed.to_string(), "completed");
        assert_eq!(AgentStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_validation_score_clamping() {
        let score = ValidationScore::new(1.5, "Test score");
        assert_eq!(score.score, 1.0);

        let score = ValidationScore::new(-0.5, "Negative score");
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn test_validation_score_threshold() {
        let score = ValidationScore::new(0.75, "Good score");
        assert!(score.passes_threshold(0.7));
        assert!(!score.passes_threshold(0.8));
    }

    #[test]
    fn test_validation_result_creation() {
        let result =
            ValidationResult::success_full("Good result", "All tests passed", 0.9, "test_agent");

        assert!(result.is_success());
        assert_eq!(result.agent_name(), "test_agent");
        assert_eq!(result.score(), Some(0.9));
    }

    #[test]
    fn test_validation_result_failure() {
        let result =
            ValidationResult::failure_with_details("Bad result", "Test failed", "test_agent");

        assert!(!result.is_success());
        assert_eq!(result.agent_name(), "test_agent");
        assert_eq!(result.score(), None);
        assert_eq!(result.details(), Some("Test failed"));
    }

    #[test]
    fn test_task_validation_report_finalize() {
        let task = GeneratedTask::minimal(
            "test-123",
            "template-1",
            DifficultyLevel::Medium,
            "Test instruction",
        );
        let mut report = TaskValidationReport::new(task);

        report.add_validation(ValidationResult::success_full(
            "Good",
            "Details 1",
            0.8,
            "agent1",
        ));
        report.add_validation(ValidationResult::success_full(
            "Acceptable",
            "Details 2",
            0.6,
            "agent2",
        ));

        report.finalize("All validations passed", 1500);

        assert!(report.overall_passed);
        assert!((report.overall_score - 0.7).abs() < 0.01);
        assert_eq!(report.duration_ms, 1500);
    }

    #[test]
    fn test_pipeline_stage_display() {
        assert_eq!(PipelineStage::TaskGeneration.to_string(), "Task Generation");
        assert_eq!(
            PipelineStage::DifficultyValidation.to_string(),
            "Difficulty Validation"
        );
    }

    #[test]
    fn test_synthetic_pipeline_stages() {
        let stages = PipelineStage::all_synthetic_stages();
        assert_eq!(stages.len(), 4);
        assert_eq!(stages[0], PipelineStage::SyntheticIdeation);
        assert_eq!(stages[1], PipelineStage::SyntheticValidation);
        assert_eq!(stages[2], PipelineStage::SyntheticExecution);
        assert_eq!(stages[3], PipelineStage::SyntheticQualityCheck);
    }

    #[test]
    fn test_synthetic_stage_display_names() {
        assert_eq!(
            PipelineStage::SyntheticIdeation.display_name(),
            "Task Ideation"
        );
        assert_eq!(
            PipelineStage::SyntheticValidation.display_name(),
            "Complexity Validation"
        );
        assert_eq!(
            PipelineStage::SyntheticExecution.display_name(),
            "Task Execution"
        );
        assert_eq!(
            PipelineStage::SyntheticQualityCheck.display_name(),
            "Quality Check"
        );
    }

    #[test]
    fn test_synthetic_stage_to_string() {
        assert_eq!(
            PipelineStage::SyntheticIdeation.to_string(),
            "Task Ideation"
        );
        assert_eq!(
            PipelineStage::SyntheticValidation.to_string(),
            "Complexity Validation"
        );
        assert_eq!(
            PipelineStage::SyntheticExecution.to_string(),
            "Task Execution"
        );
        assert_eq!(
            PipelineStage::SyntheticQualityCheck.to_string(),
            "Quality Check"
        );
    }

    #[test]
    fn test_all_stages_backward_compatible() {
        // Ensure all_stages() still returns only validation stages
        let stages = PipelineStage::all_stages();
        assert_eq!(stages.len(), 4);
        assert_eq!(stages[0], PipelineStage::TaskGeneration);
        assert_eq!(stages[1], PipelineStage::DifficultyValidation);
        assert_eq!(stages[2], PipelineStage::FeasibilityValidation);
        assert_eq!(stages[3], PipelineStage::FinalApproval);
    }

    #[test]
    fn test_agent_message_creation() {
        let msg = AgentMessage::new(
            "generator",
            "validator",
            MessageType::GenerationComplete,
            serde_json::json!({"task_id": "test-123"}),
        );

        assert_eq!(msg.from_agent, "generator");
        assert_eq!(msg.to_agent, "validator");
        assert!(!msg.id.is_empty());
    }

    #[test]
    fn test_broadcast_message() {
        let msg = AgentMessage::broadcast(
            "orchestrator",
            MessageType::StatusUpdate,
            serde_json::json!({"status": "running"}),
        );

        assert_eq!(msg.to_agent, "broadcast");
    }
}
