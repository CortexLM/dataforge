//! Synthetic Task Orchestrator for the benchmark generation pipeline.
//!
//! This orchestrator coordinates the 3-agent pipeline for generating synthetic benchmark tasks:
//! 1. **Ideator Agent**: Generates creative task ideas with high temperature
//! 2. **Task Validator Agent**: Validates task complexity and memorization risk
//! 3. **Task Executor Agent**: Creates full task specification with hidden solution
//!
//! The pipeline includes retry logic for validation failures and emits events for TUI updates.

use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::execution::DockerClient;
use crate::llm::LlmProvider;

use super::docker_validator::{DockerValidatorAgent, DockerValidatorConfig};
use super::error::{AgentError, AgentResult};
use super::ideator::{IdeatorAgent, IdeatorConfig, TaskCategory, TaskIdea as IdeatorTaskIdea};
use super::task_executor::{SyntheticTask, TaskExecutorAgent, TaskExecutorConfig};
use super::task_validator::{
    TaskIdea as ValidatorTaskIdea, TaskValidatorAgent, TaskValidatorConfig, ValidationAssessment,
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Synthetic Orchestrator.
#[derive(Debug, Clone)]
pub struct SyntheticOrchestratorConfig {
    /// Configuration for the ideator agent.
    pub ideator_config: IdeatorConfig,
    /// Configuration for the validator agent.
    pub validator_config: TaskValidatorConfig,
    /// Configuration for the executor agent.
    pub executor_config: TaskExecutorConfig,
    /// Minimum validation score to proceed to execution.
    pub min_validation_score: f64,
    /// Maximum retries for ideation if validation fails.
    pub max_ideation_retries: u32,
    /// Whether to continue generating if one task fails.
    pub continue_on_failure: bool,
    /// Whether to validate tasks in Docker containers.
    pub docker_validation_enabled: bool,
    /// Whether to validate the reference solution in Docker.
    pub docker_validate_solution: bool,
}

impl Default for SyntheticOrchestratorConfig {
    fn default() -> Self {
        Self {
            ideator_config: IdeatorConfig::default(),
            validator_config: TaskValidatorConfig::default(),
            executor_config: TaskExecutorConfig::default(),
            min_validation_score: 0.6,
            max_ideation_retries: 3,
            continue_on_failure: true,
            docker_validation_enabled: false,
            docker_validate_solution: true,
        }
    }
}

impl SyntheticOrchestratorConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the ideator configuration.
    pub fn with_ideator_config(mut self, config: IdeatorConfig) -> Self {
        self.ideator_config = config;
        self
    }

    /// Sets the validator configuration.
    pub fn with_validator_config(mut self, config: TaskValidatorConfig) -> Self {
        self.validator_config = config;
        self
    }

    /// Sets the executor configuration.
    pub fn with_executor_config(mut self, config: TaskExecutorConfig) -> Self {
        self.executor_config = config;
        self
    }

    /// Sets the minimum validation score to proceed.
    pub fn with_min_validation_score(mut self, score: f64) -> Self {
        self.min_validation_score = score.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum ideation retries.
    pub fn with_max_ideation_retries(mut self, retries: u32) -> Self {
        self.max_ideation_retries = retries;
        self
    }

    /// Sets whether to continue on failure during batch generation.
    pub fn with_continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    /// Enables or disables Docker validation.
    pub fn with_docker_validation(mut self, enabled: bool) -> Self {
        self.docker_validation_enabled = enabled;
        self
    }

    /// Sets whether to validate the reference solution in Docker.
    pub fn with_docker_solution_validation(mut self, enabled: bool) -> Self {
        self.docker_validate_solution = enabled;
        self
    }
}

// ============================================================================
// Pipeline Stage Enum
// ============================================================================

/// Stages in the synthetic task generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyntheticPipelineStage {
    /// Task ideation with high temperature.
    Ideation,
    /// Validating task difficulty and complexity.
    Validation,
    /// Creating full task specification with hidden solution.
    Execution,
    /// Docker-based validation of the task environment.
    DockerValidation,
    /// Final quality check.
    QualityCheck,
}

impl std::fmt::Display for SyntheticPipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyntheticPipelineStage::Ideation => write!(f, "Ideation"),
            SyntheticPipelineStage::Validation => write!(f, "Validation"),
            SyntheticPipelineStage::Execution => write!(f, "Execution"),
            SyntheticPipelineStage::DockerValidation => write!(f, "Docker Validation"),
            SyntheticPipelineStage::QualityCheck => write!(f, "Quality Check"),
        }
    }
}

// ============================================================================
// Pipeline Events
// ============================================================================

/// Events emitted during the synthetic task generation pipeline for TUI updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyntheticPipelineEvent {
    /// A pipeline stage has started.
    StageStarted {
        /// The stage that started.
        stage: SyntheticPipelineStage,
        /// When the stage started.
        timestamp: DateTime<Utc>,
    },
    /// Ideation stage completed successfully.
    IdeationComplete {
        /// The generated task idea.
        idea: IdeatorTaskIdea,
        /// When ideation completed.
        timestamp: DateTime<Utc>,
    },
    /// Validation stage completed.
    ValidationComplete {
        /// The validation assessment.
        assessment: ValidationAssessment,
        /// Whether validation passed.
        passed: bool,
        /// When validation completed.
        timestamp: DateTime<Utc>,
    },
    /// Validation was rejected, retrying ideation.
    ValidationRejected {
        /// Reasons for rejection.
        reasons: Vec<String>,
        /// Current retry count.
        retry_count: u32,
        /// When rejection occurred.
        timestamp: DateTime<Utc>,
    },
    /// Execution stage completed successfully.
    ExecutionComplete {
        /// The created synthetic task.
        task: SyntheticTask,
        /// When execution completed.
        timestamp: DateTime<Utc>,
    },
    /// Docker validation started.
    DockerValidationStarted {
        /// Task ID being validated.
        task_id: String,
        /// Container image being used.
        image: String,
        /// When validation started.
        timestamp: DateTime<Utc>,
    },
    /// Docker validation completed.
    DockerValidationComplete {
        /// Whether Docker validation passed.
        passed: bool,
        /// Duration in milliseconds.
        duration_ms: u64,
        /// Error message if failed.
        error: Option<String>,
        /// When validation completed.
        timestamp: DateTime<Utc>,
    },
    /// Docker validation was skipped (Docker not available or disabled).
    DockerValidationSkipped {
        /// Reason for skipping.
        reason: String,
        /// When skipped.
        timestamp: DateTime<Utc>,
    },
    /// Pipeline completed successfully.
    PipelineComplete {
        /// The final synthetic task.
        task: SyntheticTask,
        /// Total duration in milliseconds.
        total_duration_ms: u64,
        /// Number of retries that occurred.
        retries: u32,
    },
    /// Pipeline failed.
    PipelineFailed {
        /// Error description.
        error: String,
        /// Stage where failure occurred.
        stage: SyntheticPipelineStage,
        /// When failure occurred.
        timestamp: DateTime<Utc>,
    },
}

impl SyntheticPipelineEvent {
    /// Creates a StageStarted event.
    pub fn stage_started(stage: SyntheticPipelineStage) -> Self {
        Self::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    /// Creates an IdeationComplete event.
    pub fn ideation_complete(idea: IdeatorTaskIdea) -> Self {
        Self::IdeationComplete {
            idea,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ValidationComplete event.
    pub fn validation_complete(assessment: ValidationAssessment, passed: bool) -> Self {
        Self::ValidationComplete {
            assessment,
            passed,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ValidationRejected event.
    pub fn validation_rejected(reasons: Vec<String>, retry_count: u32) -> Self {
        Self::ValidationRejected {
            reasons,
            retry_count,
            timestamp: Utc::now(),
        }
    }

    /// Creates an ExecutionComplete event.
    pub fn execution_complete(task: SyntheticTask) -> Self {
        Self::ExecutionComplete {
            task,
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineComplete event.
    pub fn pipeline_complete(task: SyntheticTask, total_duration_ms: u64, retries: u32) -> Self {
        Self::PipelineComplete {
            task,
            total_duration_ms,
            retries,
        }
    }

    /// Creates a DockerValidationStarted event.
    pub fn docker_validation_started(task_id: impl Into<String>, image: impl Into<String>) -> Self {
        Self::DockerValidationStarted {
            task_id: task_id.into(),
            image: image.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a DockerValidationComplete event.
    pub fn docker_validation_complete(
        passed: bool,
        duration_ms: u64,
        error: Option<String>,
    ) -> Self {
        Self::DockerValidationComplete {
            passed,
            duration_ms,
            error,
            timestamp: Utc::now(),
        }
    }

    /// Creates a DockerValidationSkipped event.
    pub fn docker_validation_skipped(reason: impl Into<String>) -> Self {
        Self::DockerValidationSkipped {
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineFailed event.
    pub fn pipeline_failed(error: impl Into<String>, stage: SyntheticPipelineStage) -> Self {
        Self::PipelineFailed {
            error: error.into(),
            stage,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Synthetic Orchestrator
// ============================================================================

/// Orchestrator that coordinates the synthetic task generation pipeline.
///
/// The pipeline stages are:
/// 1. **Ideation**: Generate creative task ideas using high temperature LLM
/// 2. **Validation**: Validate complexity, memorization risk, and reasoning requirements
/// 3. **Execution**: Create complete task specification with hidden solution
/// 4. **Docker Validation**: Validate task runs in Docker container (optional)
/// 5. **Quality Check**: Verify task has all required fields and anti-memorization config
pub struct SyntheticOrchestrator {
    /// The ideator agent for task idea generation.
    ideator: IdeatorAgent,
    /// The validator agent for task validation.
    validator: TaskValidatorAgent,
    /// The executor agent for task creation.
    executor: TaskExecutorAgent,
    /// Docker validator agent (lazy-initialized).
    docker_validator: Option<DockerValidatorAgent>,
    /// Orchestrator configuration.
    config: SyntheticOrchestratorConfig,
}

impl SyntheticOrchestrator {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "synthetic_orchestrator";

    /// Creates a new synthetic orchestrator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: SyntheticOrchestratorConfig) -> Self {
        let ideator = IdeatorAgent::new(Arc::clone(&llm_client), config.ideator_config.clone());
        let validator =
            TaskValidatorAgent::new(Arc::clone(&llm_client), config.validator_config.clone());
        let executor = TaskExecutorAgent::new(llm_client, config.executor_config.clone());

        // Initialize Docker validator if enabled
        let docker_validator = if config.docker_validation_enabled {
            match DockerClient::new() {
                Ok(client) => {
                    let docker_config = DockerValidatorConfig::new()
                        .with_solution_validation(config.docker_validate_solution);
                    Some(DockerValidatorAgent::new(Arc::new(client), docker_config))
                }
                Err(e) => {
                    tracing::warn!("Docker validation enabled but Docker unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            ideator,
            validator,
            executor,
            docker_validator,
            config,
        }
    }

    /// Creates a new orchestrator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, SyntheticOrchestratorConfig::default())
    }

    /// Creates a new orchestrator with Docker validation enabled.
    pub fn with_docker_validation(llm_client: Arc<dyn LlmProvider>) -> Self {
        let config = SyntheticOrchestratorConfig::default().with_docker_validation(true);
        Self::new(llm_client, config)
    }

    /// Runs the complete synthetic task generation pipeline.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to focus on. If None, a random category is used.
    /// * `event_tx` - Channel sender for pipeline events.
    ///
    /// # Returns
    ///
    /// A complete `SyntheticTask`, or an error if the pipeline fails.
    pub async fn generate_task(
        &self,
        category: Option<TaskCategory>,
        event_tx: mpsc::Sender<SyntheticPipelineEvent>,
    ) -> AgentResult<SyntheticTask> {
        let start_time = Instant::now();
        let mut retries = 0u32;

        // Stage 1 & 2: Ideation with validation loop
        let (idea, assessment) = self
            .ideate_with_validation(category, &event_tx, &mut retries)
            .await?;

        // Stage 3: Execution
        self.send_event(
            &event_tx,
            SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::Execution),
        )
        .await;

        // Convert ideator TaskIdea to validator TaskIdea for the executor
        let validator_idea = Self::convert_to_validator_idea(&idea);
        let task = match self
            .executor
            .create_task(&validator_idea, &assessment)
            .await
        {
            Ok(task) => task,
            Err(e) => {
                self.send_event(
                    &event_tx,
                    SyntheticPipelineEvent::pipeline_failed(
                        e.to_string(),
                        SyntheticPipelineStage::Execution,
                    ),
                )
                .await;
                return Err(e);
            }
        };

        self.send_event(
            &event_tx,
            SyntheticPipelineEvent::execution_complete(task.clone()),
        )
        .await;

        // Stage 4: Docker Validation (if enabled)
        if self.config.docker_validation_enabled {
            self.send_event(
                &event_tx,
                SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::DockerValidation),
            )
            .await;

            if let Some(ref docker_validator) = self.docker_validator {
                self.send_event(
                    &event_tx,
                    SyntheticPipelineEvent::docker_validation_started(
                        &task.id,
                        "python:3.11-slim", // Default image
                    ),
                )
                .await;

                match docker_validator.validate_task(&task).await {
                    Ok(result) => {
                        self.send_event(
                            &event_tx,
                            SyntheticPipelineEvent::docker_validation_complete(
                                result.passed,
                                result.duration_ms,
                                result.error.clone(),
                            ),
                        )
                        .await;

                        if !result.passed {
                            let error_msg = result
                                .error
                                .unwrap_or_else(|| "Docker validation failed".to_string());
                            self.send_event(
                                &event_tx,
                                SyntheticPipelineEvent::pipeline_failed(
                                    &error_msg,
                                    SyntheticPipelineStage::DockerValidation,
                                ),
                            )
                            .await;
                            return Err(AgentError::GenerationFailed(error_msg));
                        }
                    }
                    Err(e) => {
                        self.send_event(
                            &event_tx,
                            SyntheticPipelineEvent::docker_validation_complete(
                                false,
                                0,
                                Some(e.to_string()),
                            ),
                        )
                        .await;
                        self.send_event(
                            &event_tx,
                            SyntheticPipelineEvent::pipeline_failed(
                                e.to_string(),
                                SyntheticPipelineStage::DockerValidation,
                            ),
                        )
                        .await;
                        return Err(e);
                    }
                }
            } else {
                self.send_event(
                    &event_tx,
                    SyntheticPipelineEvent::docker_validation_skipped(
                        "Docker daemon not available",
                    ),
                )
                .await;
            }
        }

        // Stage 5: Quality Check
        self.send_event(
            &event_tx,
            SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::QualityCheck),
        )
        .await;

        if let Err(e) = self.validate_synthetic_task(&task) {
            self.send_event(
                &event_tx,
                SyntheticPipelineEvent::pipeline_failed(
                    e.to_string(),
                    SyntheticPipelineStage::QualityCheck,
                ),
            )
            .await;
            return Err(e);
        }

        // Pipeline complete
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.send_event(
            &event_tx,
            SyntheticPipelineEvent::pipeline_complete(task.clone(), duration_ms, retries),
        )
        .await;

        Ok(task)
    }

    /// Generates a batch of synthetic tasks.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of tasks to generate.
    /// * `categories` - Optional list of categories to cycle through.
    /// * `event_tx` - Channel sender for pipeline events.
    ///
    /// # Returns
    ///
    /// A vector of successfully generated `SyntheticTask` instances.
    pub async fn generate_batch(
        &self,
        count: usize,
        categories: Option<Vec<TaskCategory>>,
        event_tx: mpsc::Sender<SyntheticPipelineEvent>,
    ) -> AgentResult<Vec<SyntheticTask>> {
        let mut tasks = Vec::with_capacity(count);
        let available_categories = categories.unwrap_or_else(TaskCategory::all);

        for i in 0..count {
            let category_idx = i % available_categories.len();
            let category = available_categories[category_idx];

            match self.generate_task(Some(category), event_tx.clone()).await {
                Ok(task) => tasks.push(task),
                Err(e) => {
                    tracing::warn!(
                        "Failed to generate task {} for category {:?}: {}",
                        i,
                        category,
                        e
                    );
                    if !self.config.continue_on_failure {
                        return Err(e);
                    }
                }
            }
        }

        if tasks.is_empty() && count > 0 {
            return Err(AgentError::GenerationFailed(
                "Failed to generate any synthetic tasks".to_string(),
            ));
        }

        Ok(tasks)
    }

    /// Runs ideation with validation loop until a valid task idea is found.
    async fn ideate_with_validation(
        &self,
        category: Option<TaskCategory>,
        event_tx: &mpsc::Sender<SyntheticPipelineEvent>,
        retries: &mut u32,
    ) -> AgentResult<(IdeatorTaskIdea, ValidationAssessment)> {
        loop {
            // Stage 1: Ideation
            self.send_event(
                event_tx,
                SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::Ideation),
            )
            .await;

            let idea = match self.ideator.generate_task_idea(category).await {
                Ok(idea) => idea,
                Err(e) => {
                    self.send_event(
                        event_tx,
                        SyntheticPipelineEvent::pipeline_failed(
                            e.to_string(),
                            SyntheticPipelineStage::Ideation,
                        ),
                    )
                    .await;
                    return Err(e);
                }
            };

            self.send_event(
                event_tx,
                SyntheticPipelineEvent::ideation_complete(idea.clone()),
            )
            .await;

            // Stage 2: Validation
            self.send_event(
                event_tx,
                SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::Validation),
            )
            .await;

            // Convert ideator TaskIdea to validator TaskIdea
            let validator_idea = Self::convert_to_validator_idea(&idea);
            let assessment = match self.validator.validate_task(&validator_idea).await {
                Ok(assessment) => assessment,
                Err(e) => {
                    self.send_event(
                        event_tx,
                        SyntheticPipelineEvent::pipeline_failed(
                            e.to_string(),
                            SyntheticPipelineStage::Validation,
                        ),
                    )
                    .await;
                    return Err(e);
                }
            };

            // Check if validation passed
            let passed = assessment.is_valid
                && assessment.complexity_score >= self.config.min_validation_score;

            self.send_event(
                event_tx,
                SyntheticPipelineEvent::validation_complete(assessment.clone(), passed),
            )
            .await;

            if passed {
                return Ok((idea, assessment));
            }

            // Validation failed - check retry limit
            *retries += 1;
            if *retries > self.config.max_ideation_retries {
                let error_msg = format!(
                    "Validation failed after {} retries. Last rejection reasons: {:?}",
                    self.config.max_ideation_retries, assessment.rejection_reasons
                );
                self.send_event(
                    event_tx,
                    SyntheticPipelineEvent::pipeline_failed(
                        &error_msg,
                        SyntheticPipelineStage::Validation,
                    ),
                )
                .await;
                return Err(AgentError::ThresholdNotMet {
                    score: assessment.complexity_score,
                    threshold: self.config.min_validation_score,
                });
            }

            // Emit rejection event and retry
            let reasons = if assessment.rejection_reasons.is_empty() {
                vec![format!(
                    "Complexity score {:.2} below threshold {:.2}",
                    assessment.complexity_score, self.config.min_validation_score
                )]
            } else {
                assessment.rejection_reasons.clone()
            };

            self.send_event(
                event_tx,
                SyntheticPipelineEvent::validation_rejected(reasons, *retries),
            )
            .await;

            tracing::info!(
                "Validation rejected (attempt {}), retrying ideation...",
                retries
            );
        }
    }

    /// Sends an event through the channel, ignoring send errors.
    async fn send_event(
        &self,
        event_tx: &mpsc::Sender<SyntheticPipelineEvent>,
        event: SyntheticPipelineEvent,
    ) {
        // Ignore send errors - receiver may have been dropped
        let _ = event_tx.send(event).await;
    }

    /// Validates that a synthetic task has all required fields.
    fn validate_synthetic_task(&self, task: &SyntheticTask) -> AgentResult<()> {
        // Check problem statement
        if task.problem_statement.trim().is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing problem statement".to_string(),
            ));
        }

        // Check hidden solution
        if task.hidden_solution.approach.trim().is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing hidden solution approach".to_string(),
            ));
        }

        // Check verification criteria
        if task.verification.success_criteria.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing success criteria".to_string(),
            ));
        }

        // Check anti-memorization config
        if task.anti_memorization.canary_token.is_empty()
            && self.config.executor_config.include_canary
        {
            return Err(AgentError::GenerationFailed(
                "Task missing canary token for anti-memorization".to_string(),
            ));
        }

        Ok(())
    }

    /// Converts an ideator TaskIdea to a validator TaskIdea.
    fn convert_to_validator_idea(idea: &IdeatorTaskIdea) -> ValidatorTaskIdea {
        ValidatorTaskIdea::new(
            &idea.title,
            &idea.description,
            idea.category.to_benchmark_category(),
            idea.required_skills.clone(),
        )
    }

    /// Returns a reference to the ideator agent.
    pub fn ideator(&self) -> &IdeatorAgent {
        &self.ideator
    }

    /// Returns a reference to the validator agent.
    pub fn validator(&self) -> &TaskValidatorAgent {
        &self.validator
    }

    /// Returns a reference to the executor agent.
    pub fn executor(&self) -> &TaskExecutorAgent {
        &self.executor
    }

    /// Returns the orchestrator configuration.
    pub fn config(&self) -> &SyntheticOrchestratorConfig {
        &self.config
    }
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating a SyntheticOrchestrator with fluent API.
pub struct SyntheticOrchestratorBuilder {
    llm_client: Option<Arc<dyn LlmProvider>>,
    ideator_config: Option<IdeatorConfig>,
    validator_config: Option<TaskValidatorConfig>,
    executor_config: Option<TaskExecutorConfig>,
    min_validation_score: f64,
    max_ideation_retries: u32,
    continue_on_failure: bool,
}

impl SyntheticOrchestratorBuilder {
    /// Creates a new builder with default values.
    pub fn new() -> Self {
        Self {
            llm_client: None,
            ideator_config: None,
            validator_config: None,
            executor_config: None,
            min_validation_score: 0.6,
            max_ideation_retries: 3,
            continue_on_failure: true,
        }
    }

    /// Sets the LLM client.
    pub fn llm_client(mut self, client: Arc<dyn LlmProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Sets the ideator configuration.
    pub fn ideator_config(mut self, config: IdeatorConfig) -> Self {
        self.ideator_config = Some(config);
        self
    }

    /// Sets the validator configuration.
    pub fn validator_config(mut self, config: TaskValidatorConfig) -> Self {
        self.validator_config = Some(config);
        self
    }

    /// Sets the executor configuration.
    pub fn executor_config(mut self, config: TaskExecutorConfig) -> Self {
        self.executor_config = Some(config);
        self
    }

    /// Sets the minimum validation score to proceed.
    pub fn min_validation_score(mut self, score: f64) -> Self {
        self.min_validation_score = score.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum ideation retries.
    pub fn max_ideation_retries(mut self, retries: u32) -> Self {
        self.max_ideation_retries = retries;
        self
    }

    /// Sets whether to continue on failure during batch generation.
    pub fn continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    /// Builds the SyntheticOrchestrator.
    pub fn build(self) -> AgentResult<SyntheticOrchestrator> {
        let llm_client = self
            .llm_client
            .ok_or_else(|| AgentError::ConfigurationError("LLM client is required".to_string()))?;

        let config = SyntheticOrchestratorConfig {
            ideator_config: self.ideator_config.unwrap_or_default(),
            validator_config: self.validator_config.unwrap_or_default(),
            executor_config: self.executor_config.unwrap_or_default(),
            min_validation_score: self.min_validation_score,
            max_ideation_retries: self.max_ideation_retries,
            continue_on_failure: self.continue_on_failure,
            docker_validation_enabled: false,
            docker_validate_solution: true,
        };

        Ok(SyntheticOrchestrator::new(llm_client, config))
    }
}

impl Default for SyntheticOrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationRequest, GenerationResponse, Message, Usage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Mock LLM provider that returns predetermined responses based on call count.
    struct MockLlmProvider {
        responses: Mutex<Vec<String>>,
        call_count: AtomicUsize,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }

        fn single_response(response: String) -> Self {
            Self::new(vec![response])
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().expect("lock not poisoned");
            let content = responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| responses.last().cloned().unwrap_or_default());

            Ok(GenerationResponse {
                id: format!("mock-{}", idx),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            })
        }
    }

    fn mock_ideator_response() -> String {
        r#"{
            "title": "Debug Memory Leak in Async Service",
            "description": "A production service is experiencing gradual memory growth over 24 hours. Using provided heap dumps and allocation traces, identify the source of the memory leak and propose a fix.",
            "estimated_difficulty": "hard",
            "required_skills": ["memory profiling", "async rust", "heap analysis"],
            "anti_patterns": ["restarting the service", "increasing memory limits"]
        }"#.to_string()
    }

    fn mock_validator_response() -> String {
        r#"{
            "complexity_score": 0.85,
            "memorization_risk": 0.15,
            "estimated_thinking_time_minutes": 20,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": ["Consider adding more heap dump samples"],
            "reasoning": "This task requires multi-step analysis of memory allocation patterns and cannot be solved through memorization."
        }"#.to_string()
    }

    fn mock_executor_response() -> String {
        r#"{
            "problem_statement": "A production microservice written in async Rust is experiencing gradual memory growth. After 24 hours of operation, the service consumes 4GB of RAM (started at 512MB). You have been provided with heap dumps taken at 1-hour intervals. Your task is to identify the root cause of the memory leak and propose a code fix.",
            "hidden_solution": {
                "approach": "Analyze heap dump diffs to identify objects accumulating over time. Look for Arc<Mutex<>> patterns in async code that might hold references longer than necessary. The leak is in the connection pool not properly releasing connections on timeout.",
                "key_insights": ["Heap diff analysis reveals growing Vec<Connection>", "Connection timeout handler doesn't remove from pool", "Arc cycles prevent cleanup"],
                "reference_commands": ["heap-analyzer diff dump_1h.hprof dump_2h.hprof", "grep -r 'Arc<Mutex' src/"],
                "expected_time_seconds": 1200,
                "step_count": 5
            },
            "verification": {
                "success_criteria": ["Correctly identifies connection pool as leak source", "Proposes fix for timeout handling", "Memory growth stops after fix"],
                "partial_credit": [
                    {"criterion": "Identifies memory is growing", "points": 0.2},
                    {"criterion": "Narrows down to networking code", "points": 0.4}
                ],
                "automated_checks": [
                    {"type": "OutputContains", "target": "analysis.txt", "expected": "connection_pool"},
                    {"type": "FileExists", "target": "fix.patch", "expected": "true"}
                ]
            },
            "difficulty": {
                "level": "hard",
                "complexity_factors": ["Async Rust complexity", "Heap dump analysis", "Reference cycle detection"],
                "base_score": 50.0
            },
            "tags": ["memory-leak", "async-rust", "debugging", "heap-analysis"]
        }"#.to_string()
    }

    #[test]
    fn test_config_default() {
        let config = SyntheticOrchestratorConfig::default();

        assert!((config.min_validation_score - 0.6).abs() < 0.01);
        assert_eq!(config.max_ideation_retries, 3);
        assert!(config.continue_on_failure);
    }

    #[test]
    fn test_config_builder() {
        let config = SyntheticOrchestratorConfig::new()
            .with_min_validation_score(0.8)
            .with_max_ideation_retries(5)
            .with_continue_on_failure(false);

        assert!((config.min_validation_score - 0.8).abs() < 0.01);
        assert_eq!(config.max_ideation_retries, 5);
        assert!(!config.continue_on_failure);
    }

    #[test]
    fn test_config_score_clamping() {
        let config = SyntheticOrchestratorConfig::new().with_min_validation_score(1.5);
        assert!((config.min_validation_score - 1.0).abs() < 0.01);

        let config = SyntheticOrchestratorConfig::new().with_min_validation_score(-0.5);
        assert!((config.min_validation_score - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_pipeline_stage_display() {
        assert_eq!(format!("{}", SyntheticPipelineStage::Ideation), "Ideation");
        assert_eq!(
            format!("{}", SyntheticPipelineStage::Validation),
            "Validation"
        );
        assert_eq!(
            format!("{}", SyntheticPipelineStage::Execution),
            "Execution"
        );
        assert_eq!(
            format!("{}", SyntheticPipelineStage::QualityCheck),
            "Quality Check"
        );
    }

    #[test]
    fn test_event_constructors() {
        let stage_event = SyntheticPipelineEvent::stage_started(SyntheticPipelineStage::Ideation);
        match stage_event {
            SyntheticPipelineEvent::StageStarted { stage, .. } => {
                assert_eq!(stage, SyntheticPipelineStage::Ideation);
            }
            _ => panic!("Expected StageStarted event"),
        }

        let rejected_event =
            SyntheticPipelineEvent::validation_rejected(vec!["reason1".to_string()], 2);
        match rejected_event {
            SyntheticPipelineEvent::ValidationRejected {
                reasons,
                retry_count,
                ..
            } => {
                assert_eq!(reasons.len(), 1);
                assert_eq!(retry_count, 2);
            }
            _ => panic!("Expected ValidationRejected event"),
        }

        let failed_event = SyntheticPipelineEvent::pipeline_failed(
            "Test error",
            SyntheticPipelineStage::Execution,
        );
        match failed_event {
            SyntheticPipelineEvent::PipelineFailed { error, stage, .. } => {
                assert_eq!(error, "Test error");
                assert_eq!(stage, SyntheticPipelineStage::Execution);
            }
            _ => panic!("Expected PipelineFailed event"),
        }
    }

    #[test]
    fn test_orchestrator_builder_missing_llm() {
        let result = SyntheticOrchestratorBuilder::new().build();

        assert!(result.is_err());
        match result {
            Err(AgentError::ConfigurationError(msg)) => {
                assert!(msg.contains("LLM client"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_orchestrator_builder_success() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));

        let result = SyntheticOrchestratorBuilder::new()
            .llm_client(mock_llm)
            .min_validation_score(0.7)
            .max_ideation_retries(5)
            .continue_on_failure(false)
            .build();

        assert!(result.is_ok());
        let orchestrator = result.expect("should build successfully");
        assert!((orchestrator.config().min_validation_score - 0.7).abs() < 0.01);
        assert_eq!(orchestrator.config().max_ideation_retries, 5);
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        assert_eq!(SyntheticOrchestrator::AGENT_NAME, "synthetic_orchestrator");
        assert!((orchestrator.config().min_validation_score - 0.6).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_convert_to_validator_idea() {
        let ideator_idea = IdeatorTaskIdea::new(
            TaskCategory::Debugging,
            "memory-debugging",
            "Test Title",
            "Test Description",
            crate::difficulty::DifficultyLevel::Hard,
            vec!["skill1".to_string()],
            vec!["anti1".to_string()],
        );

        let validator_idea = SyntheticOrchestrator::convert_to_validator_idea(&ideator_idea);

        assert_eq!(validator_idea.title, "Test Title");
        assert_eq!(validator_idea.description, "Test Description");
        assert_eq!(validator_idea.category, "debugging");
        assert_eq!(validator_idea.required_skills, vec!["skill1".to_string()]);
    }

    #[tokio::test]
    async fn test_validate_synthetic_task_success() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let task = create_valid_task();
        let result = orchestrator.validate_synthetic_task(&task);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_synthetic_task_missing_problem() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let task = create_task_with_empty_problem();
        let result = orchestrator.validate_synthetic_task(&task);

        assert!(result.is_err());
        match result {
            Err(AgentError::GenerationFailed(msg)) => {
                assert!(msg.contains("problem statement"));
            }
            _ => panic!("Expected GenerationFailed error"),
        }
    }

    #[tokio::test]
    async fn test_validate_synthetic_task_missing_criteria() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let task = create_task_without_criteria();
        let result = orchestrator.validate_synthetic_task(&task);

        assert!(result.is_err());
        match result {
            Err(AgentError::GenerationFailed(msg)) => {
                assert!(msg.contains("success criteria"));
            }
            _ => panic!("Expected GenerationFailed error"),
        }
    }

    #[tokio::test]
    async fn test_full_pipeline_success() {
        // Setup mock responses for: ideator, validator, executor
        let responses = vec![
            mock_ideator_response(),
            mock_validator_response(),
            mock_executor_response(),
        ];
        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let (event_tx, mut event_rx) = mpsc::channel(100);

        let result = orchestrator
            .generate_task(Some(TaskCategory::Debugging), event_tx)
            .await;

        assert!(result.is_ok(), "Pipeline should succeed: {:?}", result);
        let task = result.expect("should have task");

        assert!(!task.problem_statement.is_empty());
        assert!(!task.hidden_solution.approach.is_empty());
        assert!(task.has_canary());

        // Verify events were emitted
        let mut events = Vec::new();
        event_rx.close();
        while let Some(event) = event_rx.recv().await {
            events.push(event);
        }

        // Should have: StageStarted(Ideation), IdeationComplete, StageStarted(Validation),
        // ValidationComplete, StageStarted(Execution), ExecutionComplete,
        // StageStarted(QualityCheck), PipelineComplete
        assert!(events.len() >= 7, "Should have at least 7 events");

        // Check for PipelineComplete event
        let has_complete = events.iter().any(|e| {
            matches!(
                e,
                SyntheticPipelineEvent::PipelineComplete { retries: 0, .. }
            )
        });
        assert!(has_complete, "Should have PipelineComplete event");
    }

    #[tokio::test]
    async fn test_pipeline_with_validation_retry() {
        // First validation fails (low complexity), second succeeds
        let low_score_validation = r#"{
            "complexity_score": 0.3,
            "memorization_risk": 0.1,
            "estimated_thinking_time_minutes": 2,
            "requires_genuine_reasoning": false,
            "rejection_reasons": ["Too simple"],
            "improvement_suggestions": [],
            "reasoning": "Task is too simple"
        }"#;

        let responses = vec![
            mock_ideator_response(),
            low_score_validation.to_string(),
            mock_ideator_response(),
            mock_validator_response(),
            mock_executor_response(),
        ];
        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let (event_tx, mut event_rx) = mpsc::channel(100);

        let result = orchestrator
            .generate_task(Some(TaskCategory::Debugging), event_tx)
            .await;

        assert!(result.is_ok(), "Pipeline should succeed after retry");

        // Verify rejection event was emitted
        let mut found_rejection = false;
        event_rx.close();
        while let Some(event) = event_rx.recv().await {
            if let SyntheticPipelineEvent::ValidationRejected { retry_count, .. } = event {
                found_rejection = true;
                assert_eq!(retry_count, 1);
            }
        }
        assert!(found_rejection, "Should have ValidationRejected event");
    }

    #[tokio::test]
    async fn test_pipeline_fails_after_max_retries() {
        // All validations fail
        let low_score_validation = r#"{
            "complexity_score": 0.2,
            "memorization_risk": 0.8,
            "estimated_thinking_time_minutes": 1,
            "requires_genuine_reasoning": false,
            "rejection_reasons": ["Common knowledge"],
            "improvement_suggestions": [],
            "reasoning": "Too easy"
        }"#;

        // Create enough responses for max_retries + 1 attempts
        let mut responses = Vec::new();
        for _ in 0..5 {
            responses.push(mock_ideator_response());
            responses.push(low_score_validation.to_string());
        }

        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let config = SyntheticOrchestratorConfig::new().with_max_ideation_retries(2);
        let orchestrator = SyntheticOrchestrator::new(mock_llm, config);

        let (event_tx, _event_rx) = mpsc::channel(100);

        let result = orchestrator
            .generate_task(Some(TaskCategory::Debugging), event_tx)
            .await;

        assert!(result.is_err());
        match result {
            Err(AgentError::ThresholdNotMet { .. }) => {}
            other => panic!("Expected ThresholdNotMet error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_batch_generation() {
        let responses = vec![
            mock_ideator_response(),
            mock_validator_response(),
            mock_executor_response(),
            mock_ideator_response(),
            mock_validator_response(),
            mock_executor_response(),
        ];
        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = SyntheticOrchestrator::with_defaults(mock_llm);

        let (event_tx, _event_rx) = mpsc::channel(100);

        let result = orchestrator
            .generate_batch(
                2,
                Some(vec![TaskCategory::Debugging, TaskCategory::Security]),
                event_tx,
            )
            .await;

        assert!(result.is_ok());
        let tasks = result.expect("should have tasks");
        assert_eq!(tasks.len(), 2);
    }

    // Helper functions for creating test tasks

    fn create_valid_task() -> SyntheticTask {
        use super::super::task_executor::{
            AntiMemorizationConfig, DifficultyScoring, HiddenSolution, TaskMetadata,
            VerificationSpec,
        };
        use crate::difficulty::DifficultyLevel;

        let solution = HiddenSolution::new("Test approach")
            .with_key_insights(["insight1"])
            .with_reference_commands(["cmd1"]);
        let verification =
            VerificationSpec::new().with_success_criteria(["Criterion 1", "Criterion 2"]);
        let difficulty = DifficultyScoring::new(DifficultyLevel::Medium);
        let metadata = TaskMetadata::new("debugging", "idea-1");
        let anti_mem = AntiMemorizationConfig::new("CANARY_123");

        SyntheticTask::new(
            "Solve this problem by analyzing the data.",
            solution,
            verification,
            difficulty,
            metadata,
        )
        .with_anti_memorization(anti_mem)
    }

    fn create_task_with_empty_problem() -> SyntheticTask {
        use super::super::task_executor::{
            AntiMemorizationConfig, DifficultyScoring, HiddenSolution, TaskMetadata,
            VerificationSpec,
        };
        use crate::difficulty::DifficultyLevel;

        let solution = HiddenSolution::new("Test approach");
        let verification = VerificationSpec::new().with_success_criteria(["Criterion 1"]);
        let difficulty = DifficultyScoring::new(DifficultyLevel::Medium);
        let metadata = TaskMetadata::new("debugging", "idea-1");
        let anti_mem = AntiMemorizationConfig::new("CANARY_123");

        SyntheticTask::new("   ", solution, verification, difficulty, metadata)
            .with_anti_memorization(anti_mem)
    }

    fn create_task_without_criteria() -> SyntheticTask {
        use super::super::task_executor::{
            AntiMemorizationConfig, DifficultyScoring, HiddenSolution, TaskMetadata,
            VerificationSpec,
        };
        use crate::difficulty::DifficultyLevel;

        let solution = HiddenSolution::new("Test approach");
        let verification = VerificationSpec::new(); // No criteria
        let difficulty = DifficultyScoring::new(DifficultyLevel::Medium);
        let metadata = TaskMetadata::new("debugging", "idea-1");
        let anti_mem = AntiMemorizationConfig::new("CANARY_123");

        SyntheticTask::new(
            "Valid problem statement",
            solution,
            verification,
            difficulty,
            metadata,
        )
        .with_anti_memorization(anti_mem)
    }
}
