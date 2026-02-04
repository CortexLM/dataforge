//! Factory Orchestrator - Coordinates multi-agent benchmark task generation.
//!
//! This orchestrator coordinates the factory pipeline for generating challenging
//! synthetic benchmark tasks:
//!
//! 1. **Research Agent** - Identifies LLM weaknesses for the category
//! 2. **Task Creator** (existing IdeatorAgent) - Creates initial task
//! 3. **Difficulty Amplifier** - Makes task harder with traps
//! 4. **Task Validator** (existing) - Validates task is challenging
//! 5. **Task Executor** (existing) - Creates full specification
//!
//! # Example
//!
//! ```ignore
//! use dataforge::agents::factory_orchestrator::{FactoryOrchestrator, FactoryOrchestratorConfig};
//! use dataforge::llm::LiteLlmClient;
//! use std::sync::Arc;
//! use tokio::sync::mpsc;
//!
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//! let config = FactoryOrchestratorConfig::default();
//! let orchestrator = FactoryOrchestrator::new(llm_client, config);
//!
//! let (event_tx, mut event_rx) = mpsc::channel(100);
//! let tasks = orchestrator.run_factory_pipeline(
//!     Some("debugging"),
//!     3,
//!     event_tx,
//! ).await?;
//!
//! println!("Generated {} tasks", tasks.len());
//! ```

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::execution::DockerClient;
use crate::llm::LlmProvider;

use super::difficulty_amplifier::{AmplifierConfig, DifficultyAmplifierAgent};
use super::docker_validator::{DockerValidatorAgent, DockerValidatorConfig};
use super::error::{AgentError, AgentResult};
use super::factory_types::{
    AgentConversation, AmplifiedTask, ConversationTurn, FactoryPipelineEvent, FactoryPipelineStage,
    FactoryTaskSpec, LlmWeaknessType, ResearchFindings,
};
use super::ideator::{IdeatorAgent, IdeatorConfig, TaskCategory, TaskIdea as IdeatorTaskIdea};
use super::research_agent::{ResearchAgent, ResearchConfig};
use super::task_executor::{SyntheticTask, TaskExecutorAgent, TaskExecutorConfig};
use super::task_validator::{
    TaskIdea as ValidatorTaskIdea, TaskValidatorAgent, TaskValidatorConfig,
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Factory Orchestrator.
#[derive(Debug, Clone)]
pub struct FactoryOrchestratorConfig {
    /// Configuration for the research agent.
    pub research_config: ResearchConfig,
    /// Configuration for the ideator agent.
    pub ideator_config: IdeatorConfig,
    /// Configuration for the difficulty amplifier agent.
    pub amplifier_config: AmplifierConfig,
    /// Configuration for the task validator agent.
    pub validator_config: TaskValidatorConfig,
    /// Configuration for the task executor agent.
    pub executor_config: TaskExecutorConfig,
    /// Minimum validation score to proceed to execution.
    pub min_validation_score: f64,
    /// Maximum retries for task creation if validation fails.
    pub max_creation_retries: u32,
    /// Whether to continue generating if one task fails.
    pub continue_on_failure: bool,
    /// Whether to cache research findings between tasks of the same category.
    pub cache_research: bool,
    /// Whether to validate tasks in Docker containers.
    pub docker_validation_enabled: bool,
    /// Whether to validate the reference solution in Docker.
    pub docker_validate_solution: bool,
}

impl Default for FactoryOrchestratorConfig {
    fn default() -> Self {
        Self {
            research_config: ResearchConfig::default(),
            ideator_config: IdeatorConfig::default(),
            amplifier_config: AmplifierConfig::default(),
            validator_config: TaskValidatorConfig::default(),
            executor_config: TaskExecutorConfig::default(),
            min_validation_score: 0.7,
            max_creation_retries: 3,
            continue_on_failure: true,
            cache_research: true,
            docker_validation_enabled: false,
            docker_validate_solution: true,
        }
    }
}

impl FactoryOrchestratorConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the research configuration.
    pub fn with_research_config(mut self, config: ResearchConfig) -> Self {
        self.research_config = config;
        self
    }

    /// Sets the ideator configuration.
    pub fn with_ideator_config(mut self, config: IdeatorConfig) -> Self {
        self.ideator_config = config;
        self
    }

    /// Sets the amplifier configuration.
    pub fn with_amplifier_config(mut self, config: AmplifierConfig) -> Self {
        self.amplifier_config = config;
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

    /// Sets the maximum creation retries.
    pub fn with_max_creation_retries(mut self, retries: u32) -> Self {
        self.max_creation_retries = retries;
        self
    }

    /// Sets whether to continue on failure during batch generation.
    pub fn with_continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    /// Sets whether to cache research findings.
    pub fn with_cache_research(mut self, cache: bool) -> Self {
        self.cache_research = cache;
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
// Research Cache
// ============================================================================

/// Cache for research findings to avoid redundant LLM calls.
#[derive(Debug, Default)]
struct ResearchCache {
    findings: std::collections::HashMap<String, ResearchFindings>,
}

impl ResearchCache {
    fn new() -> Self {
        Self::default()
    }

    fn get(&self, category: &str) -> Option<&ResearchFindings> {
        self.findings.get(category)
    }

    fn insert(&mut self, category: String, findings: ResearchFindings) {
        self.findings.insert(category, findings);
    }
}

// ============================================================================
// Factory Orchestrator
// ============================================================================

/// Orchestrator that coordinates the factory multi-agent pipeline.
///
/// The pipeline stages are:
/// 1. **Research**: Identify LLM weaknesses for the target category
/// 2. **Creation**: Generate creative task idea using ideator
/// 3. **Amplification**: Add difficulty traps based on research findings
/// 4. **Validation**: Validate task meets quality and difficulty requirements
/// 5. **Docker Validation**: Validate task runs in Docker container (optional)
/// 6. **Finalization**: Create complete task specification
pub struct FactoryOrchestrator {
    /// The research agent for weakness identification.
    research_agent: ResearchAgent,
    /// The ideator agent for task idea generation.
    ideator: IdeatorAgent,
    /// The difficulty amplifier agent.
    amplifier: DifficultyAmplifierAgent,
    /// The validator agent for task validation.
    validator: TaskValidatorAgent,
    /// The executor agent for task specification creation.
    executor: TaskExecutorAgent,
    /// Docker validator agent (lazy-initialized).
    docker_validator: Option<DockerValidatorAgent>,
    /// Orchestrator configuration.
    config: FactoryOrchestratorConfig,
    /// Cache for research findings.
    research_cache: std::sync::Mutex<ResearchCache>,
    /// Conversation history for multi-agent interactions.
    conversations: std::sync::Mutex<Vec<AgentConversation>>,
}

impl FactoryOrchestrator {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "factory_orchestrator";

    /// Creates a new factory orchestrator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: FactoryOrchestratorConfig) -> Self {
        let research_agent =
            ResearchAgent::new(Arc::clone(&llm_client), config.research_config.clone());
        let ideator = IdeatorAgent::new(Arc::clone(&llm_client), config.ideator_config.clone());
        let amplifier =
            DifficultyAmplifierAgent::new(Arc::clone(&llm_client), config.amplifier_config.clone());
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
            research_agent,
            ideator,
            amplifier,
            validator,
            executor,
            docker_validator,
            config,
            research_cache: std::sync::Mutex::new(ResearchCache::new()),
            conversations: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Creates a new orchestrator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, FactoryOrchestratorConfig::default())
    }

    /// Runs the complete factory pipeline to generate tasks.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to focus on. If None, categories are cycled.
    /// * `count` - Number of tasks to generate.
    /// * `event_tx` - Channel sender for pipeline events.
    ///
    /// # Returns
    ///
    /// A vector of successfully generated `SyntheticTask` instances.
    pub async fn run_factory_pipeline(
        &self,
        category: Option<&str>,
        count: u32,
        event_tx: mpsc::Sender<FactoryPipelineEvent>,
    ) -> AgentResult<Vec<SyntheticTask>> {
        let start_time = Instant::now();
        let mut tasks = Vec::with_capacity(count as usize);
        let available_categories = TaskCategory::all();

        for i in 0..count {
            // Determine category for this task
            let task_category = match category {
                Some(cat) => self.parse_category(cat),
                None => {
                    let idx = i as usize % available_categories.len();
                    available_categories[idx]
                }
            };

            match self.generate_single_task(task_category, &event_tx).await {
                Ok(task) => tasks.push(task),
                Err(e) => {
                    tracing::warn!(
                        "Failed to generate task {} for category {:?}: {}",
                        i,
                        task_category,
                        e
                    );
                    self.send_event(
                        &event_tx,
                        FactoryPipelineEvent::pipeline_failed(
                            e.to_string(),
                            FactoryPipelineStage::Creation,
                        ),
                    )
                    .await;

                    if !self.config.continue_on_failure {
                        return Err(e);
                    }
                }
            }
        }

        if tasks.is_empty() && count > 0 {
            return Err(AgentError::GenerationFailed(
                "Failed to generate any tasks in factory pipeline".to_string(),
            ));
        }

        let total_duration = start_time.elapsed().as_millis() as u64;
        self.send_event(
            &event_tx,
            FactoryPipelineEvent::pipeline_complete(tasks.len(), total_duration),
        )
        .await;

        Ok(tasks)
    }

    /// Generates a single task through the complete pipeline.
    async fn generate_single_task(
        &self,
        category: TaskCategory,
        event_tx: &mpsc::Sender<FactoryPipelineEvent>,
    ) -> AgentResult<SyntheticTask> {
        let category_str = category.to_benchmark_category();
        let mut retries = 0u32;

        // Start a new conversation context
        let mut conversation = AgentConversation::new(
            format!("Factory pipeline for {}", category_str),
            vec![
                "orchestrator".to_string(),
                "research_agent".to_string(),
                "ideator".to_string(),
                "amplifier".to_string(),
                "validator".to_string(),
                "executor".to_string(),
            ],
        );

        // Stage 1: Research
        self.send_event(
            event_tx,
            FactoryPipelineEvent::stage_started(FactoryPipelineStage::Research),
        )
        .await;

        let research_findings = self.get_or_conduct_research(category_str, event_tx).await?;

        conversation.add_turn(ConversationTurn::assistant(
            "research_agent",
            format!(
                "Found {} weaknesses and {} potential traps for category {}",
                research_findings.identified_weaknesses.len(),
                research_findings.proposed_traps.len(),
                category_str
            ),
        ));

        self.send_event(
            event_tx,
            FactoryPipelineEvent::research_complete(
                research_findings.identified_weaknesses.len(),
                research_findings.proposed_traps.len(),
            ),
        )
        .await;

        // Loop for creation with validation retry
        loop {
            // Stage 2: Creation (Ideation)
            self.send_event(
                event_tx,
                FactoryPipelineEvent::stage_started(FactoryPipelineStage::Creation),
            )
            .await;

            let idea = match self.ideator.generate_task_idea(Some(category)).await {
                Ok(idea) => idea,
                Err(e) => {
                    self.send_event(
                        event_tx,
                        FactoryPipelineEvent::pipeline_failed(
                            e.to_string(),
                            FactoryPipelineStage::Creation,
                        ),
                    )
                    .await;
                    return Err(e);
                }
            };

            conversation.add_turn(ConversationTurn::assistant(
                "ideator",
                format!("Created task idea: {}", idea.title),
            ));

            self.send_event(
                event_tx,
                FactoryPipelineEvent::creation_complete(&idea.title, category_str),
            )
            .await;

            self.send_event(
                event_tx,
                FactoryPipelineEvent::agent_conversation(
                    "ideator",
                    format!("Task: {}", idea.title),
                ),
            )
            .await;

            // Stage 3: Amplification
            self.send_event(
                event_tx,
                FactoryPipelineEvent::stage_started(FactoryPipelineStage::Amplification),
            )
            .await;

            let factory_spec = self.convert_idea_to_spec(&idea, &research_findings);
            let amplified = match self
                .amplifier
                .amplify_task(&factory_spec, &research_findings.proposed_traps)
                .await
            {
                Ok(amp) => amp,
                Err(e) => {
                    self.send_event(
                        event_tx,
                        FactoryPipelineEvent::pipeline_failed(
                            e.to_string(),
                            FactoryPipelineStage::Amplification,
                        ),
                    )
                    .await;
                    return Err(e);
                }
            };

            conversation.add_turn(ConversationTurn::assistant(
                "amplifier",
                format!(
                    "Added {} traps, difficulty score: {:.2}",
                    amplified.traps_added.len(),
                    amplified.difficulty_score
                ),
            ));

            self.send_event(
                event_tx,
                FactoryPipelineEvent::amplification_complete(
                    amplified.traps_added.len(),
                    amplified.difficulty_score,
                ),
            )
            .await;

            // Stage 4: Validation
            self.send_event(
                event_tx,
                FactoryPipelineEvent::stage_started(FactoryPipelineStage::Validation),
            )
            .await;

            let validator_idea = Self::convert_to_validator_idea(&idea);
            let assessment = match self.validator.validate_task(&validator_idea).await {
                Ok(assessment) => assessment,
                Err(e) => {
                    self.send_event(
                        event_tx,
                        FactoryPipelineEvent::pipeline_failed(
                            e.to_string(),
                            FactoryPipelineStage::Validation,
                        ),
                    )
                    .await;
                    return Err(e);
                }
            };

            let passed = assessment.is_valid
                && assessment.complexity_score >= self.config.min_validation_score;

            conversation.add_turn(ConversationTurn::assistant(
                "validator",
                format!(
                    "Validation {}: score {:.2}",
                    if passed { "passed" } else { "failed" },
                    assessment.complexity_score
                ),
            ));

            self.send_event(
                event_tx,
                FactoryPipelineEvent::validation_complete(passed, assessment.complexity_score),
            )
            .await;

            if passed {
                // Stage 5: Finalization
                self.send_event(
                    event_tx,
                    FactoryPipelineEvent::stage_started(FactoryPipelineStage::Finalization),
                )
                .await;

                let task = match self
                    .executor
                    .create_task(&validator_idea, &assessment)
                    .await
                {
                    Ok(task) => task,
                    Err(e) => {
                        self.send_event(
                            event_tx,
                            FactoryPipelineEvent::pipeline_failed(
                                e.to_string(),
                                FactoryPipelineStage::Finalization,
                            ),
                        )
                        .await;
                        return Err(e);
                    }
                };

                // Merge amplified traps into the task
                let final_task = self.merge_amplification(task, &amplified);

                conversation.add_turn(ConversationTurn::assistant(
                    "executor",
                    format!(
                        "Created final task specification: {}",
                        final_task
                            .problem_statement
                            .chars()
                            .take(100)
                            .collect::<String>()
                    ),
                ));

                // Stage 6: Docker Validation (if enabled)
                if self.config.docker_validation_enabled {
                    if let Some(ref docker_validator) = self.docker_validator {
                        self.send_event(
                            event_tx,
                            FactoryPipelineEvent::agent_conversation(
                                "docker_validator",
                                format!("Validating task {} in Docker container", final_task.id),
                            ),
                        )
                        .await;

                        match docker_validator.validate_task(&final_task).await {
                            Ok(result) => {
                                conversation.add_turn(ConversationTurn::assistant(
                                    "docker_validator",
                                    format!(
                                        "Docker validation {}: {}ms",
                                        if result.passed { "passed" } else { "failed" },
                                        result.duration_ms
                                    ),
                                ));

                                if !result.passed {
                                    let error_msg = result
                                        .error
                                        .unwrap_or_else(|| "Docker validation failed".to_string());
                                    self.send_event(
                                        event_tx,
                                        FactoryPipelineEvent::pipeline_failed(
                                            &error_msg,
                                            FactoryPipelineStage::Finalization,
                                        ),
                                    )
                                    .await;
                                    return Err(AgentError::GenerationFailed(error_msg));
                                }
                            }
                            Err(e) => {
                                self.send_event(
                                    event_tx,
                                    FactoryPipelineEvent::pipeline_failed(
                                        e.to_string(),
                                        FactoryPipelineStage::Finalization,
                                    ),
                                )
                                .await;
                                return Err(e);
                            }
                        }
                    } else {
                        self.send_event(
                            event_tx,
                            FactoryPipelineEvent::agent_conversation(
                                "orchestrator",
                                "Docker validation skipped: Docker daemon not available",
                            ),
                        )
                        .await;
                    }
                }

                // Store conversation
                self.conversations
                    .lock()
                    .expect("lock not poisoned")
                    .push(conversation);

                return Ok(final_task);
            }

            // Validation failed - check retry limit
            retries += 1;
            if retries > self.config.max_creation_retries {
                return Err(AgentError::ThresholdNotMet {
                    score: assessment.complexity_score,
                    threshold: self.config.min_validation_score,
                });
            }

            tracing::info!(
                "Validation rejected (attempt {}), retrying creation...",
                retries
            );
        }
    }

    /// Gets research findings from cache or conducts new research.
    async fn get_or_conduct_research(
        &self,
        category: &str,
        event_tx: &mpsc::Sender<FactoryPipelineEvent>,
    ) -> AgentResult<ResearchFindings> {
        // Check cache first - clone findings if found to release lock before await
        let cached_findings = if self.config.cache_research {
            let cache = self.research_cache.lock().expect("lock not poisoned");
            cache.get(category).cloned()
        } else {
            None
        };

        // Return cached findings if available (after releasing the lock)
        if let Some(findings) = cached_findings {
            self.send_event(
                event_tx,
                FactoryPipelineEvent::agent_conversation(
                    "orchestrator",
                    format!("Using cached research for {}", category),
                ),
            )
            .await;
            return Ok(findings);
        }

        // Conduct new research
        let findings = self.research_agent.research_category(category).await?;

        // Cache the results
        if self.config.cache_research {
            let mut cache = self.research_cache.lock().expect("lock not poisoned");
            cache.insert(category.to_string(), findings.clone());
        }

        Ok(findings)
    }

    /// Converts an ideator task idea to a factory task spec.
    fn convert_idea_to_spec(
        &self,
        idea: &IdeatorTaskIdea,
        research: &ResearchFindings,
    ) -> FactoryTaskSpec {
        let weaknesses: Vec<LlmWeaknessType> = research
            .identified_weaknesses
            .iter()
            .map(|w| w.weakness_type.clone())
            .collect();

        FactoryTaskSpec::new(
            &idea.title,
            idea.category.to_benchmark_category(),
            &idea.description,
            idea.estimated_difficulty,
        )
        .with_required_skills(idea.required_skills.clone())
        .with_targeted_weaknesses(weaknesses)
        .with_difficulty_score(idea.estimated_difficulty.base_points() / 100.0)
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

    /// Merges amplification data into the final task.
    fn merge_amplification(&self, task: SyntheticTask, amplified: &AmplifiedTask) -> SyntheticTask {
        // Create updated tags including trap types
        let mut tags = task.metadata.tags.clone();
        for trap in &amplified.traps_added {
            tags.push(format!("trap:{}", trap.trap_type));
        }
        tags.push(format!(
            "amplified_difficulty:{:.2}",
            amplified.difficulty_score
        ));

        // Update metadata with amplification info
        let mut metadata = task.metadata.clone();
        metadata.tags = tags;

        // Create updated anti-memorization config with trap info
        let mut anti_mem = task.anti_memorization.clone();
        anti_mem.dynamic_values.insert(
            "trap_count".to_string(),
            amplified.traps_added.len().to_string(),
        );
        anti_mem.dynamic_values.insert(
            "amplified_difficulty".to_string(),
            format!("{:.2}", amplified.difficulty_score),
        );

        // Include expected failure points in the solution
        let mut solution = task.hidden_solution.clone();
        let failure_note = format!(
            "\n\nExpected LLM failure points:\n{}",
            amplified.expected_failure_points.join("\n- ")
        );
        solution.approach = format!("{}{}", solution.approach, failure_note);

        SyntheticTask {
            id: task.id,
            version: task.version,
            problem_statement: task.problem_statement,
            hidden_solution: solution,
            verification: task.verification,
            difficulty: task.difficulty,
            metadata,
            anti_memorization: anti_mem,
            created_at: task.created_at,
        }
    }

    /// Parses a category string to a TaskCategory.
    fn parse_category(&self, category: &str) -> TaskCategory {
        match category.to_lowercase().as_str() {
            "debugging" | "debug" => TaskCategory::Debugging,
            "security" => TaskCategory::Security,
            "algorithm_design" | "algorithm" => TaskCategory::AlgorithmDesign,
            "infrastructure" | "infra" => TaskCategory::Infrastructure,
            "data_engineering" | "data" => TaskCategory::DataEngineering,
            "networking" | "network" => TaskCategory::Networking,
            "containers" | "container" | "docker" => TaskCategory::Containers,
            "file_operations" | "file" => TaskCategory::FileOperations,
            "software_engineering" | "software" => TaskCategory::SoftwareEngineering,
            "system_debugging" => TaskCategory::SystemDebugging,
            "security_analysis" => TaskCategory::SecurityAnalysis,
            "reverse_engineering" | "reverse" => TaskCategory::ReverseEngineering,
            "performance_optimization" | "performance" => TaskCategory::PerformanceOptimization,
            "integration_tasks" | "integration" => TaskCategory::IntegrationTasks,
            "system_administration" | "sysadmin" => TaskCategory::SystemAdministration,
            "data_science" => TaskCategory::DataScience,
            _ => TaskCategory::Debugging, // Default fallback
        }
    }

    /// Sends an event through the channel, ignoring send errors.
    async fn send_event(
        &self,
        event_tx: &mpsc::Sender<FactoryPipelineEvent>,
        event: FactoryPipelineEvent,
    ) {
        let _ = event_tx.send(event).await;
    }

    /// Returns a reference to the research agent.
    pub fn research_agent(&self) -> &ResearchAgent {
        &self.research_agent
    }

    /// Returns a reference to the ideator agent.
    pub fn ideator(&self) -> &IdeatorAgent {
        &self.ideator
    }

    /// Returns a reference to the amplifier agent.
    pub fn amplifier(&self) -> &DifficultyAmplifierAgent {
        &self.amplifier
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
    pub fn config(&self) -> &FactoryOrchestratorConfig {
        &self.config
    }

    /// Gets the conversation history.
    pub fn conversations(&self) -> Vec<AgentConversation> {
        self.conversations
            .lock()
            .expect("lock not poisoned")
            .clone()
    }

    /// Clears the research cache.
    pub fn clear_research_cache(&self) {
        self.research_cache
            .lock()
            .expect("lock not poisoned")
            .findings
            .clear();
    }
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating a FactoryOrchestrator with fluent API.
pub struct FactoryOrchestratorBuilder {
    llm_client: Option<Arc<dyn LlmProvider>>,
    research_config: Option<ResearchConfig>,
    ideator_config: Option<IdeatorConfig>,
    amplifier_config: Option<AmplifierConfig>,
    validator_config: Option<TaskValidatorConfig>,
    executor_config: Option<TaskExecutorConfig>,
    min_validation_score: f64,
    max_creation_retries: u32,
    continue_on_failure: bool,
    cache_research: bool,
}

impl FactoryOrchestratorBuilder {
    /// Creates a new builder with default values.
    pub fn new() -> Self {
        Self {
            llm_client: None,
            research_config: None,
            ideator_config: None,
            amplifier_config: None,
            validator_config: None,
            executor_config: None,
            min_validation_score: 0.7,
            max_creation_retries: 3,
            continue_on_failure: true,
            cache_research: true,
        }
    }

    /// Sets the LLM client.
    pub fn llm_client(mut self, client: Arc<dyn LlmProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Sets the research configuration.
    pub fn research_config(mut self, config: ResearchConfig) -> Self {
        self.research_config = Some(config);
        self
    }

    /// Sets the ideator configuration.
    pub fn ideator_config(mut self, config: IdeatorConfig) -> Self {
        self.ideator_config = Some(config);
        self
    }

    /// Sets the amplifier configuration.
    pub fn amplifier_config(mut self, config: AmplifierConfig) -> Self {
        self.amplifier_config = Some(config);
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

    /// Sets the minimum validation score.
    pub fn min_validation_score(mut self, score: f64) -> Self {
        self.min_validation_score = score.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum creation retries.
    pub fn max_creation_retries(mut self, retries: u32) -> Self {
        self.max_creation_retries = retries;
        self
    }

    /// Sets whether to continue on failure.
    pub fn continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    /// Sets whether to cache research.
    pub fn cache_research(mut self, cache: bool) -> Self {
        self.cache_research = cache;
        self
    }

    /// Builds the FactoryOrchestrator.
    pub fn build(self) -> AgentResult<FactoryOrchestrator> {
        let llm_client = self
            .llm_client
            .ok_or_else(|| AgentError::ConfigurationError("LLM client is required".to_string()))?;

        let config = FactoryOrchestratorConfig {
            research_config: self.research_config.unwrap_or_default(),
            ideator_config: self.ideator_config.unwrap_or_default(),
            amplifier_config: self.amplifier_config.unwrap_or_default(),
            validator_config: self.validator_config.unwrap_or_default(),
            executor_config: self.executor_config.unwrap_or_default(),
            min_validation_score: self.min_validation_score,
            max_creation_retries: self.max_creation_retries,
            continue_on_failure: self.continue_on_failure,
            cache_research: self.cache_research,
            docker_validation_enabled: false,
            docker_validate_solution: true,
        };

        Ok(FactoryOrchestrator::new(llm_client, config))
    }
}

impl Default for FactoryOrchestratorBuilder {
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

    fn mock_research_response() -> String {
        r#"{
            "category_insights": ["Insight 1", "Insight 2"],
            "identified_weaknesses": [
                {
                    "weakness_type": "multi_step_reasoning",
                    "description": "LLMs struggle with multi-step tasks",
                    "exploitation_strategy": "Create long dependency chains",
                    "severity": 0.8
                }
            ],
            "proposed_traps": [
                {
                    "trap_type": "timing",
                    "description": "Race condition trap",
                    "implementation": "Use concurrent file access",
                    "detection_hint": "Check file locks",
                    "difficulty_increase": 0.2,
                    "targets_weakness": "temporal_awareness"
                }
            ],
            "difficulty_factors": [
                {"name": "Complexity", "description": "Task complexity", "weight": 0.5}
            ]
        }"#
        .to_string()
    }

    #[allow(dead_code)]
    fn mock_ideator_response() -> String {
        r#"{
            "title": "Debug Memory Leak",
            "description": "Find and fix a memory leak in the application",
            "estimated_difficulty": "hard",
            "required_skills": ["rust", "profiling"],
            "anti_patterns": ["restart service"]
        }"#
        .to_string()
    }

    #[allow(dead_code)]
    fn mock_amplification_response() -> String {
        r#"{
            "traps_added": [
                {
                    "id": "trap-1",
                    "trap_type": "timing",
                    "description": "Race condition",
                    "implementation": "impl",
                    "detection_hint": "hint",
                    "difficulty_increase": 0.2,
                    "targets_weakness": "temporal_awareness"
                }
            ],
            "expected_failure_points": ["File access timing"],
            "difficulty_score": 0.85,
            "amplification_notes": "Added timing trap"
        }"#
        .to_string()
    }

    #[allow(dead_code)]
    fn mock_validator_response() -> String {
        r#"{
            "complexity_score": 0.85,
            "memorization_risk": 0.15,
            "estimated_thinking_time_minutes": 20,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": [],
            "reasoning": "Good task"
        }"#
        .to_string()
    }

    #[allow(dead_code)]
    fn mock_executor_response() -> String {
        r#"{
            "problem_statement": "Debug the memory leak in the application",
            "hidden_solution": {
                "approach": "Use valgrind to identify leaks",
                "key_insights": ["Check allocation patterns"],
                "reference_commands": ["valgrind --leak-check=full"],
                "expected_time_seconds": 1200,
                "step_count": 5
            },
            "verification": {
                "success_criteria": ["Leak identified", "Fix proposed"],
                "partial_credit": [{"criterion": "Found leak location", "points": 0.5}],
                "automated_checks": [{"type": "FileExists", "target": "fix.patch", "expected": "true"}]
            },
            "difficulty": {
                "level": "hard",
                "complexity_factors": ["Memory analysis"],
                "base_score": 50.0
            },
            "tags": ["memory", "debugging"]
        }"#
        .to_string()
    }

    #[test]
    fn test_config_default() {
        let config = FactoryOrchestratorConfig::default();
        assert!((config.min_validation_score - 0.7).abs() < 0.01);
        assert_eq!(config.max_creation_retries, 3);
        assert!(config.continue_on_failure);
        assert!(config.cache_research);
    }

    #[test]
    fn test_config_builder() {
        let config = FactoryOrchestratorConfig::new()
            .with_min_validation_score(0.8)
            .with_max_creation_retries(5)
            .with_continue_on_failure(false)
            .with_cache_research(false);

        assert!((config.min_validation_score - 0.8).abs() < 0.01);
        assert_eq!(config.max_creation_retries, 5);
        assert!(!config.continue_on_failure);
        assert!(!config.cache_research);
    }

    #[test]
    fn test_orchestrator_builder_missing_llm() {
        let result = FactoryOrchestratorBuilder::new().build();

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

        let result = FactoryOrchestratorBuilder::new()
            .llm_client(mock_llm)
            .min_validation_score(0.8)
            .max_creation_retries(5)
            .build();

        assert!(result.is_ok());
        let orchestrator = result.expect("should build");
        assert!((orchestrator.config().min_validation_score - 0.8).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let _orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        assert_eq!(FactoryOrchestrator::AGENT_NAME, "factory_orchestrator");
    }

    #[tokio::test]
    async fn test_parse_category() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        assert_eq!(
            orchestrator.parse_category("debugging"),
            TaskCategory::Debugging
        );
        assert_eq!(
            orchestrator.parse_category("security"),
            TaskCategory::Security
        );
        assert_eq!(
            orchestrator.parse_category("containers"),
            TaskCategory::Containers
        );
        assert_eq!(
            orchestrator.parse_category("unknown"),
            TaskCategory::Debugging
        );
    }

    #[tokio::test]
    async fn test_research_caching() {
        let responses = vec![mock_research_response()];
        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        let (event_tx, _rx) = mpsc::channel(100);

        // First call should conduct research
        let findings1 = orchestrator
            .get_or_conduct_research("debugging", &event_tx)
            .await
            .expect("should research");

        // Second call should use cache
        let findings2 = orchestrator
            .get_or_conduct_research("debugging", &event_tx)
            .await
            .expect("should use cache");

        assert_eq!(findings1.category, findings2.category);
    }

    #[tokio::test]
    async fn test_clear_research_cache() {
        let mock_llm = Arc::new(MockLlmProvider::single_response(mock_research_response()));
        let orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        let (event_tx, _rx) = mpsc::channel(100);

        // Conduct research to populate cache
        let _ = orchestrator
            .get_or_conduct_research("debugging", &event_tx)
            .await;

        // Clear cache
        orchestrator.clear_research_cache();

        // Cache should be empty
        let cache = orchestrator.research_cache.lock().expect("lock");
        assert!(cache.findings.is_empty());
    }

    #[tokio::test]
    async fn test_convert_to_validator_idea() {
        use crate::difficulty::DifficultyLevel;

        let ideator_idea = IdeatorTaskIdea::new(
            TaskCategory::Debugging,
            "memory-debugging",
            "Test Title",
            "Test Description",
            DifficultyLevel::Hard,
            vec!["skill1".to_string()],
            vec!["anti1".to_string()],
        );

        let validator_idea = FactoryOrchestrator::convert_to_validator_idea(&ideator_idea);

        assert_eq!(validator_idea.title, "Test Title");
        assert_eq!(validator_idea.description, "Test Description");
        assert_eq!(validator_idea.category, "debugging");
    }

    #[tokio::test]
    async fn test_conversations_stored() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        // Conversations should start empty
        assert!(orchestrator.conversations().is_empty());
    }

    #[tokio::test]
    async fn test_agent_accessors() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("{}".to_string()));
        let orchestrator = FactoryOrchestrator::with_defaults(mock_llm);

        // Verify we can access all sub-agents
        let _ = orchestrator.research_agent();
        let _ = orchestrator.ideator();
        let _ = orchestrator.amplifier();
        let _ = orchestrator.validator();
        let _ = orchestrator.executor();
        let _ = orchestrator.config();
    }
}
