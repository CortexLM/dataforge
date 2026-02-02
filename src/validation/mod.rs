//! Validation modules for synth-bench.
//!
//! This module provides schema validation for task templates and
//! task validation for ensuring tasks meet quality standards.

pub mod schema_validator;
pub mod task_validator;

pub use schema_validator::{ErrorSeverity, SchemaError, SchemaValidationResult, SchemaValidator};
pub use task_validator::{CheckResult, TaskValidationResult, TaskValidator};
