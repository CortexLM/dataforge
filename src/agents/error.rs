//! Error types for the multi-agent validation system.
//!
//! Defines comprehensive error types for agent operations including
//! generation, validation, and orchestration failures.

use thiserror::Error;

/// Errors that can occur during agent operations.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Error during task generation.
    #[error("Task generation failed: {0}")]
    GenerationFailed(String),

    /// Error during difficulty validation.
    #[error("Difficulty validation failed: {0}")]
    DifficultyValidationFailed(String),

    /// Error during feasibility validation.
    #[error("Feasibility validation failed: {0}")]
    FeasibilityValidationFailed(String),

    /// Error from the LLM provider.
    #[error("LLM error: {0}")]
    LlmError(String),

    /// Error parsing LLM response.
    #[error("Failed to parse LLM response: {0}")]
    ResponseParseError(String),

    /// Template not found.
    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    /// Invalid difficulty level.
    #[error("Invalid difficulty level: {0}")]
    InvalidDifficulty(String),

    /// Pipeline stage failed.
    #[error("Pipeline stage '{stage}' failed: {reason}")]
    PipelineStageError { stage: String, reason: String },

    /// Channel communication error.
    #[error("Channel communication failed: {0}")]
    ChannelError(String),

    /// Configuration error.
    #[error("Agent configuration error: {0}")]
    ConfigurationError(String),

    /// Timeout during agent operation.
    #[error("Agent operation timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    /// Validation threshold not met.
    #[error("Validation threshold not met: score {score:.2} < required {threshold:.2}")]
    ThresholdNotMet { score: f64, threshold: f64 },

    /// Underlying generator error.
    #[error("Generator error: {0}")]
    Generator(#[from] crate::error::GeneratorError),

    /// Underlying template error.
    #[error("Template error: {0}")]
    Template(#[from] crate::error::TemplateError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<crate::error::LlmError> for AgentError {
    fn from(err: crate::error::LlmError) -> Self {
        AgentError::LlmError(err.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for AgentError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        AgentError::ChannelError(format!("Failed to send on channel: {}", err))
    }
}

/// Result type alias for agent operations.
pub type AgentResult<T> = Result<T, AgentError>;
