//! Legacy type definitions for backwards compatibility.
//!
//! This module re-exports types from the new schema and variables modules,
//! providing backwards compatibility with code that imports from `template::types`.
//!
//! New code should import directly from `template::schema` and `template::variables`.

pub use crate::template::schema::{
    AntiHardcodingConfig, DifficultyConfig, ExpectedOutput, GeneratedFileConfig,
    ProcessValidationConfig, TaskTemplate,
};
pub use crate::template::variables::{Distribution, NetworkType, VariableDefinition, VariableType};
