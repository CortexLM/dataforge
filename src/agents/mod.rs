//! Multi-agent validation system for synth-bench.
//!
//! This module provides a multi-agent architecture for generating and validating
//! synthetic benchmark tasks. The system consists of:
//!
//! - **Generator Agent**: Creates tasks from templates based on difficulty levels
//! - **Difficulty Validator Agent**: Uses LLM to verify task difficulty matches expectations
//! - **Feasibility Validator Agent**: Uses LLM to ensure tasks are solvable but not trivial
//! - **Task Validator Agent**: Uses LLM to validate tasks are genuinely challenging and not memorizable
//! - **Orchestrator Agent**: Coordinates the validation pipeline and provides events for TUI
//! - **Environment Builder Agent**: Builds reproducible Docker environments for tasks
//! - **Validator Agent**: Validates solution correctness via test execution
//! - **Synthetic Generator Agent**: Generates DevOps benchmark problems from scratch
//!
//! # Example
//!
//! ```ignore
//! use synth_bench::agents::{
//!     OrchestratorAgent, OrchestratorConfig, GeneratorAgentConfig,
//!     PipelineEvent, DifficultyLevel,
//! };
//! use synth_bench::llm::LiteLlmClient;
//! use std::sync::Arc;
//! use tokio::sync::mpsc;
//!
//! // Setup LLM client
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//!
//! // Configure generator
//! let generator_config = GeneratorAgentConfig::new("/output/tasks")
//!     .with_template(my_template);
//!
//! // Create orchestrator
//! let config = OrchestratorConfig::new(generator_config);
//! let orchestrator = OrchestratorAgent::new(llm_client, config);
//!
//! // Run validation pipeline
//! let (event_tx, mut event_rx) = mpsc::channel(100);
//! let report = orchestrator
//!     .run_validation_pipeline(DifficultyLevel::Medium, 42, event_tx)
//!     .await?;
//!
//! // Handle events for TUI updates
//! while let Some(event) = event_rx.recv().await {
//!     match event {
//!         PipelineEvent::StageCompleted { stage, result, .. } => {
//!             println!("Stage {} completed with score {}", stage, result.score.score);
//!         }
//!         PipelineEvent::PipelineCompleted { report } => {
//!             println!("Pipeline finished: passed={}", report.overall_passed);
//!         }
//!         _ => {}
//!     }
//! }
//! ```

pub mod analyzer_agent;
pub mod collector_agent;
pub mod difficulty_amplifier;
pub mod difficulty_validator;
pub mod docker_validator;
pub mod environment_builder;
pub mod error;
pub mod factory_orchestrator;
pub mod factory_types;
pub mod feasibility_validator;
pub mod generator;
pub mod ideator;
pub mod orchestrator;
pub mod problem_crafter;
pub mod research_agent;
pub mod synthetic_generator_agent;
pub mod synthetic_orchestrator;
pub mod task_executor;
pub mod task_validator;
pub mod test_designer;
pub mod types;
pub mod validator_agent;

// Re-export main types
pub use analyzer_agent::{
    AnalyzedTask as PipelineAnalyzedTask, AnalyzerAgent, AnalyzerConfig,
    TaskCategory as AnalyzerTaskCategory,
};
pub use collector_agent::{
    CollectedTask, CollectorAgent, CollectorConfig, PrioritizedTask, TaskSource,
};
pub use difficulty_amplifier::{AmplifierConfig, DifficultyAmplifierAgent};
pub use difficulty_validator::{DifficultyValidatorAgent, DifficultyValidatorConfig};
pub use docker_validator::{DockerValidatorAgent, DockerValidatorConfig, DockerValidationResult};
pub use environment_builder::{
    AnalyzedTask, BuiltEnvironment, EnvironmentBuilderAgent, EnvironmentConfig,
};
pub use error::{AgentError, AgentResult};
pub use factory_orchestrator::{
    FactoryOrchestrator, FactoryOrchestratorBuilder, FactoryOrchestratorConfig,
};
pub use factory_types::{
    AgentConversation, AmplifiedTask, ConversationTurn, DifficultyFactor, DifficultyTrap,
    DifficultyTrapType, FactoryPipelineEvent, FactoryPipelineStage, FactoryTaskSpec, LlmWeakness,
    LlmWeaknessType, ResearchFindings,
};
pub use feasibility_validator::{FeasibilityValidatorAgent, FeasibilityValidatorConfig};
pub use generator::{GeneratorAgent, GeneratorAgentConfig};
pub use ideator::{IdeatorAgent, IdeatorConfig, TaskCategory, TaskIdea as IdeatorTaskIdea};
pub use orchestrator::{OrchestratorAgent, OrchestratorBuilder, OrchestratorConfig};
pub use problem_crafter::{CraftedProblem, CrafterConfig, ProblemCrafterAgent};
pub use research_agent::{ResearchAgent, ResearchConfig};
pub use synthetic_generator_agent::{
    GeneratorConfig as SyntheticGeneratorConfig, SyntheticCategory, SyntheticGeneratorAgent,
    SyntheticProblem, SyntheticProblemBuilder,
};
pub use synthetic_orchestrator::{
    SyntheticOrchestrator, SyntheticOrchestratorBuilder, SyntheticOrchestratorConfig,
    SyntheticPipelineEvent, SyntheticPipelineStage,
};
pub use task_executor::{
    AntiMemorizationConfig, AutomatedCheck, CheckType, DifficultyScoring, HiddenSolution,
    PartialCreditItem, SyntheticTask, TaskExecutorAgent, TaskExecutorConfig, TaskMetadata,
    VerificationSpec,
};
pub use task_validator::{TaskIdea, TaskValidatorAgent, TaskValidatorConfig, ValidationAssessment};
pub use test_designer::{
    TestCommand, TestDesignerAgent, TestDesignerConfig, TestSpec as DesignerTestSpec,
};
pub use types::{
    AgentMessage, AgentStatus, GeneratedTask, MessageType, PipelineEvent, PipelineStage,
    TaskValidationReport, ValidationResult, ValidationScore,
};
pub use validator_agent::{
    ExpectedOutcome, TestSpec, ValidationOutcome, ValidatorAgent, ValidatorConfig,
};
