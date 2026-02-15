//! SWE-Forge: mine GitHub PRs into SWE-bench compatible benchmark datasets.
//!
//! This library provides the SWE mining pipeline, LLM integration,
//! Parquet dataset export, and HuggingFace Hub upload.

pub mod agents;
pub mod anti_hardcoding;
pub mod cli;
pub mod difficulty;
pub mod docker;
pub mod error;
pub mod execution;
pub mod export;
pub mod llm;
pub mod swe;
pub mod utils;
