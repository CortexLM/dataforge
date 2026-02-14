//! Multi-model LLM router with multiple routing strategies.
//!
//! This module provides a flexible routing system for LLM requests that supports:
//! - Round Robin: Simple rotation across models
//! - Cost Optimized: Select cheapest model that meets requirements
//! - Capability Based: Match model to task requirements
//! - Fallback Chain: Try next provider on failure
//! - Experimental: A/B testing between models

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::cost::CostTracker;
use super::litellm::{GenerationRequest, GenerationResponse, LlmProvider};
use crate::error::LlmError;

/// Error type for router operations.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// No providers are available.
    #[error("No providers available")]
    NoProviders,

    /// No model matches the requirements.
    #[error("No model matches requirements: {0}")]
    NoMatchingModel(String),

    /// All providers in the fallback chain failed.
    #[error("All providers failed. Last error: {0}")]
    AllProvidersFailed(String),

    /// Budget exceeded.
    #[error("Budget exceeded: daily={daily}, monthly={monthly}")]
    BudgetExceeded { daily: bool, monthly: bool },

    /// Underlying LLM error.
    #[error("LLM error: {0}")]
    LlmError(#[from] LlmError),
}

/// Hint about the task to help with routing decisions.
#[derive(Debug, Clone, Default)]
pub struct TaskHint {
    /// Task category (e.g., "code_generation", "text_analysis").
    pub category: Option<String>,
    /// Difficulty level (e.g., "easy", "medium", "hard").
    pub difficulty: Option<String>,
    /// Estimated tokens needed for the response.
    pub estimated_tokens: Option<u32>,
    /// Whether the task requires long context handling.
    pub requires_long_context: bool,
    /// Whether the task requires code generation capabilities.
    pub requires_code_generation: bool,
}

impl TaskHint {
    /// Create a new empty task hint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the difficulty.
    pub fn with_difficulty(mut self, difficulty: impl Into<String>) -> Self {
        self.difficulty = Some(difficulty.into());
        self
    }

    /// Set estimated tokens.
    pub fn with_estimated_tokens(mut self, tokens: u32) -> Self {
        self.estimated_tokens = Some(tokens);
        self
    }

    /// Mark as requiring long context.
    pub fn with_long_context(mut self) -> Self {
        self.requires_long_context = true;
        self
    }

    /// Mark as requiring code generation.
    pub fn with_code_generation(mut self) -> Self {
        self.requires_code_generation = true;
        self
    }
}

/// Capabilities and pricing information for a model.
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    /// Model identifier (e.g., "anthropic/claude-3-opus").
    pub name: String,
    /// Maximum context window in tokens.
    pub max_context: u32,
    /// Code generation quality score (0.0 - 1.0).
    pub coding_score: f32,
    /// Reasoning quality score (0.0 - 1.0).
    pub reasoning_score: f32,
    /// Speed/latency score (0.0 - 1.0, higher = faster).
    pub speed_score: f32,
    /// Cost per 1 million input tokens in dollars.
    pub cost_per_1m_input: f64,
    /// Cost per 1 million output tokens in dollars.
    pub cost_per_1m_output: f64,
}

impl ModelCapabilities {
    /// Create new model capabilities.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            max_context: 8192,
            coding_score: 0.5,
            reasoning_score: 0.5,
            speed_score: 0.5,
            cost_per_1m_input: 1.0,
            cost_per_1m_output: 1.0,
        }
    }

    /// Set maximum context window.
    pub fn with_max_context(mut self, max_context: u32) -> Self {
        self.max_context = max_context;
        self
    }

    /// Set coding score.
    pub fn with_coding_score(mut self, score: f32) -> Self {
        self.coding_score = score.clamp(0.0, 1.0);
        self
    }

    /// Set reasoning score.
    pub fn with_reasoning_score(mut self, score: f32) -> Self {
        self.reasoning_score = score.clamp(0.0, 1.0);
        self
    }

    /// Set speed score.
    pub fn with_speed_score(mut self, score: f32) -> Self {
        self.speed_score = score.clamp(0.0, 1.0);
        self
    }

    /// Set pricing.
    pub fn with_pricing(mut self, cost_per_1m_input: f64, cost_per_1m_output: f64) -> Self {
        self.cost_per_1m_input = cost_per_1m_input;
        self.cost_per_1m_output = cost_per_1m_output;
        self
    }

    /// Calculate estimated cost for a request.
    pub fn estimate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        (input_tokens as f64 / 1_000_000.0) * self.cost_per_1m_input
            + (output_tokens as f64 / 1_000_000.0) * self.cost_per_1m_output
    }
}

/// Strategy for routing requests to models.
#[derive(Debug, Clone, Default)]
pub enum RoutingStrategy {
    /// Simple round-robin rotation across all providers.
    #[default]
    RoundRobin,
    /// Select the cheapest model that meets requirements.
    CostOptimized,
    /// Select based on task requirements matching model capabilities.
    CapabilityBased,
    /// A/B testing between two models.
    Experimental {
        /// Control model identifier.
        control: String,
        /// Treatment model identifier.
        treatment: String,
        /// Ratio of requests to send to treatment (0.0 - 1.0).
        split_ratio: f32,
    },
}

/// Trait for LLM routers that can route requests to providers.
#[async_trait]
pub trait LlmRouter: Send + Sync {
    /// Route a request to an appropriate provider.
    ///
    /// # Arguments
    ///
    /// * `request` - The generation request
    /// * `task_hint` - Optional hint about the task for better routing
    async fn route(
        &self,
        request: &GenerationRequest,
        task_hint: Option<&TaskHint>,
    ) -> Result<GenerationResponse, RouterError>;

    /// Get list of available model identifiers.
    fn available_models(&self) -> Vec<String>;
}

/// Provider entry with associated model information.
struct ProviderEntry {
    /// The provider implementation.
    provider: Arc<dyn LlmProvider>,
    /// Model identifier this provider serves.
    model: String,
}

/// Multi-model router supporting various routing strategies.
pub struct MultiModelRouter {
    /// Registered providers with their models.
    providers: Vec<ProviderEntry>,
    /// Model capabilities for routing decisions.
    model_capabilities: HashMap<String, ModelCapabilities>,
    /// Current routing strategy.
    strategy: RoutingStrategy,
    /// Fallback chain of models to try on failure.
    fallback_chain: Vec<String>,
    /// Cost tracker for budget enforcement.
    cost_tracker: Arc<CostTracker>,
    /// Round-robin counter for round-robin strategy.
    round_robin_counter: AtomicUsize,
    /// Random state for experimental routing.
    experimental_counter: AtomicUsize,
}

impl MultiModelRouter {
    /// Create a new multi-model router with the specified strategy.
    ///
    /// # Arguments
    ///
    /// * `strategy` - The routing strategy to use
    ///
    /// # Example
    ///
    /// ```
    /// use dataforge::llm::router::{MultiModelRouter, RoutingStrategy};
    ///
    /// let router = MultiModelRouter::new(RoutingStrategy::CostOptimized);
    /// ```
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            providers: Vec::new(),
            model_capabilities: HashMap::new(),
            strategy,
            fallback_chain: Vec::new(),
            cost_tracker: Arc::new(CostTracker::new(100.0, 1000.0)),
            round_robin_counter: AtomicUsize::new(0),
            experimental_counter: AtomicUsize::new(0),
        }
    }

    /// Create a router with a custom cost tracker.
    pub fn with_cost_tracker(strategy: RoutingStrategy, cost_tracker: Arc<CostTracker>) -> Self {
        Self {
            providers: Vec::new(),
            model_capabilities: HashMap::new(),
            strategy,
            fallback_chain: Vec::new(),
            cost_tracker,
            round_robin_counter: AtomicUsize::new(0),
            experimental_counter: AtomicUsize::new(0),
        }
    }

    /// Add a provider to the router.
    ///
    /// # Arguments
    ///
    /// * `provider` - The LLM provider implementation
    /// * `model` - Model identifier this provider serves
    pub fn add_provider(&mut self, provider: Arc<dyn LlmProvider>, model: impl Into<String>) {
        self.providers.push(ProviderEntry {
            provider,
            model: model.into(),
        });
    }

    /// Add model capabilities for routing decisions.
    pub fn add_model_capabilities(&mut self, capabilities: ModelCapabilities) {
        self.model_capabilities
            .insert(capabilities.name.clone(), capabilities);
    }

    /// Set the fallback chain of models to try on failure.
    ///
    /// # Arguments
    ///
    /// * `models` - Ordered list of model identifiers to try
    pub fn set_fallback_chain(&mut self, models: Vec<String>) {
        self.fallback_chain = models;
    }

    /// Get the cost tracker.
    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }

    /// Get the current routing strategy.
    pub fn strategy(&self) -> &RoutingStrategy {
        &self.strategy
    }

    /// Set a new routing strategy.
    pub fn set_strategy(&mut self, strategy: RoutingStrategy) {
        self.strategy = strategy;
    }

    /// Select a model based on the current strategy.
    fn select_model(
        &self,
        request: &GenerationRequest,
        hint: Option<&TaskHint>,
    ) -> Result<String, RouterError> {
        if self.providers.is_empty() {
            return Err(RouterError::NoProviders);
        }

        match &self.strategy {
            RoutingStrategy::RoundRobin => self.select_round_robin(),
            RoutingStrategy::CostOptimized => self.select_cost_optimized(request, hint),
            RoutingStrategy::CapabilityBased => self.select_capability_based(request, hint),
            RoutingStrategy::Experimental {
                control,
                treatment,
                split_ratio,
            } => self.select_experimental(control, treatment, *split_ratio),
        }
    }

    /// Round-robin model selection.
    fn select_round_robin(&self) -> Result<String, RouterError> {
        let index = self.round_robin_counter.fetch_add(1, Ordering::SeqCst);
        let provider = &self.providers[index % self.providers.len()];
        Ok(provider.model.clone())
    }

    /// Cost-optimized model selection.
    fn select_cost_optimized(
        &self,
        request: &GenerationRequest,
        hint: Option<&TaskHint>,
    ) -> Result<String, RouterError> {
        let estimated_input_tokens = request
            .messages
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum::<u32>();

        let estimated_output_tokens = hint.and_then(|h| h.estimated_tokens).unwrap_or(500);

        let required_context = estimated_input_tokens + estimated_output_tokens;

        // Find eligible models (those with sufficient context window)
        let mut eligible: Vec<(&String, f64)> = self
            .model_capabilities
            .iter()
            .filter(|(_, caps)| {
                // Must have enough context
                if caps.max_context < required_context {
                    return false;
                }

                // Must meet code generation requirement if specified
                if let Some(h) = hint {
                    if h.requires_code_generation && caps.coding_score < 0.5 {
                        return false;
                    }
                    if h.requires_long_context && caps.max_context < 32_000 {
                        return false;
                    }
                }

                // Must have a registered provider
                self.providers.iter().any(|p| p.model == caps.name)
            })
            .map(|(name, caps)| {
                let cost = caps.estimate_cost(estimated_input_tokens, estimated_output_tokens);
                (name, cost)
            })
            .collect();

        if eligible.is_empty() {
            // Fall back to any available model
            return Ok(self.providers[0].model.clone());
        }

        // Sort by cost (ascending)
        eligible.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(eligible[0].0.clone())
    }

    /// Capability-based model selection.
    fn select_capability_based(
        &self,
        request: &GenerationRequest,
        hint: Option<&TaskHint>,
    ) -> Result<String, RouterError> {
        let estimated_input_tokens: u32 = request
            .messages
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let estimated_output_tokens = hint.and_then(|h| h.estimated_tokens).unwrap_or(500);
        let required_context = estimated_input_tokens + estimated_output_tokens;

        // Score each model based on task requirements
        let mut scored: Vec<(&String, f32)> = self
            .model_capabilities
            .iter()
            .filter(|(_, caps)| {
                caps.max_context >= required_context
                    && self.providers.iter().any(|p| p.model == caps.name)
            })
            .map(|(name, caps)| {
                let mut score = 0.0f32;

                if let Some(h) = hint {
                    // Weight coding score heavily if code generation required
                    if h.requires_code_generation {
                        score += caps.coding_score * 3.0;
                    }

                    // Weight reasoning for difficult tasks
                    if matches!(h.difficulty.as_deref(), Some("hard") | Some("expert")) {
                        score += caps.reasoning_score * 2.0;
                    }

                    // Speed matters for simpler tasks
                    if matches!(h.difficulty.as_deref(), Some("easy") | Some("simple")) {
                        score += caps.speed_score * 1.5;
                    }
                } else {
                    // Default: balanced scoring
                    score = caps.coding_score + caps.reasoning_score + caps.speed_score;
                }

                (name, score)
            })
            .collect();

        if scored.is_empty() {
            return Ok(self.providers[0].model.clone());
        }

        // Sort by score (descending)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored[0].0.clone())
    }

    /// Experimental A/B model selection.
    fn select_experimental(
        &self,
        control: &str,
        treatment: &str,
        split_ratio: f32,
    ) -> Result<String, RouterError> {
        // Use a deterministic counter-based approach for reproducibility
        let counter = self.experimental_counter.fetch_add(1, Ordering::SeqCst);
        let ratio_as_int = (split_ratio * 100.0) as usize;
        let use_treatment = (counter % 100) < ratio_as_int;

        if use_treatment {
            // Verify treatment model exists
            if self.providers.iter().any(|p| p.model == treatment) {
                Ok(treatment.to_string())
            } else {
                Ok(control.to_string())
            }
        } else {
            Ok(control.to_string())
        }
    }

    /// Get a provider for the specified model.
    fn get_provider(&self, model: &str) -> Option<&Arc<dyn LlmProvider>> {
        self.providers
            .iter()
            .find(|p| p.model == model)
            .map(|p| &p.provider)
    }

    /// Execute request with fallback chain on failure.
    async fn execute_with_fallback(
        &self,
        request: &GenerationRequest,
        primary_model: &str,
    ) -> Result<GenerationResponse, RouterError> {
        // Build the chain: primary model first, then fallback chain
        let mut chain: Vec<&str> = vec![primary_model];
        for model in &self.fallback_chain {
            if model != primary_model {
                chain.push(model);
            }
        }

        let mut last_error = None;

        for model in chain {
            if let Some(provider) = self.get_provider(model) {
                // Create request with the specific model
                let model_request = GenerationRequest {
                    model: model.to_string(),
                    messages: request.messages.clone(),
                    temperature: request.temperature,
                    max_tokens: request.max_tokens,
                    top_p: request.top_p,
                    response_format: request.response_format.clone(),
                    tools: request.tools.clone(),
                    tool_choice: request.tool_choice.clone(),
                };

                match provider.generate(model_request).await {
                    Ok(response) => {
                        // Record usage if we have capabilities
                        if let Some(caps) = self.model_capabilities.get(model) {
                            self.cost_tracker.record_usage(
                                model,
                                response.usage.prompt_tokens,
                                response.usage.completion_tokens,
                                caps.cost_per_1m_input,
                                caps.cost_per_1m_output,
                            );
                        }
                        return Ok(response);
                    }
                    Err(e) => {
                        tracing::warn!(
                            model = model,
                            error = %e,
                            "Provider failed, trying next in fallback chain"
                        );
                        last_error = Some(e);
                    }
                }
            }
        }

        Err(RouterError::AllProvidersFailed(
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "No providers tried".to_string()),
        ))
    }
}

#[async_trait]
impl LlmRouter for MultiModelRouter {
    async fn route(
        &self,
        request: &GenerationRequest,
        task_hint: Option<&TaskHint>,
    ) -> Result<GenerationResponse, RouterError> {
        // Check budget before routing
        if self.cost_tracker.is_over_budget() {
            return Err(RouterError::BudgetExceeded {
                daily: self.cost_tracker.is_over_daily_budget(),
                monthly: self.cost_tracker.is_over_monthly_budget(),
            });
        }

        // Select model based on strategy
        let model = self.select_model(request, task_hint)?;

        tracing::debug!(
            model = %model,
            strategy = ?self.strategy,
            "Selected model for request"
        );

        // Execute with fallback chain
        self.execute_with_fallback(request, &model).await
    }

    fn available_models(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.model.clone()).collect()
    }
}

/// Estimate token count for a string.
/// Uses simple heuristic: ~4 characters per token for English text.
fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f32 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, Message, Usage};
    use std::sync::Mutex;

    /// Mock provider for testing.
    struct MockProvider {
        #[allow(dead_code)]
        model: String,
        response: Mutex<Option<GenerationResponse>>,
        should_fail: Mutex<bool>,
    }

    impl MockProvider {
        fn new(model: &str) -> Self {
            Self {
                model: model.to_string(),
                response: Mutex::new(Some(GenerationResponse {
                    id: "test-id".to_string(),
                    model: model.to_string(),
                    choices: vec![Choice {
                        index: 0,
                        message: Message::assistant("Test response"),
                        finish_reason: "stop".to_string(),
                    }],
                    usage: Usage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                    },
                })),
                should_fail: Mutex::new(false),
            }
        }

        fn set_should_fail(&self, fail: bool) {
            *self.should_fail.lock().expect("lock poisoned") = fail;
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, LlmError> {
            if *self.should_fail.lock().expect("lock poisoned") {
                return Err(LlmError::RequestFailed("Mock failure".to_string()));
            }

            let response = self.response.lock().expect("lock poisoned").clone();
            response.ok_or_else(|| LlmError::RequestFailed("No response configured".to_string()))
        }
    }

    #[test]
    fn test_task_hint_builder() {
        let hint = TaskHint::new()
            .with_category("code_generation")
            .with_difficulty("hard")
            .with_estimated_tokens(1000)
            .with_long_context()
            .with_code_generation();

        assert_eq!(hint.category.as_deref(), Some("code_generation"));
        assert_eq!(hint.difficulty.as_deref(), Some("hard"));
        assert_eq!(hint.estimated_tokens, Some(1000));
        assert!(hint.requires_long_context);
        assert!(hint.requires_code_generation);
    }

    #[test]
    fn test_model_capabilities_builder() {
        let caps = ModelCapabilities::new("test-model")
            .with_max_context(32000)
            .with_coding_score(0.9)
            .with_reasoning_score(0.85)
            .with_speed_score(0.7)
            .with_pricing(3.0, 15.0);

        assert_eq!(caps.name, "test-model");
        assert_eq!(caps.max_context, 32000);
        assert!((caps.coding_score - 0.9).abs() < 0.001);
        assert!((caps.reasoning_score - 0.85).abs() < 0.001);
        assert!((caps.speed_score - 0.7).abs() < 0.001);
        assert!((caps.cost_per_1m_input - 3.0).abs() < 0.001);
        assert!((caps.cost_per_1m_output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_model_capabilities_estimate_cost() {
        let caps = ModelCapabilities::new("test-model").with_pricing(3.0, 15.0);

        // 1M input at $3 + 500K output at $15 = $3 + $7.5 = $10.5
        let cost = caps.estimate_cost(1_000_000, 500_000);
        assert!((cost - 10.5).abs() < 0.01);
    }

    #[test]
    fn test_router_new() {
        let router = MultiModelRouter::new(RoutingStrategy::RoundRobin);
        assert!(router.providers.is_empty());
        assert!(router.model_capabilities.is_empty());
        assert!(router.fallback_chain.is_empty());
    }

    #[test]
    fn test_router_add_provider() {
        let mut router = MultiModelRouter::new(RoutingStrategy::RoundRobin);
        let provider = Arc::new(MockProvider::new("test-model"));

        router.add_provider(provider, "test-model");

        assert_eq!(router.providers.len(), 1);
        assert_eq!(router.available_models(), vec!["test-model"]);
    }

    #[test]
    fn test_router_add_capabilities() {
        let mut router = MultiModelRouter::new(RoutingStrategy::RoundRobin);
        let caps = ModelCapabilities::new("test-model").with_max_context(32000);

        router.add_model_capabilities(caps);

        assert!(router.model_capabilities.contains_key("test-model"));
    }

    #[tokio::test]
    async fn test_router_round_robin() {
        let mut router = MultiModelRouter::new(RoutingStrategy::RoundRobin);

        let provider1 = Arc::new(MockProvider::new("model-a"));
        let provider2 = Arc::new(MockProvider::new("model-b"));

        router.add_provider(provider1, "model-a");
        router.add_provider(provider2, "model-b");

        let request = GenerationRequest::new("", vec![Message::user("test")]);

        // First request
        let result1 = router.route(&request, None).await;
        assert!(result1.is_ok());

        // Second request should go to different model
        let result2 = router.route(&request, None).await;
        assert!(result2.is_ok());

        // Third request should cycle back
        let result3 = router.route(&request, None).await;
        assert!(result3.is_ok());
    }

    #[tokio::test]
    async fn test_router_fallback_on_failure() {
        let mut router = MultiModelRouter::new(RoutingStrategy::RoundRobin);

        let provider1 = Arc::new(MockProvider::new("model-a"));
        provider1.set_should_fail(true);

        let provider2 = Arc::new(MockProvider::new("model-b"));

        router.add_provider(provider1, "model-a");
        router.add_provider(provider2, "model-b");
        router.set_fallback_chain(vec!["model-a".to_string(), "model-b".to_string()]);

        let request = GenerationRequest::new("", vec![Message::user("test")]);

        // Should succeed via fallback to model-b
        let result = router.route(&request, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_router_budget_exceeded() {
        let cost_tracker = Arc::new(CostTracker::new(0.001, 1000.0));

        // Exceed the daily budget
        cost_tracker.record_usage("test", 1_000_000, 0, 1.0, 0.0);

        let router = MultiModelRouter::with_cost_tracker(RoutingStrategy::RoundRobin, cost_tracker);

        let request = GenerationRequest::new("test-model", vec![Message::user("test")]);

        let result = router.route(&request, None).await;
        assert!(matches!(result, Err(RouterError::BudgetExceeded { .. })));
    }

    #[tokio::test]
    async fn test_router_no_providers() {
        let router = MultiModelRouter::new(RoutingStrategy::RoundRobin);
        let request = GenerationRequest::new("test-model", vec![Message::user("test")]);

        let result = router.route(&request, None).await;
        assert!(matches!(result, Err(RouterError::NoProviders)));
    }

    #[test]
    fn test_estimate_tokens() {
        // 20 characters / 4 = 5 tokens
        assert_eq!(estimate_tokens("Hello, world! Test."), 5);

        // Empty string
        assert_eq!(estimate_tokens(""), 0);

        // Long text
        let long_text = "a".repeat(1000);
        assert_eq!(estimate_tokens(&long_text), 250);
    }

    #[test]
    fn test_routing_strategy_default() {
        let strategy = RoutingStrategy::default();
        assert!(matches!(strategy, RoutingStrategy::RoundRobin));
    }

    #[test]
    fn test_select_experimental() {
        let mut router = MultiModelRouter::new(RoutingStrategy::Experimental {
            control: "model-a".to_string(),
            treatment: "model-b".to_string(),
            split_ratio: 0.5,
        });

        let provider_a = Arc::new(MockProvider::new("model-a"));
        let provider_b = Arc::new(MockProvider::new("model-b"));

        router.add_provider(provider_a, "model-a");
        router.add_provider(provider_b, "model-b");

        // Run multiple selections and verify we get both
        let mut got_a = false;
        let mut got_b = false;

        for _ in 0..100 {
            let request = GenerationRequest::new("", vec![Message::user("test")]);
            let model = router
                .select_model(&request, None)
                .expect("should select model");
            if model == "model-a" {
                got_a = true;
            } else if model == "model-b" {
                got_b = true;
            }
        }

        assert!(
            got_a && got_b,
            "Should get both models in experimental mode"
        );
    }
}
