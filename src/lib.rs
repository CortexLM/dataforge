//! DataForge SWE pipeline for mining GitHub PRs into terminal benchmark tasks.
//!
//! This library provides tools for SWE mining, validation, and export.

// Core modules
pub mod agents;
pub mod anti_hardcoding;
pub mod categories;
pub mod cli;
pub mod collectors;
pub mod difficulty;
pub mod diversity;
pub mod docker;
pub mod error;
pub mod execution;
pub mod export;
pub mod generator;
pub mod llm;
pub mod metrics;
pub mod pipeline;
pub mod prompts;
pub mod quality;
pub mod registry;
pub mod scaffold;
pub mod scheduler;
pub mod storage;
pub mod template;
pub mod test_framework;
pub mod trajectory;
pub mod utils;
pub mod validation;
pub mod swe;
pub mod workspace;

// Re-export commonly used error types
pub use error::{
    DockerError, ExportError, GeneratorError, LlmError, RegistryError, TemplateError,
    ValidationError,
};
