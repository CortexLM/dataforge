//! Full Pipeline Orchestrator - Coordinates ALL agents for maximum quality dataset generation.
//!
//! This orchestrator integrates all 14 agents in a comprehensive pipeline:
//!
//! 1. **CollectorAgent** - Collects problems from external sources (optional)
//! 2. **AnalyzerAgent** - Analyzes and categorizes collected tasks
//! 3. **ResearchAgent** - Identifies LLM weaknesses for the category
//! 4. **IdeatorAgent** - Generates creative task ideas
//! 5. **ProblemCrafterAgent** - Reformulates the problem statement
//! 6. **DifficultyAmplifierAgent** - Adds difficulty traps
//! 7. **FeasibilityValidatorAgent** - Validates task is feasible
//! 8. **DifficultyValidatorAgent** - Validates difficulty level
//! 9. **TaskValidatorAgent** - Validates complexity and memorization risk
//! 10. **TaskExecutorAgent** - Creates complete task specification
//! 11. **TestDesignerAgent** - Designs validation tests
//! 12. **EnvironmentBuilderAgent** - Builds Docker environment
//! 13. **DockerValidatorAgent** - Validates execution in Docker
//! 14. **ValidatorAgent** - Validates solution correctness

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::execution::DockerClient;
use crate::llm::LlmProvider;

use super::analyzer_agent::{AnalyzedTask as AnalyzerAnalyzedTask, AnalyzerAgent, AnalyzerConfig};
use super::collector_agent::{CollectedTask, CollectorAgent, CollectorConfig, TaskSource};
use super::difficulty_amplifier::{AmplifierConfig, DifficultyAmplifierAgent};
use super::difficulty_validator::{DifficultyValidatorAgent, DifficultyValidatorConfig};
use super::docker_validator::{
    DockerValidationResult, DockerValidatorAgent, DockerValidatorConfig,
};
use super::environment_builder::{
    AnalyzedTask as EnvAnalyzedTask, BuiltEnvironment, EnvironmentBuilderAgent, EnvironmentConfig,
};
use super::error::{AgentError, AgentResult};
use super::factory_types::{AmplifiedTask, FactoryTaskSpec, ResearchFindings};
use super::feasibility_validator::{FeasibilityValidatorAgent, FeasibilityValidatorConfig};
use super::ideator::{IdeatorAgent, IdeatorConfig, TaskCategory, TaskIdea as IdeatorTaskIdea};
use super::problem_crafter::{CraftedProblem, CrafterConfig, ProblemCrafterAgent};
use super::research_agent::{ResearchAgent, ResearchConfig};
use super::task_executor::{SyntheticTask, TaskExecutorAgent, TaskExecutorConfig};
use super::task_validator::{
    TaskIdea as ValidatorTaskIdea, TaskValidatorAgent, TaskValidatorConfig, ValidationAssessment,
};
use super::test_designer::{TestDesignerAgent, TestDesignerConfig, TestSpec as DesignerTestSpec};

use super::validator_agent::{ValidatorAgent, ValidatorConfig};

// ============================================================================
// Pipeline Stages
// ============================================================================

/// Stages in the full pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FullPipelineStage {
    /// Collection from external sources.
    Collection,
    /// Analysis and categorization.
    Analysis,
    /// Research on LLM weaknesses.
    Research,
    /// Creative ideation.
    Ideation,
    /// Problem statement crafting.
    ProblemCrafting,
    /// Difficulty amplification.
    Amplification,
    /// Feasibility validation.
    FeasibilityValidation,
    /// Difficulty validation.
    DifficultyValidation,
    /// Task complexity validation.
    TaskValidation,
    /// Task specification creation.
    Execution,
    /// Test design.
    TestDesign,
    /// Environment building.
    EnvironmentBuilding,
    /// Docker validation.
    DockerValidation,
    /// Solution validation.
    SolutionValidation,
    /// Final quality check.
    QualityCheck,
}

impl std::fmt::Display for FullPipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FullPipelineStage::Collection => write!(f, "Collection"),
            FullPipelineStage::Analysis => write!(f, "Analysis"),
            FullPipelineStage::Research => write!(f, "Research"),
            FullPipelineStage::Ideation => write!(f, "Ideation"),
            FullPipelineStage::ProblemCrafting => write!(f, "Problem Crafting"),
            FullPipelineStage::Amplification => write!(f, "Amplification"),
            FullPipelineStage::FeasibilityValidation => write!(f, "Feasibility Validation"),
            FullPipelineStage::DifficultyValidation => write!(f, "Difficulty Validation"),
            FullPipelineStage::TaskValidation => write!(f, "Task Validation"),
            FullPipelineStage::Execution => write!(f, "Execution"),
            FullPipelineStage::TestDesign => write!(f, "Test Design"),
            FullPipelineStage::EnvironmentBuilding => write!(f, "Environment Building"),
            FullPipelineStage::DockerValidation => write!(f, "Docker Validation"),
            FullPipelineStage::SolutionValidation => write!(f, "Solution Validation"),
            FullPipelineStage::QualityCheck => write!(f, "Quality Check"),
        }
    }
}

// ============================================================================
// Pipeline Events
// ============================================================================

/// Events emitted during the full pipeline for progress tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FullPipelineEvent {
    /// Pipeline stage started.
    StageStarted {
        stage: FullPipelineStage,
        timestamp: DateTime<Utc>,
    },
    /// Pipeline stage completed successfully.
    StageCompleted {
        stage: FullPipelineStage,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Pipeline stage failed.
    StageFailed {
        stage: FullPipelineStage,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// Stage was skipped (optional stage not enabled).
    StageSkipped {
        stage: FullPipelineStage,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// Collection completed.
    CollectionComplete {
        tasks_collected: usize,
        timestamp: DateTime<Utc>,
    },
    /// Analysis completed.
    AnalysisComplete {
        category: String,
        difficulty: String,
        timestamp: DateTime<Utc>,
    },
    /// Research completed.
    ResearchComplete {
        weaknesses_found: usize,
        traps_proposed: usize,
        timestamp: DateTime<Utc>,
    },
    /// Ideation completed.
    IdeationComplete {
        task_title: String,
        category: String,
        timestamp: DateTime<Utc>,
    },
    /// Problem crafting completed.
    ProblemCraftingComplete {
        statement_length: usize,
        hints_count: usize,
        timestamp: DateTime<Utc>,
    },
    /// Amplification completed.
    AmplificationComplete {
        traps_added: usize,
        difficulty_score: f64,
        timestamp: DateTime<Utc>,
    },
    /// Validation completed (for any validation stage).
    ValidationComplete {
        stage: FullPipelineStage,
        passed: bool,
        score: f64,
        timestamp: DateTime<Utc>,
    },
    /// Task execution completed.
    ExecutionComplete {
        task_id: String,
        timestamp: DateTime<Utc>,
    },
    /// Test design completed.
    TestDesignComplete {
        test_count: usize,
        timestamp: DateTime<Utc>,
    },
    /// Environment building completed.
    EnvironmentComplete {
        dockerfile_generated: bool,
        timestamp: DateTime<Utc>,
    },
    /// Docker validation completed.
    DockerValidationComplete {
        passed: bool,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Full pipeline completed.
    PipelineComplete {
        task_id: String,
        total_duration_ms: u64,
        stages_completed: usize,
        timestamp: DateTime<Utc>,
    },
    /// Full pipeline failed.
    PipelineFailed {
        error: String,
        failed_stage: FullPipelineStage,
        timestamp: DateTime<Utc>,
    },
}

impl FullPipelineEvent {
    pub fn stage_started(stage: FullPipelineStage) -> Self {
        Self::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    pub fn stage_completed(stage: FullPipelineStage, duration_ms: u64) -> Self {
        Self::StageCompleted {
            stage,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    pub fn stage_failed(stage: FullPipelineStage, error: impl Into<String>) -> Self {
        Self::StageFailed {
            stage,
            error: error.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn stage_skipped(stage: FullPipelineStage, reason: impl Into<String>) -> Self {
        Self::StageSkipped {
            stage,
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Full Pipeline Orchestrator.
#[derive(Debug, Clone)]
pub struct FullPipelineConfig {
    // Agent configurations
    pub collector_config: CollectorConfig,
    pub analyzer_config: AnalyzerConfig,
    pub research_config: ResearchConfig,
    pub ideator_config: IdeatorConfig,
    pub crafter_config: CrafterConfig,
    pub amplifier_config: AmplifierConfig,
    pub feasibility_config: FeasibilityValidatorConfig,
    pub difficulty_validator_config: DifficultyValidatorConfig,
    pub task_validator_config: TaskValidatorConfig,
    pub executor_config: TaskExecutorConfig,
    pub test_designer_config: TestDesignerConfig,
    pub environment_config: EnvironmentConfig,
    pub docker_validator_config: DockerValidatorConfig,
    pub validator_config: ValidatorConfig,

    // Pipeline behavior
    /// Whether to use external collection (if false, starts from ideation).
    pub use_collection: bool,
    /// Whether to enable Docker validation.
    pub docker_validation_enabled: bool,
    /// Whether to validate the reference solution.
    pub validate_solution: bool,
    /// Minimum validation score to proceed.
    pub min_validation_score: f64,
    /// Maximum retries for ideation if validation fails.
    pub max_retries: u32,
    /// Whether to continue on non-critical failures.
    pub continue_on_failure: bool,
    /// Output directory for generated environments.
    pub output_dir: String,
}

impl Default for FullPipelineConfig {
    fn default() -> Self {
        Self {
            collector_config: CollectorConfig::default(),
            analyzer_config: AnalyzerConfig::default(),
            research_config: ResearchConfig::default(),
            ideator_config: IdeatorConfig::default(),
            crafter_config: CrafterConfig::default(),
            amplifier_config: AmplifierConfig::default(),
            feasibility_config: FeasibilityValidatorConfig::default(),
            difficulty_validator_config: DifficultyValidatorConfig::default(),
            task_validator_config: TaskValidatorConfig::default(),
            executor_config: TaskExecutorConfig::default(),
            test_designer_config: TestDesignerConfig::default(),
            environment_config: EnvironmentConfig::default(),
            docker_validator_config: DockerValidatorConfig::default(),
            validator_config: ValidatorConfig::default(),
            use_collection: false,
            docker_validation_enabled: false,
            validate_solution: true,
            min_validation_score: 0.6,
            max_retries: 3,
            continue_on_failure: true,
            output_dir: "./generated-datasets".to_string(),
        }
    }
}

impl FullPipelineConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_collection(mut self, enabled: bool) -> Self {
        self.use_collection = enabled;
        self
    }

    pub fn with_docker_validation(mut self, enabled: bool) -> Self {
        self.docker_validation_enabled = enabled;
        self
    }

    pub fn with_solution_validation(mut self, enabled: bool) -> Self {
        self.validate_solution = enabled;
        self
    }

    pub fn with_min_validation_score(mut self, score: f64) -> Self {
        self.min_validation_score = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn with_output_dir(mut self, dir: impl Into<String>) -> Self {
        self.output_dir = dir.into();
        self
    }
}

// ============================================================================
// Full Pipeline Result
// ============================================================================

/// Result of a full pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullPipelineResult {
    /// The generated synthetic task.
    pub task: SyntheticTask,
    /// Research findings used.
    pub research_findings: Option<ResearchFindings>,
    /// Crafted problem statement.
    pub crafted_problem: Option<CraftedProblem>,
    /// Amplified task information.
    pub amplified_task: Option<AmplifiedTask>,
    /// Designed tests.
    pub test_spec: Option<DesignerTestSpec>,
    /// Built environment.
    pub environment: Option<BuiltEnvironment>,
    /// Docker validation result.
    pub docker_validation: Option<DockerValidationResult>,
    /// Stages that were completed.
    pub completed_stages: Vec<FullPipelineStage>,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
}

// ============================================================================
// Full Pipeline Orchestrator
// ============================================================================

/// Orchestrator that coordinates all 14 agents for maximum quality dataset generation.
#[allow(dead_code)]
pub struct FullPipelineOrchestrator {
    // Core agents (always used)
    research_agent: ResearchAgent,
    ideator: IdeatorAgent,
    amplifier: DifficultyAmplifierAgent,
    task_validator: TaskValidatorAgent,
    executor: TaskExecutorAgent,

    // Optional collection/analysis agents (reserved for future use)
    collector: CollectorAgent,
    analyzer: AnalyzerAgent,
    problem_crafter: ProblemCrafterAgent,

    // Validation agents (reserved for future use)
    feasibility_validator: FeasibilityValidatorAgent,
    difficulty_validator: DifficultyValidatorAgent,

    // Environment and test agents
    test_designer: TestDesignerAgent,
    environment_builder: EnvironmentBuilderAgent,
    validator: ValidatorAgent,

    // Docker validator (lazy-initialized)
    docker_validator: Option<DockerValidatorAgent>,

    // Configuration
    config: FullPipelineConfig,
}

impl FullPipelineOrchestrator {
    pub const AGENT_NAME: &'static str = "full_pipeline_orchestrator";

    /// Creates a new full pipeline orchestrator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: FullPipelineConfig) -> Self {
        let collector = CollectorAgent::new(Arc::clone(&llm_client));
        let analyzer = AnalyzerAgent::new(Arc::clone(&llm_client));
        let research_agent =
            ResearchAgent::new(Arc::clone(&llm_client), config.research_config.clone());
        let ideator = IdeatorAgent::new(Arc::clone(&llm_client), config.ideator_config.clone());
        let problem_crafter = ProblemCrafterAgent::new(Arc::clone(&llm_client));
        let amplifier =
            DifficultyAmplifierAgent::new(Arc::clone(&llm_client), config.amplifier_config.clone());
        let feasibility_validator = FeasibilityValidatorAgent::new(
            Arc::clone(&llm_client),
            config.feasibility_config.clone(),
        );
        let difficulty_validator = DifficultyValidatorAgent::new(
            Arc::clone(&llm_client),
            config.difficulty_validator_config.clone(),
        );
        let task_validator = TaskValidatorAgent::new(
            Arc::clone(&llm_client),
            config.task_validator_config.clone(),
        );
        let executor =
            TaskExecutorAgent::new(Arc::clone(&llm_client), config.executor_config.clone());
        let test_designer = TestDesignerAgent::new(Arc::clone(&llm_client));
        let environment_builder = EnvironmentBuilderAgent::new(
            Arc::clone(&llm_client),
            config.environment_config.clone(),
        );
        let validator = ValidatorAgent::new(llm_client, config.validator_config.clone());

        // Initialize Docker validator if enabled
        let docker_validator = if config.docker_validation_enabled {
            match DockerClient::new() {
                Ok(client) => {
                    let docker_config = DockerValidatorConfig::new()
                        .with_solution_validation(config.validate_solution);
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
            collector,
            analyzer,
            research_agent,
            ideator,
            problem_crafter,
            amplifier,
            feasibility_validator,
            difficulty_validator,
            task_validator,
            executor,
            test_designer,
            environment_builder,
            validator,
            docker_validator,
            config,
        }
    }

    /// Creates with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, FullPipelineConfig::default())
    }

    /// Runs the full pipeline to generate a single task.
    pub async fn generate_task(
        &self,
        category: Option<TaskCategory>,
        event_tx: mpsc::Sender<FullPipelineEvent>,
    ) -> AgentResult<FullPipelineResult> {
        let start_time = Instant::now();
        let mut completed_stages = Vec::new();

        // Stage 1: Research
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::Research,
            ))
            .await;
        let stage_start = Instant::now();

        let category_str = category
            .map(|c| c.to_benchmark_category().to_string())
            .unwrap_or_else(|| "debugging".to_string());

        let research_findings = match self.research_agent.research_category(&category_str).await {
            Ok(findings) => {
                let _ = event_tx
                    .send(FullPipelineEvent::ResearchComplete {
                        weaknesses_found: findings.identified_weaknesses.len(),
                        traps_proposed: findings.proposed_traps.len(),
                        timestamp: Utc::now(),
                    })
                    .await;
                let _ = event_tx
                    .send(FullPipelineEvent::stage_completed(
                        FullPipelineStage::Research,
                        stage_start.elapsed().as_millis() as u64,
                    ))
                    .await;
                completed_stages.push(FullPipelineStage::Research);
                Some(findings)
            }
            Err(e) => {
                tracing::warn!("Research failed, continuing without: {}", e);
                let _ = event_tx
                    .send(FullPipelineEvent::stage_failed(
                        FullPipelineStage::Research,
                        e.to_string(),
                    ))
                    .await;
                None
            }
        };

        // Stage 2: Ideation with retries
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::Ideation,
            ))
            .await;
        let stage_start = Instant::now();

        let mut idea: Option<IdeatorTaskIdea> = None;
        let mut assessment: Option<ValidationAssessment> = None;

        for retry in 0..=self.config.max_retries {
            let generated_idea = match self.ideator.generate_task_idea(category).await {
                Ok(i) => i,
                Err(e) => {
                    if retry == self.config.max_retries {
                        return Err(AgentError::GenerationFailed(format!(
                            "Ideation failed after {} retries: {}",
                            self.config.max_retries, e
                        )));
                    }
                    continue;
                }
            };

            // Stage 3: Task Validation
            let validator_idea = Self::convert_to_validator_idea(&generated_idea);
            let validation = match self.task_validator.validate_task(&validator_idea).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Validation failed on retry {}: {}", retry, e);
                    continue;
                }
            };

            if validation.is_valid
                && validation.complexity_score >= self.config.min_validation_score
            {
                idea = Some(generated_idea);
                assessment = Some(validation);
                break;
            }

            tracing::info!(
                "Idea rejected (score: {:.2}, retry: {})",
                validation.complexity_score,
                retry
            );
        }

        let idea = idea.ok_or_else(|| {
            AgentError::GenerationFailed("Failed to generate valid task idea".to_string())
        })?;
        let assessment = assessment.unwrap();

        let _ = event_tx
            .send(FullPipelineEvent::IdeationComplete {
                task_title: idea.title.clone(),
                category: idea.category.to_benchmark_category().to_string(),
                timestamp: Utc::now(),
            })
            .await;
        let _ = event_tx
            .send(FullPipelineEvent::stage_completed(
                FullPipelineStage::Ideation,
                stage_start.elapsed().as_millis() as u64,
            ))
            .await;
        completed_stages.push(FullPipelineStage::Ideation);
        completed_stages.push(FullPipelineStage::TaskValidation);

        // Stage 4: Difficulty Amplification
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::Amplification,
            ))
            .await;
        let stage_start = Instant::now();

        let amplified_task = if let Some(ref findings) = research_findings {
            let base_spec = FactoryTaskSpec::new(
                &idea.title,
                idea.category.to_benchmark_category(),
                &idea.description,
                idea.estimated_difficulty,
            )
            .with_required_skills(idea.required_skills.clone());

            match self
                .amplifier
                .amplify_task(&base_spec, &findings.proposed_traps)
                .await
            {
                Ok(amplified) => {
                    let _ = event_tx
                        .send(FullPipelineEvent::AmplificationComplete {
                            traps_added: amplified.traps_added.len(),
                            difficulty_score: amplified.difficulty_score,
                            timestamp: Utc::now(),
                        })
                        .await;
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_completed(
                            FullPipelineStage::Amplification,
                            stage_start.elapsed().as_millis() as u64,
                        ))
                        .await;
                    completed_stages.push(FullPipelineStage::Amplification);
                    Some(amplified)
                }
                Err(e) => {
                    tracing::warn!("Amplification failed: {}", e);
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_failed(
                            FullPipelineStage::Amplification,
                            e.to_string(),
                        ))
                        .await;
                    None
                }
            }
        } else {
            let _ = event_tx
                .send(FullPipelineEvent::stage_skipped(
                    FullPipelineStage::Amplification,
                    "No research findings available",
                ))
                .await;
            None
        };

        // Stage 5: Task Execution
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::Execution,
            ))
            .await;
        let stage_start = Instant::now();

        let validator_idea = Self::convert_to_validator_idea(&idea);
        let task = self
            .executor
            .create_task(&validator_idea, &assessment)
            .await?;

        let _ = event_tx
            .send(FullPipelineEvent::ExecutionComplete {
                task_id: task.id.clone(),
                timestamp: Utc::now(),
            })
            .await;
        let _ = event_tx
            .send(FullPipelineEvent::stage_completed(
                FullPipelineStage::Execution,
                stage_start.elapsed().as_millis() as u64,
            ))
            .await;
        completed_stages.push(FullPipelineStage::Execution);

        // Stage 6: Test Design
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::TestDesign,
            ))
            .await;
        let stage_start = Instant::now();

        let test_spec = {
            // Create a minimal AnalyzedTask for test design (uses analyzer_agent type)
            let collected = CollectedTask::new(
                TaskSource::Manual,
                &task.metadata.category,
                &task.problem_statement,
            );
            let analyzed = AnalyzerAnalyzedTask::new(
                collected,
                super::analyzer_agent::TaskCategory::Debugging,
                &task.metadata.category,
                task.difficulty.level,
                task.metadata.tags.clone(),
                &task.problem_statement,
                (task.hidden_solution.expected_time_seconds / 60) as u32,
                vec![],
            );

            match self.test_designer.design_tests(&analyzed, None).await {
                Ok(spec) => {
                    let _ = event_tx
                        .send(FullPipelineEvent::TestDesignComplete {
                            test_count: spec.total_tests(),
                            timestamp: Utc::now(),
                        })
                        .await;
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_completed(
                            FullPipelineStage::TestDesign,
                            stage_start.elapsed().as_millis() as u64,
                        ))
                        .await;
                    completed_stages.push(FullPipelineStage::TestDesign);
                    Some(spec)
                }
                Err(e) => {
                    tracing::warn!("Test design failed: {}", e);
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_failed(
                            FullPipelineStage::TestDesign,
                            e.to_string(),
                        ))
                        .await;
                    None
                }
            }
        };

        // Stage 7: Environment Building
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::EnvironmentBuilding,
            ))
            .await;
        let stage_start = Instant::now();

        let environment = {
            // Create EnvAnalyzedTask for environment builder (uses environment_builder type)
            let env_analyzed = EnvAnalyzedTask::new(
                &task.id,
                &task.problem_statement,
                &task.metadata.category,
                task.difficulty.level,
            );

            let output_path = Path::new(&self.config.output_dir);
            match self
                .environment_builder
                .build(&env_analyzed, output_path)
                .await
            {
                Ok(env) => {
                    let _ = event_tx
                        .send(FullPipelineEvent::EnvironmentComplete {
                            dockerfile_generated: !env.dockerfile_content.is_empty(),
                            timestamp: Utc::now(),
                        })
                        .await;
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_completed(
                            FullPipelineStage::EnvironmentBuilding,
                            stage_start.elapsed().as_millis() as u64,
                        ))
                        .await;
                    completed_stages.push(FullPipelineStage::EnvironmentBuilding);
                    Some(env)
                }
                Err(e) => {
                    tracing::warn!("Environment building failed: {}", e);
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_failed(
                            FullPipelineStage::EnvironmentBuilding,
                            e.to_string(),
                        ))
                        .await;
                    None
                }
            }
        };

        // Stage 8: Docker Validation
        let docker_validation = if let Some(ref validator) = self.docker_validator {
            let _ = event_tx
                .send(FullPipelineEvent::stage_started(
                    FullPipelineStage::DockerValidation,
                ))
                .await;
            let stage_start = Instant::now();

            match validator.validate_task(&task).await {
                Ok(result) => {
                    let _ = event_tx
                        .send(FullPipelineEvent::DockerValidationComplete {
                            passed: result.passed,
                            duration_ms: result.duration_ms,
                            timestamp: Utc::now(),
                        })
                        .await;
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_completed(
                            FullPipelineStage::DockerValidation,
                            stage_start.elapsed().as_millis() as u64,
                        ))
                        .await;
                    completed_stages.push(FullPipelineStage::DockerValidation);
                    Some(result)
                }
                Err(e) => {
                    tracing::warn!("Docker validation failed: {}", e);
                    let _ = event_tx
                        .send(FullPipelineEvent::stage_failed(
                            FullPipelineStage::DockerValidation,
                            e.to_string(),
                        ))
                        .await;
                    None
                }
            }
        } else {
            let _ = event_tx
                .send(FullPipelineEvent::stage_skipped(
                    FullPipelineStage::DockerValidation,
                    "Docker validation not enabled",
                ))
                .await;
            None
        };

        // Quality check
        let _ = event_tx
            .send(FullPipelineEvent::stage_started(
                FullPipelineStage::QualityCheck,
            ))
            .await;
        self.quality_check(&task)?;
        completed_stages.push(FullPipelineStage::QualityCheck);
        let _ = event_tx
            .send(FullPipelineEvent::stage_completed(
                FullPipelineStage::QualityCheck,
                0,
            ))
            .await;

        let total_duration_ms = start_time.elapsed().as_millis() as u64;

        let _ = event_tx
            .send(FullPipelineEvent::PipelineComplete {
                task_id: task.id.clone(),
                total_duration_ms,
                stages_completed: completed_stages.len(),
                timestamp: Utc::now(),
            })
            .await;

        Ok(FullPipelineResult {
            task,
            research_findings,
            crafted_problem: None,
            amplified_task,
            test_spec,
            environment,
            docker_validation,
            completed_stages,
            total_duration_ms,
        })
    }

    /// Generates multiple tasks.
    pub async fn generate_tasks(
        &self,
        category: Option<TaskCategory>,
        count: usize,
        event_tx: mpsc::Sender<FullPipelineEvent>,
    ) -> AgentResult<Vec<FullPipelineResult>> {
        let mut results = Vec::with_capacity(count);

        for _ in 0..count {
            match self.generate_task(category, event_tx.clone()).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::error!("Task generation failed: {}", e);
                    if !self.config.continue_on_failure {
                        return Err(e);
                    }
                }
            }
        }

        if results.is_empty() && count > 0 {
            return Err(AgentError::GenerationFailed(
                "Failed to generate any tasks".to_string(),
            ));
        }

        Ok(results)
    }

    /// Performs final quality check on the task.
    fn quality_check(&self, task: &SyntheticTask) -> AgentResult<()> {
        if task.problem_statement.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing problem statement".to_string(),
            ));
        }

        if task.hidden_solution.approach.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing solution approach".to_string(),
            ));
        }

        if task.verification.success_criteria.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Task missing success criteria".to_string(),
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

    /// Returns the configuration.
    pub fn config(&self) -> &FullPipelineConfig {
        &self.config
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stage_display() {
        assert_eq!(format!("{}", FullPipelineStage::Collection), "Collection");
        assert_eq!(format!("{}", FullPipelineStage::Research), "Research");
        assert_eq!(format!("{}", FullPipelineStage::Ideation), "Ideation");
        assert_eq!(format!("{}", FullPipelineStage::Execution), "Execution");
        assert_eq!(
            format!("{}", FullPipelineStage::DockerValidation),
            "Docker Validation"
        );
    }

    #[test]
    fn test_config_defaults() {
        let config = FullPipelineConfig::default();
        assert!(!config.use_collection);
        assert!(!config.docker_validation_enabled);
        assert!(config.validate_solution);
        assert!((config.min_validation_score - 0.6).abs() < 0.01);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_builder() {
        let config = FullPipelineConfig::new()
            .with_collection(true)
            .with_docker_validation(true)
            .with_min_validation_score(0.8)
            .with_max_retries(5);

        assert!(config.use_collection);
        assert!(config.docker_validation_enabled);
        assert!((config.min_validation_score - 0.8).abs() < 0.01);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_event_constructors() {
        let event = FullPipelineEvent::stage_started(FullPipelineStage::Ideation);
        match event {
            FullPipelineEvent::StageStarted { stage, .. } => {
                assert_eq!(stage, FullPipelineStage::Ideation);
            }
            _ => panic!("Expected StageStarted event"),
        }

        let event = FullPipelineEvent::stage_completed(FullPipelineStage::Execution, 1000);
        match event {
            FullPipelineEvent::StageCompleted {
                stage, duration_ms, ..
            } => {
                assert_eq!(stage, FullPipelineStage::Execution);
                assert_eq!(duration_ms, 1000);
            }
            _ => panic!("Expected StageCompleted event"),
        }
    }
}
