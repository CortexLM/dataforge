//! Feasibility Validator Agent for the multi-agent validation system.
//!
//! This agent uses LLM to verify that a task is solvable but not trivial,
//! ensuring the task provides an appropriate challenge.

use std::sync::Arc;

use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};
use super::types::{GeneratedTask, ValidationResult};

/// System prompt for feasibility validation.
const FEASIBILITY_VALIDATION_SYSTEM_PROMPT: &str = r#"You are an expert task feasibility evaluator for benchmark tasks designed to test AI agents on terminal/CLI skills.

Your job is to analyze a task and determine:
1. Is the task SOLVABLE? (Can it be completed with standard Linux tools?)
2. Is the task NOT TRIVIAL? (Does it require actual problem-solving?)
3. Are the instructions CLEAR? (Can the task be understood without ambiguity?)
4. Is the task SELF-CONTAINED? (Does it have all necessary information?)

Evaluation Criteria:
- Solvability: The task must be achievable using standard Linux CLI tools (bash, grep, awk, sed, find, etc.)
- Non-triviality: The task should require actual thinking, not just copy-paste commands
- Clarity: Instructions must be unambiguous and complete
- Independence: The task should not require external resources or APIs

Red Flags for IMPOSSIBLE tasks:
- Requires proprietary software not available in standard Linux
- References files or systems that don't exist
- Contains logical contradictions
- Requires internet access when not explicitly provided

Red Flags for TRIVIAL tasks:
- Can be solved with a single obvious command
- Solution is directly stated in the instructions
- No reasoning or analysis required

Output Format:
You MUST respond with ONLY a JSON object in this exact format:
{
  "score": <float between 0.0 and 1.0>,
  "is_solvable": <true or false>,
  "is_non_trivial": <true or false>,
  "is_clear": <true or false>,
  "reasoning": "<detailed explanation>",
  "solvability_notes": "<explanation of why solvable or not>",
  "complexity_notes": "<explanation of complexity level>",
  "issues": ["<issue1>", "<issue2>"]
}

Scoring:
- 1.0: Excellent task - solvable, appropriately challenging, clear instructions
- 0.8-0.9: Good task with minor concerns
- 0.6-0.8: Acceptable but needs improvement
- Below 0.6: Problematic - too easy, too hard, or unclear

Do not include any text outside the JSON object."#;

/// User prompt template for feasibility validation.
const FEASIBILITY_VALIDATION_USER_TEMPLATE: &str = r#"Evaluate the feasibility of the following task.

Task Information:
- Task ID: {task_id}
- Difficulty: {difficulty}
- Category: {category} / {subcategory}
- Template: {template_id}

Task Instructions:
{instruction}

Task Parameters:
{parameters}

Analyze this task for:
1. Solvability - Can this be completed with standard Linux tools?
2. Non-triviality - Does this require actual problem-solving?
3. Clarity - Are the instructions clear and unambiguous?
4. Completeness - Is all necessary information provided?"#;

/// Configuration for the Feasibility Validator Agent.
#[derive(Debug, Clone)]
pub struct FeasibilityValidatorConfig {
    /// Minimum score threshold for passing validation.
    pub pass_threshold: f64,
    /// Require task to be marked solvable.
    pub require_solvable: bool,
    /// Require task to be marked non-trivial.
    pub require_non_trivial: bool,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for FeasibilityValidatorConfig {
    fn default() -> Self {
        Self {
            pass_threshold: 0.7,
            require_solvable: true,
            require_non_trivial: true,
            temperature: 0.3,
            max_tokens: 1200,
        }
    }
}

impl FeasibilityValidatorConfig {
    /// Creates a new configuration with custom threshold.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.pass_threshold = threshold;
        self
    }

    /// Sets whether solvability is required for passing.
    pub fn require_solvable(mut self, require: bool) -> Self {
        self.require_solvable = require;
        self
    }

    /// Sets whether non-triviality is required for passing.
    pub fn require_non_trivial(mut self, require: bool) -> Self {
        self.require_non_trivial = require;
        self
    }
}

/// Feasibility Validator Agent that uses LLM to assess task feasibility.
pub struct FeasibilityValidatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: FeasibilityValidatorConfig,
}

impl std::fmt::Debug for FeasibilityValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeasibilityValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl FeasibilityValidatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "feasibility_validator";

    /// Creates a new feasibility validator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: FeasibilityValidatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new feasibility validator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, FeasibilityValidatorConfig::default())
    }

    /// Validates the feasibility of a generated task.
    ///
    /// # Arguments
    ///
    /// * `task` - The generated task to validate
    ///
    /// # Returns
    ///
    /// A `ValidationResult` containing the score and reasoning.
    pub async fn validate_feasibility(
        &self,
        task: &GeneratedTask,
    ) -> AgentResult<ValidationResult> {
        let prompt = self.build_prompt(task);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(FEASIBILITY_VALIDATION_SYSTEM_PROMPT),
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

    /// Builds the user prompt for feasibility validation.
    fn build_prompt(&self, task: &GeneratedTask) -> String {
        let difficulty_str = match task.difficulty {
            crate::difficulty::DifficultyLevel::Easy => "Easy",
            crate::difficulty::DifficultyLevel::Medium => "Medium",
            crate::difficulty::DifficultyLevel::Hard => "Hard",
        };

        let parameters_str = task
            .parameters
            .iter()
            .map(|(k, v)| format!("  {}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        let parameters_display = if parameters_str.is_empty() {
            "  (no parameters)".to_string()
        } else {
            parameters_str
        };

        FEASIBILITY_VALIDATION_USER_TEMPLATE
            .replace("{task_id}", &task.task_id)
            .replace("{difficulty}", difficulty_str)
            .replace("{category}", &task.category)
            .replace("{subcategory}", &task.subcategory)
            .replace("{template_id}", &task.template_id)
            .replace("{instruction}", &task.instruction)
            .replace("{parameters}", &parameters_display)
    }

    /// Parses the LLM response into a ValidationResult.
    fn parse_response(&self, content: &str) -> AgentResult<ValidationResult> {
        let json_content = self.extract_json(content)?;

        let parsed: FeasibilityValidationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        // Build comprehensive reasoning
        let reasoning = self.build_reasoning(&parsed);

        // Collect all issues
        let mut issues = parsed.issues.unwrap_or_default();
        if !parsed.is_solvable {
            issues.push("Task may not be solvable with standard tools".to_string());
        }
        if !parsed.is_non_trivial {
            issues.push("Task may be too trivial".to_string());
        }
        if !parsed.is_clear {
            issues.push("Task instructions may be unclear".to_string());
        }

        let details = if issues.is_empty() {
            None
        } else {
            Some(issues.join("; "))
        };

        // Determine if validation passes
        let passes_threshold = parsed.score >= self.config.pass_threshold;
        let meets_solvability = !self.config.require_solvable || parsed.is_solvable;
        let meets_non_triviality = !self.config.require_non_trivial || parsed.is_non_trivial;

        let passed = passes_threshold && meets_solvability && meets_non_triviality;

        if passed {
            Ok(ValidationResult::Success {
                message: reasoning,
                details,
                score: Some(parsed.score),
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            })
        } else {
            Ok(ValidationResult::Failure {
                message: reasoning,
                details,
                agent_name: Self::AGENT_NAME.to_string(),
                timestamp: chrono::Utc::now(),
            })
        }
    }

    /// Builds a comprehensive reasoning string from the response.
    fn build_reasoning(&self, response: &FeasibilityValidationResponse) -> String {
        let mut parts = vec![response.reasoning.clone()];

        if let Some(ref notes) = response.solvability_notes {
            if !notes.is_empty() {
                parts.push(format!("Solvability: {}", notes));
            }
        }

        if let Some(ref notes) = response.complexity_notes {
            if !notes.is_empty() {
                parts.push(format!("Complexity: {}", notes));
            }
        }

        parts.join(" | ")
    }

    /// Extracts JSON from the response, handling potential markdown code blocks.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        let trimmed = content.trim();

        // If it already starts with '{', use as-is
        if trimmed.starts_with('{') {
            return Ok(trimmed.to_string());
        }

        // Try to extract from markdown code block
        if let Some(start) = trimmed.find("```json") {
            let json_start = start + 7;
            if let Some(end) = trimmed[json_start..].find("```") {
                return Ok(trimmed[json_start..json_start + end].trim().to_string());
            }
        }

        // Try to extract from generic code block
        if let Some(start) = trimmed.find("```") {
            let content_start = start + 3;
            let line_end = trimmed[content_start..]
                .find('\n')
                .map(|i| content_start + i + 1)
                .unwrap_or(content_start);
            if let Some(end) = trimmed[line_end..].find("```") {
                return Ok(trimmed[line_end..line_end + end].trim().to_string());
            }
        }

        // Try to find JSON object anywhere in the content
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                if end > start {
                    return Ok(trimmed[start..=end].to_string());
                }
            }
        }

        Err(AgentError::ResponseParseError(
            "Could not extract JSON from response".to_string(),
        ))
    }
}

/// Response structure from LLM feasibility validation.
#[derive(Debug, serde::Deserialize)]
struct FeasibilityValidationResponse {
    score: f64,
    is_solvable: bool,
    is_non_trivial: bool,
    #[serde(default = "default_true")]
    is_clear: bool,
    reasoning: String,
    #[serde(default)]
    solvability_notes: Option<String>,
    #[serde(default)]
    complexity_notes: Option<String>,
    #[serde(default)]
    issues: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::difficulty::DifficultyLevel;
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
    async fn test_feasibility_validation_pass() {
        let mock_response = r#"{
            "score": 0.9,
            "is_solvable": true,
            "is_non_trivial": true,
            "is_clear": true,
            "reasoning": "This task is well-designed and appropriately challenging.",
            "solvability_notes": "Can be solved using grep, awk, and basic file operations.",
            "complexity_notes": "Requires multiple steps and some analysis.",
            "issues": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = FeasibilityValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Medium,
            "Find and analyze error patterns in the log files.",
        );

        let result = agent
            .validate_feasibility(&task)
            .await
            .expect("validation should succeed");

        assert!(result.is_success());
        assert_eq!(result.score(), Some(0.9));
        assert!(result.details().is_none());
    }

    #[tokio::test]
    async fn test_feasibility_validation_fail_not_solvable() {
        let mock_response = r#"{
            "score": 0.3,
            "is_solvable": false,
            "is_non_trivial": true,
            "is_clear": true,
            "reasoning": "Task requires proprietary software not available in Linux.",
            "issues": ["Requires Microsoft SQL Server which is not standard Linux"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = FeasibilityValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Hard,
            "Query the SQL Server database for recent transactions.",
        );

        let result = agent
            .validate_feasibility(&task)
            .await
            .expect("validation should succeed");

        assert!(!result.is_success());
        assert!(result.details().expect("has details").contains("solvable"));
    }

    #[tokio::test]
    async fn test_feasibility_validation_fail_too_trivial() {
        let mock_response = r#"{
            "score": 0.5,
            "is_solvable": true,
            "is_non_trivial": false,
            "is_clear": true,
            "reasoning": "This task is too simple - a single command solves it.",
            "complexity_notes": "Just requires running 'ls -la'",
            "issues": ["Can be solved with one obvious command"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = FeasibilityValidatorAgent::with_defaults(mock_provider);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Easy,
            "List all files in the current directory with details.",
        );

        let result = agent
            .validate_feasibility(&task)
            .await
            .expect("validation should succeed");

        assert!(!result.is_success());
        assert!(result.details().expect("has details").contains("trivial"));
    }

    #[tokio::test]
    async fn test_feasibility_without_non_trivial_requirement() {
        let mock_response = r#"{
            "score": 0.8,
            "is_solvable": true,
            "is_non_trivial": false,
            "is_clear": true,
            "reasoning": "Simple but valid task.",
            "issues": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let config = FeasibilityValidatorConfig::default().require_non_trivial(false);
        let agent = FeasibilityValidatorAgent::new(mock_provider, config);

        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Easy,
            "Echo hello world.",
        );

        let result = agent
            .validate_feasibility(&task)
            .await
            .expect("validation should succeed");

        // Should pass because non-triviality is not required
        assert!(result.is_success());
    }

    #[test]
    fn test_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = FeasibilityValidatorAgent::with_defaults(mock_provider);

        let mut task = GeneratedTask::minimal(
            "test-medium-123",
            "log-analysis-001",
            DifficultyLevel::Medium,
            "Analyze the log file.",
        );
        task.parameters.insert(
            "log_path".to_string(),
            serde_json::json!("/var/log/app.log"),
        );

        let prompt = agent.build_prompt(&task);

        assert!(prompt.contains("test-medium-123"));
        assert!(prompt.contains("Medium"));
        assert!(prompt.contains("log-analysis-001"));
        assert!(prompt.contains("Analyze the log file."));
        assert!(prompt.contains("log_path"));
    }

    #[test]
    fn test_build_reasoning() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = FeasibilityValidatorAgent::with_defaults(mock_provider);

        let response = FeasibilityValidationResponse {
            score: 0.85,
            is_solvable: true,
            is_non_trivial: true,
            is_clear: true,
            reasoning: "Good task".to_string(),
            solvability_notes: Some("Can use grep and awk".to_string()),
            complexity_notes: Some("Requires 5 steps".to_string()),
            issues: None,
        };

        let reasoning = agent.build_reasoning(&response);

        assert!(reasoning.contains("Good task"));
        assert!(reasoning.contains("grep and awk"));
        assert!(reasoning.contains("5 steps"));
    }
}
