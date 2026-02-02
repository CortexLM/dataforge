//! Error types for synth-bench operations.
//!
//! Defines comprehensive error types for all major subsystems:
//! - Template parsing and validation
//! - Task generation and instantiation
//! - Registry lifecycle operations
//! - Docker container management
//! - Dataset export (HuggingFace, filesystem)
//! - LLM API interactions
//! - Validation and verification

use thiserror::Error;

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Task '{0}' not found in registry")]
    TaskNotFound(String),

    #[error("Task '{0}' already exists in registry")]
    DuplicateTask(String),

    #[error("Invalid state transition from '{from}' to '{to}': {reason}")]
    InvalidTransition {
        from: String,
        to: String,
        reason: String,
    },

    #[error("Review requirements not met: {0}")]
    ReviewRequirementsNotMet(String),

    #[error("Publish requirements not met: {0}")]
    PublishRequirementsNotMet(String),

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),

    #[error("Lifecycle error: {0}")]
    LifecycleError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),
}

/// Errors that can occur during task generation.
#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("Template error: {0}")]
    Template(String),

    #[error("Variable '{name}' not found in template")]
    VariableNotFound { name: String },

    #[error("Circular dependency detected in variables: {0}")]
    CircularDependency(String),

    #[error("Invalid variable type '{var_type}' for variable '{name}'")]
    InvalidVariableType { name: String, var_type: String },

    #[error("Range expression evaluation failed: {0}")]
    RangeExpressionError(String),

    #[error("File generation failed for '{path}': {reason}")]
    FileGenerationFailed { path: String, reason: String },

    #[error("Unknown file generator: {0}")]
    UnknownGenerator(String),

    #[error("Tera template rendering error: {0}")]
    Tera(#[from] tera::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Invalid parameter value: {0}")]
    InvalidParameter(String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),
}

/// Errors that can occur during LLM operations.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Missing API key: LITELLM_API_KEY environment variable not set")]
    MissingApiKey,

    #[error("Missing API base URL: LITELLM_API_BASE environment variable not set")]
    MissingApiBase,

    #[error("HTTP request failed: {0}")]
    RequestFailed(String),

    #[error("Failed to parse LLM response: {0}")]
    ParseError(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Invalid model: {0}")]
    InvalidModel(String),

    #[error("Context length exceeded: {limit} tokens")]
    ContextLengthExceeded { limit: u32 },

    #[error("API error ({code}): {message}")]
    ApiError { code: u16, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors that can occur during template operations.
#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Template '{0}' not found")]
    NotFound(String),

    #[error("Failed to parse template file '{path}': {message}")]
    ParseError { path: String, message: String },

    #[error("Invalid variable definition for '{variable}': {message}")]
    InvalidVariableDefinition { variable: String, message: String },

    #[error("Invalid regex pattern '{pattern}' for variable '{variable}': {message}")]
    InvalidRegexPattern {
        variable: String,
        pattern: String,
        message: String,
    },

    #[error("Invalid range [{min}, {max}] for variable '{variable}': min must be <= max")]
    InvalidRange {
        variable: String,
        min: String,
        max: String,
    },

    #[error("Invalid difficulty config: min_score ({min}) must be <= max_score ({max})")]
    InvalidDifficultyRange { min: f64, max: f64 },

    #[error("Invalid difficulty level '{0}': must be 'easy', 'medium', or 'hard'")]
    InvalidDifficultyLevel(String),

    #[error("Missing required field '{field}' in template '{template}'")]
    MissingRequiredField { template: String, field: String },

    #[error("Invalid template ID '{0}': must be non-empty and contain only alphanumeric characters, hyphens, and underscores")]
    InvalidTemplateId(String),

    #[error("Invalid version '{0}': must follow semantic versioning (e.g., '1.0.0')")]
    InvalidVersion(String),

    #[error("Empty choices list for variable '{0}'")]
    EmptyChoices(String),

    #[error("Weights count ({weights}) does not match choices count ({choices}) for variable '{variable}'")]
    WeightsMismatch {
        variable: String,
        weights: usize,
        choices: usize,
    },

    #[error("Weights must be non-negative for variable '{0}'")]
    NegativeWeight(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Duplicate template ID '{0}' found during loading")]
    DuplicateTemplateId(String),
}

/// Errors that can occur during validation operations.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Schema validation failed: {0}")]
    SchemaError(String),

    #[error("Structure validation failed: {0}")]
    StructureError(String),

    #[error("Solution validation failed: {0}")]
    SolutionError(String),

    #[error("Missing required file: {0}")]
    MissingFile(String),

    #[error("Invalid file content in '{file}': {reason}")]
    InvalidContent { file: String, reason: String },

    #[error("Canary verification failed: expected {expected}, got {actual}")]
    CanaryMismatch { expected: String, actual: String },

    #[error("Task output validation failed: {0}")]
    OutputMismatch(String),

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Errors that can occur during Docker operations.
#[derive(Debug, Error)]
pub enum DockerError {
    #[error("Docker build failed: {0}")]
    BuildFailed(String),

    #[error("Docker run failed: {0}")]
    RunFailed(String),

    #[error("Container '{id}' not found")]
    ContainerNotFound { id: String },

    #[error("Container execution timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    #[error("Failed to copy files to container: {0}")]
    CopyFailed(String),

    #[error("Docker daemon not available: {0}")]
    DaemonUnavailable(String),

    #[error("Invalid Dockerfile: {0}")]
    InvalidDockerfile(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Container exited with non-zero code {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors that can occur during export operations.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("HuggingFace API error: {0}")]
    HuggingFaceApi(String),

    #[error("Failed to create dataset: {0}")]
    DatasetCreationFailed(String),

    #[error("Failed to upload file '{file}': {reason}")]
    UploadFailed { file: String, reason: String },

    #[error("Invalid export format: {0}")]
    InvalidFormat(String),

    #[error("Filesystem error: {0}")]
    FilesystemError(String),

    #[error("Missing authentication token")]
    MissingToken,

    #[error("Export path already exists: {0}")]
    PathExists(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("No tasks to export")]
    NoTasks,

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
