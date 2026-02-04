//! dataforge: Synthetic benchmark task generator for LLM evaluation.
//!
//! This library provides tools for generating, managing, and exporting
//! synthetic terminal-based benchmark tasks.

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
pub mod validation;

// Re-export commonly used error types
pub use error::{
    DockerError, ExportError, GeneratorError, LlmError, RegistryError, TemplateError,
    ValidationError,
};
