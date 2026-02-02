//! LLM provider implementations for the multi-model router.
//!
//! This module provides various LLM provider implementations that can be used
//! with the multi-model router system.

pub mod openrouter;

pub use openrouter::OpenRouterProvider;

// Re-export the main LlmProvider trait from litellm for convenience
pub use super::litellm::LlmProvider;
