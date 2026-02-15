//! Agents for SWE task validation and Docker-based verification.

pub mod docker_validator;
pub mod error;
pub mod task_executor;
pub mod task_validator;

pub use docker_validator::{DockerValidationResult, DockerValidatorAgent, DockerValidatorConfig};
pub use error::{AgentError, AgentResult};
pub use task_executor::{
    AntiMemorizationConfig, AutomatedCheck, CheckType, DifficultyScoring, HiddenSolution,
    PartialCreditItem, SyntheticTask, TaskExecutorAgent, TaskExecutorConfig, TaskMetadata,
    VerificationSpec,
};
pub use task_validator::{TaskIdea, TaskValidatorAgent, TaskValidatorConfig, ValidationAssessment};
