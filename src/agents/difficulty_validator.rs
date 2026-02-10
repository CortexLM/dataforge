//! Difficulty Validator Agent for the multi-agent validation system.
//!
//! This agent uses LLM to evaluate whether a generated task matches
//! the expected difficulty level.

use std::sync::Arc;

use crate::difficulty::DifficultyLevel;
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};
use super::types::{GeneratedTask, ValidationResult};

/// System prompt for difficulty validation.
const DIFFICULTY_VALIDATION_SYSTEM_PROMPT: &str = r#"You are an expert task difficulty evaluator for benchmark tasks designed to test AI agents on terminal/CLI skills.

Your job is to analyze a task and determine if its difficulty matches the expected level.

Difficulty Levels:
- EASY: Simple, single-step tasks requiring basic commands. Expected completion time: 30s-2min. 1-3 command steps.
- MEDIUM: Multi-step tasks requiring command chaining or moderate complexity. Expected completion time: 2-10min. 3-8 command steps.
- HARD: Complex tasks requiring advanced knowledge, debugging, or multi-step problem solving. Expected completion time: 10-30min. 8-20 command steps.

Evaluation Criteria:
1. Number of distinct steps required
2. Level of domain knowledge needed
3. Complexity of command syntax
4. Need for troubleshooting or iteration
5. Clarity vs ambiguity of instructions

Output Format:
You MUST respond with ONLY a JSON object in this exact format:
{
  "score": <float between 0.0 and 1.0>,
  "matches_difficulty": <true or false>,
  "reasoning": "<detailed explanation>",
  "estimated_steps": <integer>,
  "issues": ["<issue1>", "<issue2>"]
}

The score represents how well the task matches the expected difficulty:
- 1.0: Perfect match
- 0.7-0.9: Good match with minor concerns
- 0.5-0.7: Acceptable but questionable
- Below 0.5: Mismatch

Do not include any text outside the JSON object."#;

/// User prompt template for difficulty validation.
const DIFFICULTY_VALIDATION_USER_TEMPLATE: &str = r#"Evaluate if the following task matches the expected difficulty level.

Expected Difficulty: {difficulty}
Expected Characteristics:
- Time Range: {time_range}
- Command Steps: {steps_range}
- Target Success Rate: {success_rate}%

Task Information:
- Task ID: {task_id}
- Category: {category} / {subcategory}
- Template: {template_id}

Task Instructions:
{instruction}

Analyze this task and determine if it truly matches the {difficulty} difficulty level."#;

/// Configuration for the Difficulty Validator Agent.
#[derive(Debug, Clone)]
pub struct DifficultyValidatorConfig {
    /// Minimum score threshold for passing validation.
    pub pass_threshold: f64,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for DifficultyValidatorConfig {
    fn default() -> Self {
        Self {
            pass_threshold: 0.7,
            temperature: 0.3,
            max_tokens: 1000,
        }
    }
}

impl DifficultyValidatorConfig {
    /// Creates a new configuration with custom threshold.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.pass_threshold = threshold;
        self
    }
}

/// Difficulty Validator Agent that uses LLM to assess task difficulty.
pub struct DifficultyValidatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: DifficultyValidatorConfig,
}

impl std::fmt::Debug for DifficultyValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DifficultyValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DifficultyValidatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "difficulty_validator";

    /// Creates a new difficulty validator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: DifficultyValidatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new difficulty validator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, DifficultyValidatorConfig::default())
    }

    /// Validates whether a task matches the expected difficulty level.
    ///
    /// # Arguments
    ///
    /// * `task` - The generated task to validate
    /// * `expected_difficulty` - The expected difficulty level
    ///
    /// # Returns
    ///
    /// A `ValidationResult` containing the score and reasoning.
    pub async fn validate_difficulty(
        &self,
        task: &GeneratedTask,
        expected_difficulty: DifficultyLevel,
    ) -> AgentResult<ValidationResult> {
        let prompt = self.build_prompt(task, expected_difficulty);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(DIFFICULTY_VALIDATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_response(content)
    }

    /// Builds the user prompt for difficulty validation.
    fn build_prompt(&self, task: &GeneratedTask, expected_difficulty: DifficultyLevel) -> String {
        let (time_min, time_max) = expected_difficulty.expected_time_range();
        let (steps_min, steps_max) = expected_difficulty.command_steps_range();
        let success_rate = (expected_difficulty.target_success_rate() * 100.0) as u32;

        let difficulty_str = match expected_difficulty {
            DifficultyLevel::Easy => "EASY",
            DifficultyLevel::Medium => "MEDIUM",
            DifficultyLevel::Hard => "HARD",
        };

        DIFFICULTY_VALIDATION_USER_TEMPLATE
            .replace("{difficulty}", difficulty_str)
            .replace("{time_range}", &format!("{}s - {}s", time_min, time_max))
            .replace("{steps_range}", &format!("{} - {}", steps_min, steps_max))
            .replace("{success_rate}", &success_rate.to_string())
            .replace("{task_id}", &task.task_id)
            .replace("{category}", &task.category)
            .replace("{subcategory}", &task.subcategory)
            .replace("{template_id}", &task.template_id)
            .replace("{instruction}", &task.instruction)
    }

    /// Parses the LLM response into a ValidationResult.
    fn parse_response(&self, content: &str) -> AgentResult<ValidationResult> {
        // Try to extract JSON from the response
        let json_content = self.extract_json(content)?;

        let parsed: DifficultyValidationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let issues = parsed.issues.unwrap_or_default();
        let details = if issues.is_empty() {
            None
        } else {
            Some(issues.join("; "))
        };

        let passed = parsed.matches_difficulty && parsed.score >= self.config.pass_threshold;

        if passed {
            Ok(ValidationResult::Success {
                message: parsed.reasoning,
                details,
                score: Some(parsed.score),
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            })
        } else {
            Ok(ValidationResult::Failure {
                message: parsed.reasoning,
                details,
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            })
        }
    }

    /// Extracts JSON from the response, handling potential markdown code blocks.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        use crate::utils::json_extraction::try_extract_json_from_response;

        let result = try_extract_json_from_response(content);

        match result {
            crate::utils::json_extraction::JsonExtractionResult::Success(json) => Ok(json),
            crate::utils::json_extraction::JsonExtractionResult::Truncated {
                partial_json,
                unclosed_braces,
                unclosed_brackets,
            } => {
                let preview_len = partial_json.len().min(200);
                let preview = &partial_json[..preview_len];
                tracing::warn!(
                    unclosed_braces = unclosed_braces,
                    unclosed_brackets = unclosed_brackets,
                    partial_preview = %preview,
                    "JSON appears truncated in LLM response"
                );
                Err(AgentError::ResponseParseError(format!(
                    "JSON appears truncated: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                    unclosed_braces, unclosed_brackets, preview
                )))
            }
            crate::utils::json_extraction::JsonExtractionResult::NotFound => {
                let trimmed = content.trim();
                let preview_len = trimmed.len().min(100);
                let preview = &trimmed[..preview_len];
                tracing::warn!(
                    content_preview = %preview,
                    "Could not find JSON in LLM response"
                );
                Err(AgentError::ResponseParseError(format!(
                    "No JSON content found in response. Content starts with: '{}'",
                    preview
                )))
            }
        }
    }
}

/// Response structure from LLM difficulty validation.
#[derive(Debug, serde::Deserialize)]
struct DifficultyValidationResponse {
    score: f64,
    matches_difficulty: bool,
    reasoning: String,
    #[serde(default)]
    #[allow(dead_code)]
    estimated_steps: Option<u32>,
    #[serde(default)]
    issues: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        response: Mutex<String>,
    }

    impl MockLlmProvider {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: Mutex::new(response.into()),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let content = self.response.lock().expect("lock not poisoned").clone();
            Ok(GenerationResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            })
        }
    }

    #[tokio::test]
    async fn test_difficulty_validation_pass() {
        let mock_response = r#"{
            "score": 0.85,
            "matches_difficulty": true,
            "reasoning": "The task requires multiple steps and moderate CLI knowledge.",
            "estimated_steps": 5,
            "issues": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Medium,
            "Find and count error lines in log files.",
        );

        let result = agent
            .validate_difficulty(&task, DifficultyLevel::Medium)
            .await
            .expect("validation should succeed");

        assert!(result.is_success());
        assert_eq!(result.score(), Some(0.85));
        assert!(result.details().is_none());
    }

    #[tokio::test]
    async fn test_difficulty_validation_fail() {
        let mock_response = r#"{
            "score": 0.4,
            "matches_difficulty": false,
            "reasoning": "This task is too simple for medium difficulty.",
            "estimated_steps": 2,
            "issues": ["Task only requires 2 steps", "No domain knowledge needed"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Medium,
            "List files in current directory.",
        );

        let result = agent
            .validate_difficulty(&task, DifficultyLevel::Medium)
            .await
            .expect("validation should succeed");

        assert!(!result.is_success());
        assert!(result.details().is_some());
    }

    #[tokio::test]
    async fn test_json_extraction_from_code_block() {
        let mock_response = r#"Here's my analysis:

```json
{
    "score": 0.9,
    "matches_difficulty": true,
    "reasoning": "Good match",
    "issues": []
}
```

The task is appropriate."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Easy,
            "Echo hello world.",
        );

        let result = agent
            .validate_difficulty(&task, DifficultyLevel::Easy)
            .await
            .expect("validation should succeed");

        assert!(result.is_success());
        assert_eq!(result.score(), Some(0.9));
    }

    #[test]
    fn test_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = DifficultyValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-medium-123",
            "log-analysis-001",
            DifficultyLevel::Medium,
            "Analyze the log file.",
        );

        let prompt = agent.build_prompt(&task, DifficultyLevel::Medium);

        assert!(prompt.contains("MEDIUM"));
        assert!(prompt.contains("test-medium-123"));
        assert!(prompt.contains("log-analysis-001"));
        assert!(prompt.contains("Analyze the log file."));
    }
}
