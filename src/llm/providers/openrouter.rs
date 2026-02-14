//! OpenRouter provider implementation for the multi-model router.
//!
//! OpenRouter provides a unified API for accessing multiple LLM providers
//! through a single endpoint, making it ideal for multi-model routing scenarios.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::LlmError;
#[cfg(test)]
use crate::llm::ResponseFormat;
use crate::llm::{
    Choice, GenerationRequest, GenerationResponse, LlmProvider, Message, ToolCallFunction,
    ToolCallInfo, Usage,
};

/// Default OpenRouter API endpoint.
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Default model to use if none specified.
const DEFAULT_MODEL: &str = "openai/gpt-5.2-codex:nitro";

/// Maximum number of retry attempts for transient failures.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff in milliseconds.
const BASE_RETRY_DELAY_MS: u64 = 1000;

/// Request timeout in seconds (increased for large code generation).
const REQUEST_TIMEOUT_SECS: u64 = 300;

/// OpenRouter provider for LLM requests.
///
/// This provider implements the `LlmProvider` trait and routes requests
/// through OpenRouter's API, which provides access to multiple LLM providers.
pub struct OpenRouterProvider {
    /// HTTP client for making API requests.
    client: Client,
    /// API key for OpenRouter authentication.
    api_key: String,
    /// Base URL for the OpenRouter API.
    base_url: String,
    /// Default model to use when none is specified.
    default_model: String,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider with the given API key.
    ///
    /// Uses the default model (`openai/gpt-5.2-codex:nitro`) and base URL.
    ///
    /// # Arguments
    ///
    /// * `api_key` - OpenRouter API key for authentication
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()
                .expect("Failed to build HTTP client - system TLS configuration error"),
            api_key,
            base_url: OPENROUTER_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Create a new OpenRouter provider with a specific default model.
    ///
    /// # Arguments
    ///
    /// * `api_key` - OpenRouter API key for authentication
    /// * `model` - Default model identifier (e.g., "anthropic/claude-3-opus")
    pub fn with_model(api_key: String, model: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()
                .expect("Failed to build HTTP client - system TLS configuration error"),
            api_key,
            base_url: OPENROUTER_BASE_URL.to_string(),
            default_model: model,
        }
    }

    /// Create a new OpenRouter provider with custom base URL.
    ///
    /// Useful for testing or using OpenRouter-compatible proxies.
    ///
    /// # Security Warning
    ///
    /// This method allows non-HTTPS URLs for testing purposes. Using plain HTTP
    /// in production can expose API keys and request data. Always use HTTPS
    /// for production deployments.
    ///
    /// # Arguments
    ///
    /// * `api_key` - API key for authentication
    /// * `base_url` - Custom base URL for the API (should use HTTPS in production)
    /// * `model` - Default model identifier
    pub fn with_custom_url(api_key: String, base_url: String, model: String) -> Self {
        // Log warning for insecure configurations (non-HTTPS in non-localhost URLs)
        if !base_url.starts_with("https://")
            && !base_url.contains("localhost")
            && !base_url.contains("127.0.0.1")
        {
            tracing::warn!(
                "OpenRouter provider configured with non-HTTPS URL: {}. \
                 This may expose API keys and request data. Use HTTPS for production.",
                base_url
            );
        }

        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()
                .expect("Failed to build HTTP client - system TLS configuration error"),
            api_key,
            base_url,
            default_model: model,
        }
    }

    /// Get the API key (for debugging, returns masked value).
    pub fn api_key_masked(&self) -> String {
        if self.api_key.len() <= 8 {
            "*".repeat(self.api_key.len())
        } else {
            format!(
                "{}...{}",
                &self.api_key[..4],
                &self.api_key[self.api_key.len() - 4..]
            )
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the default model.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Execute a request with exponential backoff retry logic.
    async fn execute_with_retry(
        &self,
        request: &ApiRequest,
    ) -> Result<GenerationResponse, LlmError> {
        let mut last_error = None;
        let url = format!("{}/chat/completions", self.base_url);

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                // Exponential backoff: 1s, 2s, 4s
                let delay_ms = BASE_RETRY_DELAY_MS * (1 << (attempt - 1));
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                tracing::debug!(
                    attempt = attempt + 1,
                    delay_ms = delay_ms,
                    "Retrying OpenRouter request after transient failure"
                );
            }

            match self.execute_request(&url, request).await {
                Ok(response) => return Ok(response),
                Err(err) => {
                    // Only retry on transient errors
                    if is_transient_error(&err) {
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = MAX_RETRIES,
                            error = %err,
                            "Transient error, will retry"
                        );
                        last_error = Some(err);
                    } else {
                        // Non-transient errors should fail immediately
                        return Err(err);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            LlmError::RequestFailed("Max retries exceeded with no error captured".to_string())
        }))
    }

    /// Execute a single request (no retry logic).
    async fn execute_request(
        &self,
        url: &str,
        request: &ApiRequest,
    ) -> Result<GenerationResponse, LlmError> {
        let http_response = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://swe_forge.local")
            .header("X-Title", "swe_forge")
            .json(request)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed(e.to_string()))?;

        let status = http_response.status();

        if !status.is_success() {
            let status_code = status.as_u16();
            let error_text = http_response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error response".to_string());

            // Try to parse structured error response
            if let Ok(error_response) = serde_json::from_str::<ApiErrorResponse>(&error_text) {
                if status_code == 429 {
                    return Err(LlmError::RateLimited(error_response.error.message));
                }
                return Err(LlmError::ApiError {
                    code: status_code,
                    message: error_response.error.message,
                });
            }

            return Err(LlmError::ApiError {
                code: status_code,
                message: error_text,
            });
        }

        let api_response: ApiResponse = http_response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse API response: {}", e)))?;

        // Convert to GenerationResponse
        let choices = api_response
            .choices
            .into_iter()
            .map(|choice| {
                // Convert API tool_calls to our ToolCallInfo format
                let tool_calls_info = choice.message.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| ToolCallInfo {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: ToolCallFunction {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect::<Vec<_>>()
                });

                // For backwards compat: if tool_calls exist, put first arguments as content
                // so single-shot callers (quality scorer, etc.) still work via first_content()
                let content = if let Some(ref tool_calls) = choice.message.tool_calls {
                    if let Some(first_call) = tool_calls.first() {
                        if !first_call.function.arguments.is_empty() {
                            first_call.function.arguments.clone()
                        } else {
                            choice.message.content.clone()
                        }
                    } else {
                        choice.message.content.clone()
                    }
                } else if !choice.message.content.trim().is_empty() {
                    choice.message.content
                } else if let Some(rc) = choice.message.reasoning_content {
                    if !rc.trim().is_empty() {
                        rc
                    } else {
                        choice.message.reasoning.unwrap_or_default()
                    }
                } else {
                    choice.message.reasoning.unwrap_or_default()
                };

                Choice {
                    index: choice.index,
                    message: Message {
                        role: choice.message.role,
                        content,
                        tool_calls: tool_calls_info,
                        tool_call_id: None,
                    },
                    finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".to_string()),
                }
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

/// Check if an error is transient and should be retried.
fn is_transient_error(error: &LlmError) -> bool {
    match error {
        LlmError::RequestFailed(msg) => {
            // Network errors, timeouts, connection issues
            msg.contains("timeout")
                || msg.contains("connection")
                || msg.contains("temporarily")
                || msg.contains("Connection refused")
        }
        LlmError::RateLimited(_) => true,
        LlmError::ApiError { code, .. } => {
            // Server errors (5xx) and rate limits are transient
            *code >= 500 || *code == 429
        }
        _ => false,
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse, LlmError> {
        let model = if request.model.is_empty() || request.model == "default" {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let has_structured_output = request.response_format.is_some();

        // Don't use reasoning mode when structured output is requested --
        // reasoning tokens interfere with strict JSON output.
        let (reasoning, adjusted_max_tokens) =
            if is_reasoning_model(&model) && !has_structured_output {
                let user_max_tokens = request.max_tokens.unwrap_or(4000);
                let reasoning_budget = 8000u32;
                let min_total_tokens = 16000u32;
                let adjusted = (user_max_tokens + reasoning_budget).max(min_total_tokens);

                (
                    Some(ReasoningConfig {
                        effort: Some("medium".to_string()),
                        max_tokens: None,
                    }),
                    Some(adjusted),
                )
            } else {
                (None, request.max_tokens)
            };

        let response_format = request
            .response_format
            .map(|rf| serde_json::to_value(&rf).unwrap_or(serde_json::Value::Null));

        let tools = request
            .tools
            .map(|t| serde_json::to_value(&t).unwrap_or(serde_json::Value::Null));
        let tool_choice = request
            .tool_choice
            .map(|tc| serde_json::to_value(&tc).unwrap_or(serde_json::Value::Null));

        let api_request = ApiRequest {
            model,
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: adjusted_max_tokens,
            top_p: request.top_p,
            reasoning,
            response_format,
            tools,
            tool_choice,
        };

        self.execute_with_retry(&api_request).await
    }
}

/// Known reasoning model identifiers that should have reasoning enabled.
/// These are exact model prefixes/names to avoid false positives.
const REASONING_MODEL_PATTERNS: &[&str] = &[
    // Kimi models (Moonshot AI)
    "moonshotai/kimi",
    "kimi-k2",
    "kimi-k1",
    // OpenAI reasoning models (must be exact to avoid matching 'pro1', 'collection-o1', etc.)
    "openai/o1",
    "openai/o3",
    "o1-preview",
    "o1-mini",
    "o3-mini",
    // DeepSeek reasoning models
    "deepseek/deepseek-r1",
    "deepseek-r1",
    "deepseek-reasoner",
    // Generic reasoning indicators (full word boundaries)
    "-thinking",
    "-reasoning",
];

/// Check if a model is a reasoning model that needs special handling.
///
/// Uses explicit model patterns to avoid false positives (e.g., 'pro1-turbo'
/// should not match 'o1', 'model-collection-o1' should not match 'o1').
fn is_reasoning_model(model: &str) -> bool {
    let model_lower = model.to_lowercase();

    REASONING_MODEL_PATTERNS.iter().any(|pattern| {
        let pattern_lower = pattern.to_lowercase();
        // Check if the model name contains this pattern
        // For patterns starting with specific providers (e.g., 'openai/'), match from start
        if pattern_lower.contains('/') {
            model_lower.starts_with(&pattern_lower)
        } else if pattern_lower.starts_with('-') {
            // For suffix patterns (e.g., '-thinking'), check if it's at the end or followed by delimiter
            model_lower.ends_with(&pattern_lower)
                || model_lower.contains(&format!("{}-", pattern_lower))
        } else {
            // For standalone patterns, ensure they're at word boundaries
            // Match: "kimi-k2", "kimi-k2.5", "kimi-k2-preview"
            // Don't match: "akimi", "kimix", etc.
            let idx = model_lower.find(&pattern_lower);
            if let Some(start) = idx {
                let end = start + pattern_lower.len();
                // Check if pattern is at a word boundary
                let at_start = start == 0
                    || !model_lower
                        .chars()
                        .nth(start - 1)
                        .map(|c| c.is_alphanumeric())
                        .unwrap_or(false);
                let at_end = end == model_lower.len()
                    || !model_lower
                        .chars()
                        .nth(end)
                        .map(|c| c.is_alphanumeric())
                        .unwrap_or(false);
                at_start && at_end
            } else {
                false
            }
        }
    })
}

/// Internal request structure for the OpenRouter API.
#[derive(Debug, Clone, Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

/// Configuration for reasoning-enabled models.
/// Supports both effort-based (OpenAI-style) and max_tokens-based (Anthropic-style) configuration.
#[derive(Debug, Clone, Serialize)]
struct ReasoningConfig {
    /// Reasoning effort level (OpenAI-style): "xhigh", "high", "medium", "low", "minimal", "none"
    /// This is more broadly compatible across providers via OpenRouter.
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    /// Maximum tokens for reasoning output (Anthropic-style).
    /// Note: Not all providers support this directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// Internal response structure from the OpenRouter API.
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
    finish_reason: Option<String>,
}

/// Internal message structure from the API response.
/// Supports reasoning models that may include reasoning_content/reasoning.
#[derive(Debug, Deserialize)]
struct ApiMessage {
    role: String,
    /// The main content - may be empty for reasoning models or tool calls.
    #[serde(default)]
    content: String,
    /// Reasoning text from reasoning models (e.g., Kimi K2.5, DeepSeek R1).
    #[serde(default)]
    reasoning: Option<String>,
    /// Reasoning content from reasoning models (e.g., Kimi K2.5).
    #[serde(default)]
    reasoning_content: Option<String>,
    /// Tool calls returned by the model (function calling).
    #[serde(default)]
    tool_calls: Option<Vec<ApiToolCall>>,
}

/// A tool call returned by the model.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiToolCall {
    #[serde(default)]
    id: String,
    #[serde(rename = "type", default)]
    _tool_type: String,
    function: ApiToolCallFunction,
}

/// Function details within a tool call response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiToolCallFunction {
    #[serde(default)]
    name: String,
    /// JSON string of function arguments - this is our structured output.
    #[serde(default)]
    arguments: String,
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
struct ApiErrorDetail {
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openrouter_provider_new() {
        let provider = OpenRouterProvider::new("test-api-key".to_string());

        assert_eq!(provider.base_url(), OPENROUTER_BASE_URL);
        assert_eq!(provider.default_model(), DEFAULT_MODEL);
        assert_eq!(provider.api_key_masked(), "test...-key");
    }

    #[test]
    fn test_openrouter_provider_with_model() {
        let provider = OpenRouterProvider::with_model(
            "test-key".to_string(),
            "anthropic/claude-3".to_string(),
        );

        assert_eq!(provider.default_model(), "anthropic/claude-3");
    }

    #[test]
    fn test_openrouter_provider_with_custom_url() {
        let provider = OpenRouterProvider::with_custom_url(
            "test-key".to_string(),
            "https://custom.api.com/v1".to_string(),
            "custom-model".to_string(),
        );

        assert_eq!(provider.base_url(), "https://custom.api.com/v1");
        assert_eq!(provider.default_model(), "custom-model");
    }

    #[test]
    fn test_api_key_masked_short() {
        let provider = OpenRouterProvider::new("abc".to_string());
        assert_eq!(provider.api_key_masked(), "***");
    }

    #[test]
    fn test_api_key_masked_normal() {
        let provider = OpenRouterProvider::new("sk-1234567890abcdef".to_string());
        assert_eq!(provider.api_key_masked(), "sk-1...cdef");
    }

    #[test]
    fn test_is_transient_error_rate_limited() {
        let error = LlmError::RateLimited("Too many requests".to_string());
        assert!(is_transient_error(&error));
    }

    #[test]
    fn test_is_transient_error_server_error() {
        let error = LlmError::ApiError {
            code: 500,
            message: "Internal server error".to_string(),
        };
        assert!(is_transient_error(&error));
    }

    #[test]
    fn test_is_transient_error_client_error() {
        let error = LlmError::ApiError {
            code: 400,
            message: "Bad request".to_string(),
        };
        assert!(!is_transient_error(&error));
    }

    #[test]
    fn test_is_transient_error_timeout() {
        let error = LlmError::RequestFailed("Request timeout".to_string());
        assert!(is_transient_error(&error));
    }

    #[test]
    fn test_is_transient_error_connection() {
        let error = LlmError::RequestFailed("Connection refused".to_string());
        assert!(is_transient_error(&error));
    }

    #[test]
    fn test_is_transient_error_parse_error() {
        let error = LlmError::ParseError("Invalid JSON".to_string());
        assert!(!is_transient_error(&error));
    }

    #[tokio::test]
    async fn test_generate_connection_error() {
        let provider = OpenRouterProvider::with_custom_url(
            "test-key".to_string(),
            "http://localhost:65535".to_string(),
            "test-model".to_string(),
        );

        let request = GenerationRequest::new("test-model", vec![Message::user("test")]);
        let result = provider.generate(request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::RequestFailed(_)));
    }

    #[test]
    fn test_api_request_serialization() {
        let request = ApiRequest {
            model: "test-model".to_string(),
            messages: vec![Message::user("Hello")],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            top_p: None,
            reasoning: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"max_tokens\":1000"));
        assert!(!json.contains("top_p"));
    }

    #[test]
    fn test_api_request_serialization_with_reasoning_effort() {
        let request = ApiRequest {
            model: "openai/gpt-5.2-codex:nitro".to_string(),
            messages: vec![Message::user("Hello")],
            temperature: Some(0.7),
            max_tokens: Some(16000),
            top_p: None,
            reasoning: Some(ReasoningConfig {
                effort: Some("medium".to_string()),
                max_tokens: None,
            }),
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"reasoning\""));
        assert!(json.contains("\"effort\":\"medium\""));
    }

    #[test]
    fn test_api_request_serialization_with_reasoning_max_tokens() {
        let request = ApiRequest {
            model: "anthropic/claude-3.5-sonnet".to_string(),
            messages: vec![Message::user("Hello")],
            temperature: Some(0.7),
            max_tokens: Some(10000),
            top_p: None,
            reasoning: Some(ReasoningConfig {
                effort: None,
                max_tokens: Some(8000),
            }),
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"reasoning\""));
        assert!(json.contains("\"max_tokens\":8000"));
    }

    #[test]
    fn test_api_request_serialization_with_response_format() {
        let rf = ResponseFormat::JsonSchema {
            json_schema: crate::llm::JsonSchemaSpec {
                name: "test".to_string(),
                strict: true,
                schema: serde_json::json!({
                    "type": "object",
                    "properties": { "score": { "type": "number" } },
                    "required": ["score"],
                    "additionalProperties": false
                }),
            },
        };
        let request = ApiRequest {
            model: "test-model".to_string(),
            messages: vec![Message::user("Hello")],
            temperature: Some(0.3),
            max_tokens: Some(1000),
            top_p: None,
            reasoning: None,
            response_format: Some(serde_json::to_value(&rf).unwrap()),
            tools: None,
            tool_choice: None,
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"response_format\""));
        assert!(json.contains("\"json_schema\""));
        assert!(json.contains("\"strict\":true"));
    }

    // Tests for is_reasoning_model() function
    #[test]
    fn test_is_reasoning_model_kimi_models() {
        // Should match Kimi models
        assert!(is_reasoning_model("moonshotai/kimi-k2.5"));
        assert!(is_reasoning_model("moonshotai/kimi-k2"));
        assert!(is_reasoning_model("moonshotai/kimi-k1.5"));
        assert!(is_reasoning_model("kimi-k2.5"));
        assert!(is_reasoning_model("kimi-k2-preview"));
        // Case insensitive
        assert!(is_reasoning_model("MOONSHOTAI/KIMI-K2.5"));
    }

    #[test]
    fn test_is_reasoning_model_openai_o1_o3() {
        // Should match OpenAI reasoning models
        assert!(is_reasoning_model("openai/o1"));
        assert!(is_reasoning_model("openai/o1-preview"));
        assert!(is_reasoning_model("openai/o1-mini"));
        assert!(is_reasoning_model("openai/o3"));
        assert!(is_reasoning_model("openai/o3-mini"));
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o3-mini"));
    }

    #[test]
    fn test_is_reasoning_model_deepseek() {
        // Should match DeepSeek reasoning models
        assert!(is_reasoning_model("deepseek/deepseek-r1"));
        assert!(is_reasoning_model("deepseek-r1"));
        assert!(is_reasoning_model("deepseek-r1-distill"));
        assert!(is_reasoning_model("deepseek-reasoner"));
    }

    #[test]
    fn test_is_reasoning_model_suffix_patterns() {
        // Should match models with thinking/reasoning suffix
        assert!(is_reasoning_model("model-thinking"));
        assert!(is_reasoning_model("claude-thinking"));
        assert!(is_reasoning_model("model-reasoning"));
    }

    #[test]
    fn test_is_reasoning_model_false_positives() {
        // Should NOT match these (false positive prevention)
        assert!(!is_reasoning_model("pro1-turbo")); // 'o1' is part of 'pro1'
        assert!(!is_reasoning_model("model-collection-o1")); // 'o1' is not a standalone prefix
        assert!(!is_reasoning_model("gpt-4o")); // 'o' is not 'o1'
        assert!(!is_reasoning_model("gpt-4")); // No reasoning pattern
        assert!(!is_reasoning_model("anthropic/claude-3-opus")); // No reasoning pattern
        assert!(!is_reasoning_model("llama-3.1-70b")); // No reasoning pattern
        assert!(!is_reasoning_model("akimi-model")); // 'kimi' is not at word boundary
        assert!(!is_reasoning_model("kimiex-model")); // 'kimi' is not at word boundary
    }

    #[test]
    fn test_is_reasoning_model_standard_models() {
        // Standard models should not be reasoning models
        assert!(!is_reasoning_model("gpt-4-turbo"));
        assert!(!is_reasoning_model("gpt-3.5-turbo"));
        assert!(!is_reasoning_model("anthropic/claude-3-sonnet"));
        assert!(!is_reasoning_model("meta-llama/llama-3.1-8b"));
        assert!(!is_reasoning_model("google/gemini-pro"));
        assert!(!is_reasoning_model("mistral/mistral-large"));
    }

    // Tests for reasoning content extraction logic
    #[test]
    fn test_api_message_deserialization_content_only() {
        let json = r#"{"role": "assistant", "content": "Hello there!"}"#;
        let message: ApiMessage = serde_json::from_str(json).expect("should parse");
        assert_eq!(message.content, "Hello there!");
        assert!(message.reasoning.is_none());
        assert!(message.reasoning_content.is_none());
    }

    #[test]
    fn test_api_message_deserialization_with_reasoning_content() {
        let json =
            r#"{"role": "assistant", "content": "", "reasoning_content": "Let me think..."}"#;
        let message: ApiMessage = serde_json::from_str(json).expect("should parse");
        assert_eq!(message.content, "");
        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("Let me think...")
        );
    }

    #[test]
    fn test_api_message_deserialization_with_reasoning() {
        let json = r#"{"role": "assistant", "content": "", "reasoning": "Step by step..."}"#;
        let message: ApiMessage = serde_json::from_str(json).expect("should parse");
        assert_eq!(message.content, "");
        assert_eq!(message.reasoning.as_deref(), Some("Step by step..."));
    }

    #[test]
    fn test_api_message_deserialization_all_fields() {
        let json = r#"{"role": "assistant", "content": "Final answer", "reasoning": "Step 1", "reasoning_content": "Step 2"}"#;
        let message: ApiMessage = serde_json::from_str(json).expect("should parse");
        assert_eq!(message.content, "Final answer");
        assert_eq!(message.reasoning.as_deref(), Some("Step 1"));
        assert_eq!(message.reasoning_content.as_deref(), Some("Step 2"));
    }

    #[test]
    fn test_reasoning_content_extraction_priority() {
        // Test the extraction priority: content > reasoning_content > reasoning
        // This simulates what execute_request does when processing ApiChoice

        // Case 1: Content has value - use content
        let content_result =
            extract_reasoning_content("Main content", Some("reasoning_content"), Some("reasoning"));
        assert_eq!(content_result, "Main content");

        // Case 2: Content empty, reasoning_content has value - use reasoning_content
        let rc_result =
            extract_reasoning_content("", Some("reasoning_content value"), Some("reasoning"));
        assert_eq!(rc_result, "reasoning_content value");

        // Case 3: Content empty, reasoning_content empty, reasoning has value - use reasoning
        let r_result = extract_reasoning_content("", Some(""), Some("reasoning value"));
        assert_eq!(r_result, "reasoning value");

        // Case 4: All empty
        let empty_result = extract_reasoning_content("", None, None);
        assert_eq!(empty_result, "");
    }

    /// Helper function to test reasoning content extraction logic
    /// (mirrors the logic in execute_request)
    fn extract_reasoning_content(
        content: &str,
        reasoning_content: Option<&str>,
        reasoning: Option<&str>,
    ) -> String {
        if !content.trim().is_empty() {
            content.to_string()
        } else if let Some(rc) = reasoning_content {
            if !rc.trim().is_empty() {
                rc.to_string()
            } else {
                reasoning.unwrap_or_default().to_string()
            }
        } else {
            reasoning.unwrap_or_default().to_string()
        }
    }
}
