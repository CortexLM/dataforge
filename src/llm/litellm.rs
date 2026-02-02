//! LiteLLM-compatible client implementation for synth-bench.
//!
//! This module provides a client for interacting with LiteLLM-compatible APIs
//! for AI-assisted template generation and instruction improvement.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

use super::cache::{CachedMessage, PromptCache};
use crate::error::LlmError;

/// A message in a conversation with an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender (e.g., "system", "user", "assistant").
    pub role: String,
    /// Content of the message.
    pub content: String,
}

impl Message {
    /// Create a new system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// Create a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    /// Create a new assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Request for text generation from an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRequest {
    /// Model identifier to use for generation.
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Sampling temperature (0.0 - 2.0). Higher values = more random.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Nucleus sampling parameter (0.0 - 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

impl GenerationRequest {
    /// Create a new generation request with default parameters.
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
        }
    }

    /// Set the temperature for this request.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the max tokens for this request.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the top_p for this request.
    pub fn with_top_p(mut self, top_p: f64) -> Self {
        self.top_p = Some(top_p);
        self
    }
}

/// Response from an LLM generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResponse {
    /// Unique identifier for this response.
    pub id: String,
    /// Model that generated this response.
    pub model: String,
    /// Generated choices/completions.
    pub choices: Vec<Choice>,
    /// Token usage statistics.
    pub usage: Usage,
}

impl GenerationResponse {
    /// Get the content of the first choice, if available.
    pub fn first_content(&self) -> Option<&str> {
        self.choices.first().map(|c| c.message.content.as_str())
    }
}

/// A single generated choice from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    /// Index of this choice in the response.
    pub index: u32,
    /// Generated message.
    pub message: Message,
    /// Reason the generation stopped (e.g., "stop", "length").
    pub finish_reason: String,
}

/// Token usage statistics for a generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens generated.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

/// Trait for LLM providers that can generate text.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a response for the given request.
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse, LlmError>;
}

/// Client for LiteLLM-compatible APIs.
pub struct LiteLlmClient {
    /// Base URL for the API.
    api_base: String,
    /// Optional API key for authentication.
    api_key: Option<String>,
    /// Default model to use for requests.
    default_model: String,
    /// HTTP client for making API requests.
    http_client: Client,
}

impl LiteLlmClient {
    /// Create a new LiteLLM client with explicit configuration.
    ///
    /// # Arguments
    ///
    /// * `api_base` - Base URL for the LiteLLM API (e.g., "http://localhost:4000")
    /// * `api_key` - Optional API key for authentication
    /// * `default_model` - Default model to use when none is specified
    pub fn new(api_base: String, api_key: Option<String>, default_model: String) -> Self {
        Self {
            api_base,
            api_key,
            default_model,
            http_client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Create a new LiteLLM client pre-configured for OpenRouter.
    ///
    /// # Arguments
    ///
    /// * `api_key` - API key for OpenRouter authentication
    ///
    /// # Returns
    ///
    /// A client configured with:
    /// - api_base: "https://openrouter.ai/api/v1"
    /// - default_model: "anthropic/claude-opus-4.5"
    pub fn new_with_defaults(api_key: String) -> Self {
        Self {
            api_base: "https://openrouter.ai/api/v1".to_string(),
            api_key: Some(api_key),
            default_model: "anthropic/claude-opus-4.5".to_string(),
            http_client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Create a new LiteLLM client from environment variables.
    ///
    /// Reads the following environment variables:
    /// - `LITELLM_API_BASE`: Base URL for the API (required)
    /// - `LITELLM_API_KEY`: API key for authentication (optional)
    /// - `LITELLM_DEFAULT_MODEL`: Default model (defaults to "anthropic/claude-opus-4.5")
    ///
    /// # Errors
    ///
    /// Returns `LlmError::MissingApiBase` if `LITELLM_API_BASE` is not set.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_base = env::var("LITELLM_API_BASE").map_err(|_| LlmError::MissingApiBase)?;
        let api_key = env::var("LITELLM_API_KEY").ok();
        let default_model = env::var("LITELLM_DEFAULT_MODEL")
            .unwrap_or_else(|_| "anthropic/claude-opus-4.5".to_string());

        Ok(Self {
            api_base,
            api_key,
            default_model,
            http_client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
        })
    }

    /// Get the API base URL.
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// Get the default model.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Check if an API key is configured.
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Generate a response with caching support for system prompts.
    ///
    /// This method caches messages according to the cache configuration,
    /// which can significantly reduce token usage when the same system
    /// prompts are used across multiple conversations.
    ///
    /// # Arguments
    ///
    /// * `request` - The generation request containing messages
    /// * `cache` - The prompt cache to use for caching
    ///
    /// # Returns
    ///
    /// The generation response from the API.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use synth_bench::llm::{LiteLlmClient, PromptCache, Message, GenerationRequest};
    ///
    /// let client = LiteLlmClient::from_env()?;
    /// let cache = PromptCache::new(1000);
    ///
    /// let request = GenerationRequest::new(
    ///     "gpt-4",
    ///     vec![
    ///         Message::system("You are a helpful assistant."),
    ///         Message::user("Hello!"),
    ///     ],
    /// );
    ///
    /// // First call - system prompt cached for future use
    /// let response = client.generate_with_cache(request, &cache).await?;
    /// ```
    pub async fn generate_with_cache(
        &self,
        request: GenerationRequest,
        cache: &PromptCache,
    ) -> Result<GenerationResponse, LlmError> {
        // Cache messages according to cache configuration
        let cached_messages: Vec<CachedMessage> = request
            .messages
            .into_iter()
            .map(|msg| cache.cache_message(msg))
            .collect();

        // Log cache statistics for debugging
        let stats = cache.stats();
        tracing::debug!(
            hits = stats.hits,
            misses = stats.misses,
            hit_rate = format!("{:.2}%", stats.hit_rate() * 100.0),
            tokens_saved = stats.tokens_saved,
            "Cache stats after processing messages"
        );

        // Convert cached messages back to regular messages for API call
        let messages: Vec<Message> = cached_messages.into_iter().map(Into::into).collect();

        // Create new request with potentially cached messages
        let new_request = GenerationRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
        };

        // Delegate to the standard generate method
        self.generate(new_request).await
    }
}

/// Internal request structure for the OpenAI-compatible API.
#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
}

/// Internal response structure from the OpenAI-compatible API.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    id: String,
    model: String,
    choices: Vec<ApiChoice>,
    usage: ApiUsage,
}

/// Internal choice structure from the API response.
#[derive(Debug, Deserialize)]
struct ApiChoice {
    index: u32,
    message: ApiMessage,
    finish_reason: String,
}

/// Internal message structure from the API response.
#[derive(Debug, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

/// Internal usage structure from the API response.
#[derive(Debug, Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Error response from the API.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

/// Error detail from the API.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields kept for complete API error deserialization
struct ApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
}

#[async_trait]
impl LlmProvider for LiteLlmClient {
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse, LlmError> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let api_request = ApiRequest {
            model: model.clone(),
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
        };

        let url = format!("{}/chat/completions", self.api_base);

        let mut http_request = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://synth-bench.local")
            .header("X-Title", "synth-bench");

        if let Some(ref api_key) = self.api_key {
            http_request = http_request.header("Authorization", format!("Bearer {}", api_key));
        }

        let http_response = http_request
            .json(&api_request)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed(e.to_string()))?;

        let status = http_response.status();

        if !status.is_success() {
            let status_code = status.as_u16();

            // Try to parse error response body
            let error_text = http_response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error response".to_string());

            // Try to parse as structured error
            if let Ok(error_response) = serde_json::from_str::<ApiErrorResponse>(&error_text) {
                // Handle rate limiting specifically
                if status_code == 429 {
                    return Err(LlmError::RateLimited(error_response.error.message));
                }

                return Err(LlmError::ApiError {
                    code: status_code,
                    message: error_response.error.message,
                });
            }

            // Fall back to raw error text
            return Err(LlmError::ApiError {
                code: status_code,
                message: error_text,
            });
        }

        let api_response: ApiResponse = http_response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse API response: {}", e)))?;

        // Convert API response to GenerationResponse
        let choices = api_response
            .choices
            .into_iter()
            .map(|choice| Choice {
                index: choice.index,
                message: Message {
                    role: choice.message.role,
                    content: choice.message.content,
                },
                finish_reason: choice.finish_reason,
            })
            .collect();

        Ok(GenerationResponse {
            id: api_response.id,
            model: api_response.model,
            choices,
            usage: Usage {
                prompt_tokens: api_response.usage.prompt_tokens,
                completion_tokens: api_response.usage.completion_tokens,
                total_tokens: api_response.usage.total_tokens,
            },
        })
    }
}

/// System prompt for template generation with an LLM.
pub const TEMPLATE_GENERATION_PROMPT: &str = r#"
You are a benchmark task designer creating synthetic tasks for evaluating 
AI agents on terminal/CLI skills.

Create a task template for the following specification:

Category: {category}
Subcategory: {subcategory}
Difficulty: {difficulty}
Skills tested: {skills}

Requirements:
1. The task must be solvable using standard Linux commands
2. Include 3-5 variable parameters that can be randomized
3. Write a clear instruction that doesn't give away the solution
4. Design the task to resist memorization
5. Include expected completion time estimate

Output format: YAML template following the TaskTemplate schema.
"#;

/// System prompt for instruction improvement.
const INSTRUCTION_IMPROVEMENT_PROMPT: &str = r#"
You are a technical writer improving benchmark task instructions.

The goal is to make instructions:
1. Clear and unambiguous
2. Complete but concise
3. Neither too easy (giving away the solution) nor too vague
4. Testable with clear success criteria

Current instruction:
{instruction}

Feedback:
{feedback}

Provide an improved version of the instruction.
"#;

/// Assistant for AI-powered template generation and improvement.
pub struct TemplateAssistant {
    /// The LLM provider to use for generation.
    client: Box<dyn LlmProvider>,
}

impl TemplateAssistant {
    /// Create a new template assistant with the given LLM provider.
    pub fn new(client: Box<dyn LlmProvider>) -> Self {
        Self { client }
    }

    /// Generate a draft template based on the provided specification.
    ///
    /// # Arguments
    ///
    /// * `category` - Primary category for the task (e.g., "file_manipulation")
    /// * `subcategory` - Subcategory within the primary category
    /// * `difficulty` - Difficulty level (e.g., "easy", "medium", "hard")
    /// * `skills` - List of skills the task should test
    ///
    /// # Returns
    ///
    /// A YAML string containing the draft template.
    pub async fn generate_template_draft(
        &self,
        category: &str,
        subcategory: &str,
        difficulty: &str,
        skills: &[String],
    ) -> Result<String, LlmError> {
        let skills_str = skills.join(", ");
        let prompt = TEMPLATE_GENERATION_PROMPT
            .replace("{category}", category)
            .replace("{subcategory}", subcategory)
            .replace("{difficulty}", difficulty)
            .replace("{skills}", &skills_str);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system("You are a benchmark task designer. Output only valid YAML."),
                Message::user(prompt),
            ],
        )
        .with_temperature(0.7)
        .with_max_tokens(2000);

        let response = self.client.generate(request).await?;
        response
            .first_content()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("No content in LLM response".to_string()))
    }

    /// Improve an existing instruction based on feedback.
    ///
    /// # Arguments
    ///
    /// * `current_instruction` - The current instruction text to improve
    /// * `feedback` - Feedback describing what to improve
    ///
    /// # Returns
    ///
    /// An improved version of the instruction.
    pub async fn improve_instruction(
        &self,
        current_instruction: &str,
        feedback: &str,
    ) -> Result<String, LlmError> {
        let prompt = INSTRUCTION_IMPROVEMENT_PROMPT
            .replace("{instruction}", current_instruction)
            .replace("{feedback}", feedback);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(
                    "You are a technical writer. Output only the improved instruction text.",
                ),
                Message::user(prompt),
            ],
        )
        .with_temperature(0.5)
        .with_max_tokens(1000);

        let response = self.client.generate(request).await?;
        response
            .first_content()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("No content in LLM response".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_constructors() {
        let system = Message::system("You are helpful.");
        assert_eq!(system.role, "system");
        assert_eq!(system.content, "You are helpful.");

        let user = Message::user("Hello");
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "Hello");

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.content, "Hi there!");
    }

    #[test]
    fn test_generation_request_builder() {
        let request = GenerationRequest::new("gpt-4", vec![Message::user("test")])
            .with_temperature(0.7)
            .with_max_tokens(1000)
            .with_top_p(0.9);

        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(1000));
        assert_eq!(request.top_p, Some(0.9));
    }

    #[test]
    fn test_generation_response_first_content() {
        let response = GenerationResponse {
            id: "test-id".to_string(),
            model: "gpt-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message::assistant("Hello!"),
                finish_reason: "stop".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        assert_eq!(response.first_content(), Some("Hello!"));

        let empty_response = GenerationResponse {
            id: "test-id".to_string(),
            model: "gpt-4".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };

        assert_eq!(empty_response.first_content(), None);
    }

    #[test]
    fn test_litellm_client_new() {
        let client = LiteLlmClient::new(
            "http://localhost:4000".to_string(),
            Some("test-key".to_string()),
            "gpt-4".to_string(),
        );

        assert_eq!(client.api_base(), "http://localhost:4000");
        assert_eq!(client.default_model(), "gpt-4");
        assert!(client.has_api_key());
    }

    #[test]
    fn test_litellm_client_without_key() {
        let client = LiteLlmClient::new(
            "http://localhost:4000".to_string(),
            None,
            "gpt-4".to_string(),
        );

        assert!(!client.has_api_key());
    }

    #[test]
    fn test_litellm_client_new_with_defaults() {
        let client = LiteLlmClient::new_with_defaults("test-api-key".to_string());

        assert_eq!(client.api_base(), "https://openrouter.ai/api/v1");
        assert_eq!(client.default_model(), "anthropic/claude-opus-4.5");
        assert!(client.has_api_key());
    }

    #[tokio::test]
    async fn test_litellm_client_generate_connection_error() {
        // Test that connection errors are properly handled
        let client = LiteLlmClient::new(
            "http://localhost:65535".to_string(), // Use a port that's unlikely to have a server
            None,
            "gpt-4".to_string(),
        );

        let request = GenerationRequest::new("gpt-4", vec![Message::user("test")]);
        let result = client.generate(request).await;

        // Should return a RequestFailed error when no server is running
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::RequestFailed(_)));
    }

    #[test]
    fn test_api_request_serialization() {
        let request = ApiRequest {
            model: "gpt-4".to_string(),
            messages: vec![Message::user("test")],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            top_p: None, // Should be skipped in JSON
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"model\":\"gpt-4\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"max_tokens\":1000"));
        assert!(!json.contains("top_p")); // Should be skipped because None
    }

    #[tokio::test]
    async fn test_generate_with_cache_caches_system_prompts() {
        let client = LiteLlmClient::new(
            "http://localhost:65535".to_string(),
            None,
            "gpt-4".to_string(),
        );
        let cache = PromptCache::new(100);

        // Create a request with a system prompt
        let request = GenerationRequest::new(
            "gpt-4",
            vec![
                Message::system("You are a helpful assistant."),
                Message::user("Hello!"),
            ],
        );

        // First call - will fail due to no server, but should still cache
        let _ = client.generate_with_cache(request.clone(), &cache).await;

        // Check cache statistics - system prompt should be cached (miss on first call)
        // User messages are not cached by default, so only system prompt counts
        let stats = cache.stats();
        assert_eq!(
            stats.misses, 1,
            "First system prompt should be a cache miss"
        );
        assert_eq!(cache.len(), 1, "Only system prompt should be cached");

        // Second call with same system prompt
        let _ = client.generate_with_cache(request, &cache).await;

        // Should now have a cache hit for the system prompt
        // User message is not cached so it doesn't count as hit or miss
        let stats = cache.stats();
        assert_eq!(
            stats.hits, 1,
            "Second call should hit cache for system prompt"
        );
        assert_eq!(stats.misses, 1, "Only system prompt miss from first call");
    }

    #[test]
    fn test_cache_integration_with_messages() {
        let cache = PromptCache::new(100);

        // Test that system prompts are cached
        let msg1 = Message::system("You are helpful");
        let cached1 = cache.cache_message(msg1.clone());
        assert!(!cached1.from_cache, "First access should be a miss");

        let cached2 = cache.cache_message(msg1);
        assert!(cached2.from_cache, "Second access should be a hit");

        // Test that user messages are not cached by default
        let user_msg = Message::user("Hello");
        let cached_user1 = cache.cache_message(user_msg.clone());
        assert!(!cached_user1.from_cache);

        let cached_user2 = cache.cache_message(user_msg);
        assert!(
            !cached_user2.from_cache,
            "User messages should not be cached by default"
        );
    }
}
