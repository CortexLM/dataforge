//! Orchestrator Agent for the multi-agent validation system.
//!
//! This agent coordinates all other agents in the validation pipeline,
//! managing the workflow and providing events for TUI updates.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::difficulty::DifficultyLevel;
use crate::llm::LlmProvider;
use crate::template::TaskTemplate;

use super::difficulty_validator::{DifficultyValidatorAgent, DifficultyValidatorConfig};
use super::error::{AgentError, AgentResult};
use super::feasibility_validator::{FeasibilityValidatorAgent, FeasibilityValidatorConfig};
use super::generator::{GeneratorAgent, GeneratorAgentConfig};
use super::types::{
    GeneratedTask, PipelineEvent, PipelineStage, TaskValidationReport, ValidationResult,
};

/// Configuration for the Orchestrator Agent.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Configuration for the generator agent.
    pub generator_config: GeneratorAgentConfig,
    /// Configuration for the difficulty validator.
    pub difficulty_validator_config: DifficultyValidatorConfig,
    /// Configuration for the feasibility validator.
    pub feasibility_validator_config: FeasibilityValidatorConfig,
    /// Minimum overall score to pass final approval.
    pub final_approval_threshold: f64,
    /// Whether to continue pipeline even if a stage fails.
    pub continue_on_failure: bool,
}

impl OrchestratorConfig {
    /// Creates a new orchestrator configuration.
    pub fn new(generator_config: GeneratorAgentConfig) -> Self {
        Self {
            generator_config,
            difficulty_validator_config: DifficultyValidatorConfig::default(),
            feasibility_validator_config: FeasibilityValidatorConfig::default(),
            final_approval_threshold: 0.7,
            continue_on_failure: false,
        }
    }

    /// Sets the difficulty validator configuration.
    pub fn with_difficulty_config(mut self, config: DifficultyValidatorConfig) -> Self {
        self.difficulty_validator_config = config;
        self
    }

    /// Sets the feasibility validator configuration.
    pub fn with_feasibility_config(mut self, config: FeasibilityValidatorConfig) -> Self {
        self.feasibility_validator_config = config;
        self
    }

    /// Sets the final approval threshold.
    pub fn with_final_approval_threshold(mut self, threshold: f64) -> Self {
        self.final_approval_threshold = threshold;
        self
    }

    /// Sets whether to continue the pipeline on failure.
    pub fn continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }
}

/// Orchestrator Agent that coordinates the validation pipeline.
///
/// The pipeline stages are:
/// 1. Task Generation - Create a task from templates
/// 2. Difficulty Validation - Verify task matches expected difficulty
/// 3. Feasibility Validation - Verify task is solvable but not trivial
/// 4. Final Approval - Aggregate results and make final decision
pub struct OrchestratorAgent {
    generator: GeneratorAgent,
    difficulty_validator: DifficultyValidatorAgent,
    feasibility_validator: FeasibilityValidatorAgent,
    config: OrchestratorConfig,
}

impl OrchestratorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "orchestrator";

    /// Creates a new orchestrator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: OrchestratorConfig) -> Self {
        let generator = GeneratorAgent::new(config.generator_config.clone());
        let difficulty_validator = DifficultyValidatorAgent::new(
            Arc::clone(&llm_client),
            config.difficulty_validator_config.clone(),
        );
        let feasibility_validator =
            FeasibilityValidatorAgent::new(llm_client, config.feasibility_validator_config.clone());

        Self {
            generator,
            difficulty_validator,
            feasibility_validator,
            config,
        }
    }

    /// Creates an orchestrator with a single template.
    pub fn with_template(
        llm_client: Arc<dyn LlmProvider>,
        output_dir: impl Into<std::path::PathBuf>,
        template: TaskTemplate,
    ) -> Self {
        let generator_config = GeneratorAgentConfig::new(output_dir).with_template(template);
        let config = OrchestratorConfig::new(generator_config);
        Self::new(llm_client, config)
    }

    /// Runs the complete validation pipeline.
    ///
    /// # Arguments
    ///
    /// * `difficulty` - The target difficulty level for the task
    /// * `seed` - Random seed for task generation
    /// * `event_tx` - Channel sender for pipeline events
    ///
    /// # Returns
    ///
    /// A `TaskValidationReport` containing all validation results.
    pub async fn run_validation_pipeline(
        &self,
        difficulty: DifficultyLevel,
        seed: u64,
        event_tx: mpsc::Sender<PipelineEvent>,
    ) -> AgentResult<TaskValidationReport> {
        let start_time = Instant::now();

        // Stage 1: Task Generation
        let task = self
            .run_generation_stage(difficulty, seed, &event_tx)
            .await?;

        // Initialize the report
        let mut report = TaskValidationReport::new(task.clone());

        // Add generation result
        let generation_result = self.generator.create_validation_result(&task);
        report.add_validation(generation_result.clone());

        // Stage 2: Difficulty Validation
        let difficulty_result = self
            .run_difficulty_validation_stage(&task, difficulty, &event_tx)
            .await;

        match difficulty_result {
            Ok(result) => {
                report.add_validation(result);
            }
            Err(e) => {
                self.send_event(
                    &event_tx,
                    PipelineEvent::stage_failed(PipelineStage::DifficultyValidation, e.to_string()),
                )
                .await;
                if !self.config.continue_on_failure {
                    return Err(e);
                }
            }
        }

        // Stage 3: Feasibility Validation
        let feasibility_result = self
            .run_feasibility_validation_stage(&task, &event_tx)
            .await;

        match feasibility_result {
            Ok(result) => {
                report.add_validation(result);
            }
            Err(e) => {
                self.send_event(
                    &event_tx,
                    PipelineEvent::stage_failed(
                        PipelineStage::FeasibilityValidation,
                        e.to_string(),
                    ),
                )
                .await;
                if !self.config.continue_on_failure {
                    return Err(e);
                }
            }
        }

        // Stage 4: Final Approval
        let final_result = self
            .run_final_approval_stage(&mut report, &event_tx)
            .await?;
        report.add_validation(final_result);

        // Finalize the report
        let duration_ms = start_time.elapsed().as_millis() as u64;
        let summary = self.generate_summary(&report);
        report.finalize(summary, duration_ms);

        // Send pipeline completed event
        self.send_event(&event_tx, PipelineEvent::pipeline_completed(report.clone()))
            .await;

        Ok(report)
    }

    /// Runs the task generation stage.
    async fn run_generation_stage(
        &self,
        difficulty: DifficultyLevel,
        seed: u64,
        event_tx: &mpsc::Sender<PipelineEvent>,
    ) -> AgentResult<GeneratedTask> {
        self.send_event(
            event_tx,
            PipelineEvent::stage_started(PipelineStage::TaskGeneration),
        )
        .await;

        let result = self.generator.generate_task(difficulty, seed).await;

        match &result {
            Ok(task) => {
                let validation_result = self.generator.create_validation_result(task);
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_completed(
                        PipelineStage::TaskGeneration,
                        validation_result,
                    ),
                )
                .await;
            }
            Err(e) => {
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_failed(PipelineStage::TaskGeneration, e.to_string()),
                )
                .await;
            }
        }

        result
    }

    /// Runs the difficulty validation stage.
    async fn run_difficulty_validation_stage(
        &self,
        task: &GeneratedTask,
        expected_difficulty: DifficultyLevel,
        event_tx: &mpsc::Sender<PipelineEvent>,
    ) -> AgentResult<ValidationResult> {
        self.send_event(
            event_tx,
            PipelineEvent::stage_started(PipelineStage::DifficultyValidation),
        )
        .await;

        let result = self
            .difficulty_validator
            .validate_difficulty(task, expected_difficulty)
            .await;

        match &result {
            Ok(validation_result) => {
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_completed(
                        PipelineStage::DifficultyValidation,
                        validation_result.clone(),
                    ),
                )
                .await;
            }
            Err(e) => {
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_failed(PipelineStage::DifficultyValidation, e.to_string()),
                )
                .await;
            }
        }

        result
    }

    /// Runs the feasibility validation stage.
    async fn run_feasibility_validation_stage(
        &self,
        task: &GeneratedTask,
        event_tx: &mpsc::Sender<PipelineEvent>,
    ) -> AgentResult<ValidationResult> {
        self.send_event(
            event_tx,
            PipelineEvent::stage_started(PipelineStage::FeasibilityValidation),
        )
        .await;

        let result = self.feasibility_validator.validate_feasibility(task).await;

        match &result {
            Ok(validation_result) => {
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_completed(
                        PipelineStage::FeasibilityValidation,
                        validation_result.clone(),
                    ),
                )
                .await;
            }
            Err(e) => {
                self.send_event(
                    event_tx,
                    PipelineEvent::stage_failed(
                        PipelineStage::FeasibilityValidation,
                        e.to_string(),
                    ),
                )
                .await;
            }
        }

        result
    }

    /// Runs the final approval stage.
    async fn run_final_approval_stage(
        &self,
        report: &mut TaskValidationReport,
        event_tx: &mpsc::Sender<PipelineEvent>,
    ) -> AgentResult<ValidationResult> {
        self.send_event(
            event_tx,
            PipelineEvent::stage_started(PipelineStage::FinalApproval),
        )
        .await;

        // Calculate overall score from existing validations
        let scores: Vec<f64> = report
            .validations
            .values()
            .filter_map(|v| v.score())
            .collect();

        let avg_score = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };

        // Check if all validations passed
        let all_passed = report.validations.values().all(|v| v.is_success());

        // Determine final approval
        let passes_threshold = avg_score >= self.config.final_approval_threshold;
        let approved = all_passed && passes_threshold;

        // Build reasoning
        let reasoning = if approved {
            format!(
                "Task approved with overall score {:.2}. All validation stages passed.",
                avg_score
            )
        } else if !all_passed {
            let failed_stages: Vec<_> = report
                .validations
                .iter()
                .filter(|(_, v)| !v.is_success())
                .map(|(name, _)| name.as_str())
                .collect();
            format!(
                "Task not approved. Failed stages: {}. Overall score: {:.2}",
                failed_stages.join(", "),
                avg_score
            )
        } else {
            format!(
                "Task not approved. Overall score {:.2} below threshold {:.2}",
                avg_score, self.config.final_approval_threshold
            )
        };

        // Collect all issues
        let issues: Vec<String> = report
            .validations
            .values()
            .filter_map(|v| v.details().map(|d| d.to_string()))
            .collect();

        let details = if issues.is_empty() {
            None
        } else {
            Some(issues.join("; "))
        };

        let result = if approved {
            ValidationResult::Success {
                message: reasoning,
                details,
                score: Some(avg_score),
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            }
        } else {
            ValidationResult::Failure {
                message: reasoning,
                details,
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            }
        };

        self.send_event(
            event_tx,
            PipelineEvent::stage_completed(PipelineStage::FinalApproval, result.clone()),
        )
        .await;

        Ok(result)
    }

    /// Generates a summary for the validation report.
    fn generate_summary(&self, report: &TaskValidationReport) -> String {
        let passed_count = report
            .validations
            .values()
            .filter(|v| v.is_success())
            .count();
        let total_count = report.validations.len();
        let issues = report.all_issues();

        let mut summary = format!(
            "Validation pipeline completed: {}/{} stages passed. Task ID: {}",
            passed_count, total_count, report.task.task_id
        );

        if !issues.is_empty() {
            summary.push_str(&format!(" Issues found: {}", issues.len()));
        }

        summary
    }

    /// Sends an event through the channel, ignoring send errors.
    async fn send_event(&self, event_tx: &mpsc::Sender<PipelineEvent>, event: PipelineEvent) {
        // Ignore send errors - receiver may have been dropped
        let _ = event_tx.send(event).await;
    }

    /// Returns a reference to the generator agent.
    pub fn generator(&self) -> &GeneratorAgent {
        &self.generator
    }

    /// Returns a reference to the difficulty validator agent.
    pub fn difficulty_validator(&self) -> &DifficultyValidatorAgent {
        &self.difficulty_validator
    }

    /// Returns a reference to the feasibility validator agent.
    pub fn feasibility_validator(&self) -> &FeasibilityValidatorAgent {
        &self.feasibility_validator
    }
}

/// Builder for creating an OrchestratorAgent with fluent API.
pub struct OrchestratorBuilder {
    llm_client: Option<Arc<dyn LlmProvider>>,
    generator_config: Option<GeneratorAgentConfig>,
    difficulty_config: DifficultyValidatorConfig,
    feasibility_config: FeasibilityValidatorConfig,
    final_threshold: f64,
    continue_on_failure: bool,
}

impl OrchestratorBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            llm_client: None,
            generator_config: None,
            difficulty_config: DifficultyValidatorConfig::default(),
            feasibility_config: FeasibilityValidatorConfig::default(),
            final_threshold: 0.7,
            continue_on_failure: false,
        }
    }

    /// Sets the LLM client.
    pub fn llm_client(mut self, client: Arc<dyn LlmProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Sets the generator configuration.
    pub fn generator_config(mut self, config: GeneratorAgentConfig) -> Self {
        self.generator_config = Some(config);
        self
    }

    /// Sets the difficulty validator configuration.
    pub fn difficulty_config(mut self, config: DifficultyValidatorConfig) -> Self {
        self.difficulty_config = config;
        self
    }

    /// Sets the feasibility validator configuration.
    pub fn feasibility_config(mut self, config: FeasibilityValidatorConfig) -> Self {
        self.feasibility_config = config;
        self
    }

    /// Sets the final approval threshold.
    pub fn final_threshold(mut self, threshold: f64) -> Self {
        self.final_threshold = threshold;
        self
    }

    /// Sets whether to continue on failure.
    pub fn continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    /// Builds the OrchestratorAgent.
    pub fn build(self) -> AgentResult<OrchestratorAgent> {
        let llm_client = self
            .llm_client
            .ok_or_else(|| AgentError::ConfigurationError("LLM client is required".to_string()))?;

        let generator_config = self.generator_config.ok_or_else(|| {
            AgentError::ConfigurationError("Generator configuration is required".to_string())
        })?;

        let config = OrchestratorConfig::new(generator_config)
            .with_difficulty_config(self.difficulty_config)
            .with_feasibility_config(self.feasibility_config)
            .with_final_approval_threshold(self.final_threshold)
            .continue_on_failure(self.continue_on_failure);

        Ok(OrchestratorAgent::new(llm_client, config))
    }
}

impl Default for OrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationRequest, GenerationResponse, Message, Usage};
    use crate::template::{DifficultyConfig, TaskTemplate};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Mock LLM provider that returns predetermined responses.
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
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().expect("lock not poisoned");
            let content = responses.get(idx).cloned().unwrap_or_else(|| {
                r#"{"score": 0.8, "matches_difficulty": true, "is_solvable": true, "is_non_trivial": true, "is_clear": true, "reasoning": "Default response"}"#.to_string()
            });

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
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            })
        }
    }

    fn create_test_template() -> TaskTemplate {
        TaskTemplate::new(
            "test-task-001",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Find the error count in the log file.",
            "grep -c 'ERROR' /var/log/app.log",
        )
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let mock_provider = Arc::new(MockLlmProvider::new(vec![]));

        let generator_config =
            GeneratorAgentConfig::new(temp_dir.path()).with_template(create_test_template());

        let config = OrchestratorConfig::new(generator_config);
        let orchestrator = OrchestratorAgent::new(mock_provider, config);

        assert_eq!(orchestrator.generator().template_count(), 1);
    }

    #[tokio::test]
    async fn test_orchestrator_builder() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let mock_provider = Arc::new(MockLlmProvider::new(vec![]));

        let generator_config =
            GeneratorAgentConfig::new(temp_dir.path()).with_template(create_test_template());

        let orchestrator = OrchestratorBuilder::new()
            .llm_client(mock_provider)
            .generator_config(generator_config)
            .final_threshold(0.8)
            .continue_on_failure(true)
            .build()
            .expect("builder should succeed");

        assert_eq!(orchestrator.generator().template_count(), 1);
    }

    #[tokio::test]
    async fn test_builder_missing_llm_client() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let generator_config =
            GeneratorAgentConfig::new(temp_dir.path()).with_template(create_test_template());

        let result = OrchestratorBuilder::new()
            .generator_config(generator_config)
            .build();

        assert!(result.is_err());
        match result {
            Err(AgentError::ConfigurationError(_)) => {}
            _ => panic!("expected ConfigurationError"),
        }
    }

    #[tokio::test]
    async fn test_generate_summary() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let mock_provider = Arc::new(MockLlmProvider::new(vec![]));

        let generator_config =
            GeneratorAgentConfig::new(temp_dir.path()).with_template(create_test_template());

        let config = OrchestratorConfig::new(generator_config);
        let orchestrator = OrchestratorAgent::new(mock_provider, config);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Medium,
            "Test instruction",
        );
        let mut report = TaskValidationReport::new(task);

        report.add_validation(ValidationResult::success_full(
            "Good",
            "Details",
            0.9,
            "test_agent",
        ));

        let summary = orchestrator.generate_summary(&report);

        assert!(summary.contains("1/1 stages passed"));
        assert!(summary.contains("test-123"));
    }
}
