//! LLM integration for dataforge.
//!
//! This module provides integration with various LLM providers for AI-assisted
//! template generation, instruction improvement, and multi-model routing.
//!
//! # Caching Support
//!
//! The module includes a prompt caching system for multi-conversation efficiency.
//! System prompts and conversation prefixes can be cached to reduce token usage
//! across multiple agent conversations.
//!
//! ```ignore
//! use dataforge::llm::{LiteLlmClient, PromptCache, Message, GenerationRequest};
//!
//! let client = LiteLlmClient::from_env()?;
//! let cache = PromptCache::new(1000);
//!
//! // Cache system prompts for reuse
//! let cached_msg = cache.cache_message(Message::system("You are helpful"));
//! let request = GenerationRequest::new("gpt-4", vec![cached_msg.into()]);
//! let response = client.generate_with_cache(request, &cache).await?;
//! ```
//!
//! # Multi-Model Routing
//!
//! The router module provides flexible routing strategies for distributing
//! requests across multiple LLM providers:
//!
//! ```ignore
//! use dataforge::llm::router::{MultiModelRouter, RoutingStrategy, ModelCapabilities};
//! use dataforge::llm::providers::OpenRouterProvider;
//! use std::sync::Arc;
//!
//! let mut router = MultiModelRouter::new(RoutingStrategy::CostOptimized);
//!
//! // Add providers
//! let provider = Arc::new(OpenRouterProvider::new("api-key".to_string()));
//! router.add_provider(provider, "moonshotai/kimi-k2.5");
//!
//! // Add model capabilities for cost optimization
//! router.add_model_capabilities(ModelCapabilities::new("moonshotai/kimi-k2.5")
//!     .with_pricing(0.5, 1.5)
//!     .with_coding_score(0.8));
//! ```
//!
//! # Cost Tracking
//!
//! Track LLM usage costs with daily and monthly budgets:
//!
//! ```ignore
//! use dataforge::llm::cost::CostTracker;
//!
//! let tracker = CostTracker::new(10.0, 100.0); // $10/day, $100/month
//! tracker.record_usage("gpt-4", 1000, 500, 3.0, 15.0);
//!
//! if tracker.is_over_budget() {
//!     println!("Budget exceeded!");
//! }
//! ```

pub mod cache;
pub mod cost;
pub mod litellm;
pub mod providers;
pub mod router;

pub use cache::{
    create_shared_cache, create_shared_cache_with_config, CacheConfig, CacheStats, CachedMessage,
    ContentHash, PromptCache, SharedPromptCache,
};
pub use litellm::{
    Choice, GenerationRequest, GenerationResponse, LiteLlmClient, LlmProvider, Message,
    TemplateAssistant, Usage, TEMPLATE_GENERATION_PROMPT,
};

// Re-export key types from submodules for convenience
pub use cost::{CostReport, CostTracker, UsageRecord};
pub use providers::OpenRouterProvider;
pub use router::{
    LlmRouter, ModelCapabilities, MultiModelRouter, RouterError, RoutingStrategy, TaskHint,
};
