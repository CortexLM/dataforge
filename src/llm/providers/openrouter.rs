//! OpenRouter provider implementation for the multi-model router.
//!
//! OpenRouter provides a unified API for accessing multiple LLM providers
//! through a single endpoint, making it ideal for multi-model routing scenarios.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::LlmError;
use crate::llm::{Choice, GenerationRequest, GenerationResponse, LlmProvider, Message, Usage};

/// Default OpenRouter API endpoint.
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Default model to use if none specified.
const DEFAULT_MODEL: &str = "moonshotai/kimi-k2.5";

/// Maximum number of retry attempts for transient failures.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff in milliseconds.
const BASE_RETRY_DELAY_MS: u64 = 1000;

/// Request timeout in seconds.
const REQUEST_TIMEOUT_SECS: u64 = 120;

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
    /// Uses the default model (`moonshotai/kimi-k2.5`) and base URL.
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
    /// # Arguments
    ///
    /// * `api_key` - API key for authentication
    /// * `base_url` - Custom base URL for the API
    /// * `model` - Default model identifier
    pub fn with_custom_url(api_key: String, base_url: String, model: String) -> Self {
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
            .header("HTTP-Referer", "https://dataforge.local")
            .header("X-Title", "dataforge")
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
            .map(|choice| Choice {
                index: choice.index,
                message: Message {
                    role: choice.message.role,
                    content: choice.message.content,
                },
                finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".to_string()),
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
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let api_request = ApiRequest {
            model,
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
        };

        self.execute_with_retry(&api_request).await
    }
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
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"max_tokens\":1000"));
        assert!(!json.contains("top_p"));
    }
}
