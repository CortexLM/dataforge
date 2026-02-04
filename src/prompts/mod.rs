//! LLM prompts for synthetic benchmark task generation.
//!
//! This module contains carefully crafted prompts for each stage of the
//! synthetic task generation pipeline and category-specific ideation prompts.
//!
//! # Architecture
//!
//! The prompts module is organized into four submodules:
//!
//! - [`categories`] - Category-specific prompts with domain expertise guidance
//! - [`ideation`] - Prompts for creative task idea generation
//! - [`validation`] - Prompts for task quality and complexity validation
//! - [`execution`] - Prompts for privileged task execution and solution generation
//!
//! # Usage
//!
//! ```no_run
//! use dataforge::prompts::{
//!     get_category_prompt, build_ideation_prompt, build_validation_prompt, build_execution_prompt
//! };
//!
//! // Get category-specific prompt
//! let category_prompt = get_category_prompt("AlgorithmDesign")
//!     .expect("valid category");
//!
//! // Build prompts for each pipeline stage
//! let ideation = build_ideation_prompt(category_prompt, 1.0, None);
//! let validation = build_validation_prompt(
//!     "Task Title",
//!     "Task description...",
//!     "AlgorithmDesign",
//!     &["rust".to_string(), "optimization".to_string()],
//! );
//! let execution = build_execution_prompt(
//!     "Task Title",
//!     "Task description...",
//!     "AlgorithmDesign",
//!     0.85,
//!     25,
//!     "CANARY_TOKEN_abc123",
//! );
//! ```

pub mod categories;
pub mod execution;
pub mod external_data;
pub mod factory_prompts;
pub mod ideation;
pub mod validation;

pub use categories::{get_category_prompt, CategoryPrompt, CATEGORY_PROMPTS};
pub use execution::{build_execution_prompt, ExecutionPrompt};
pub use external_data::*;
pub use factory_prompts::{
    build_amplifier_prompt, build_research_prompt, get_category_research_context,
    AMPLIFIER_AGENT_SYSTEM, AMPLIFIER_USER_TEMPLATE, ORCHESTRATOR_COORDINATION_SYSTEM,
    RESEARCH_AGENT_SYSTEM, RESEARCH_USER_TEMPLATE,
};
pub use ideation::{build_ideation_prompt, IdeationPrompt};
pub use validation::{build_validation_prompt, ValidationPrompt};
