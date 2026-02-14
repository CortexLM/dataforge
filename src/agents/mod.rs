//! DataForge agents for SWE task mining and quality scoring.
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::agents::{
//!     OrchestratorAgent, OrchestratorConfig,
//!     PipelineEvent, DifficultyLevel,
//! };
//! use swe_forge::llm::LiteLlmClient;
//! use std::sync::Arc;
//! use tokio::sync::mpsc;
//!
//! // Setup LLM client
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//!
//! // Create orchestrator
//! let config = OrchestratorConfig::default();
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
pub mod code_cleaner;
pub mod code_generator;
pub mod collector_agent;
pub mod debate_agents;
pub mod debate_orchestrator;
pub mod difficulty_amplifier;
pub mod difficulty_validator;
pub mod docker_validator;
pub mod environment_builder;
pub mod error;
pub mod factory_orchestrator;
pub mod factory_types;
pub mod feasibility_validator;
pub mod full_pipeline_orchestrator;
pub mod generator;
pub mod ideator;
pub mod orchestrator;
pub mod problem_crafter;
pub mod research_agent;
pub mod synthetic_generator_agent;
pub mod synthetic_orchestrator;
pub mod task_evaluator;
pub mod task_executor;
pub mod task_validator;
pub mod test_designer;
pub mod types;
pub mod validator_agent;
pub mod vulnerability_injector;
pub mod workspace_ideator;
pub mod workspace_orchestrator;
pub mod workspace_validator;

// Advanced synthetic workspace generation
pub mod synthetic_workspace;

// Re-export SWE-relevant and shared validation types
pub use analyzer_agent::{
    AnalyzedTask as PipelineAnalyzedTask, AnalyzerAgent, AnalyzerConfig,
    TaskCategory as AnalyzerTaskCategory,
};
pub use collector_agent::{
    CollectedTask, CollectorAgent, CollectorConfig, PrioritizedTask, TaskSource,
};
pub use difficulty_validator::{DifficultyValidatorAgent, DifficultyValidatorConfig};
pub use docker_validator::{DockerValidationResult, DockerValidatorAgent, DockerValidatorConfig};
pub use error::{AgentError, AgentResult};
pub use feasibility_validator::FeasibilityValidatorAgent;
pub use orchestrator::{OrchestratorAgent, OrchestratorBuilder, OrchestratorConfig};
pub use research_agent::{ResearchAgent, ResearchConfig};
pub use task_evaluator::{
    AgentAction, AgentStep, EvaluationConfig, EvaluationResult, TaskEvaluator, TerminationReason,
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

pub use workspace_validator::{
    BenchmarkArtifacts, BenchmarkDifficulty, ValidatedWorkspace, ValidationScores,
    WorkspaceFile as ValidatedWorkspaceFile, WorkspaceValidationResult, WorkspaceValidatorAgent,
    WorkspaceValidatorConfig,
};

// Debate system re-exports
pub use debate_agents::{
    AgentPosition, DebateAgentRole, DebateResponse, DebateTopic, ResponseToOther,
};
pub use debate_orchestrator::{
    ConsensusMechanism, ConsensusResult, DebateContext, DebateEvent, DebateOrchestrator,
    DebateOrchestratorBuilder, DebateOrchestratorConfig, DebateRound, DissentingOpinion,
};
// Advanced synthetic workspace re-exports are intentionally disabled in SWE mode.
