//! Task Validator Agent for the multi-agent validation system.
//!
//! This agent validates that generated tasks are:
//! - Solvable but non-trivial (requires actual work, not memorization)
//! - Complex enough to push LLM limits (estimated >5 minutes thinking time)
//! - Cannot be solved by pattern matching or simple lookup
//! - Requires genuine problem-solving skills
//! - Rejects tasks that are too easy or have commonly known answers

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::error::{AgentError, AgentResult};
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for task validation.
const TASK_VALIDATION_SYSTEM_PROMPT: &str = r#"You are a terminal benchmark task validator. Your job is to assess if a task is PRACTICAL, EXECUTABLE, and SUFFICIENTLY CHALLENGING for testing AI coding agents.

Evaluate the following task and determine:
1. COMPLEXITY: How many distinct investigation/action steps needed? (score 0.0-1.0)
   - 1-3 steps → 0.2-0.4 (easy, reject for benchmark)
   - 4-7 steps → 0.5-0.6 (medium)
   - 8-15 steps → 0.7-0.85 (hard, ideal for benchmark)
   - 15+ steps → 0.85-1.0 (very hard)
2. MEMORIZATION RISK: Is this solvable by recalling common patterns? (score 0.0-1.0)
3. THINKING TIME: How long for a skilled developer to solve? (minutes)
4. GENUINE REASONING: Does this require investigation before the solution is clear? (true/false)

HARD TASK INDICATORS (should have MOST of these for complexity > 0.7):
- Symptoms don't directly reveal the root cause
- Multiple files/components need investigation
- Edge cases that break naive solutions
- Requires understanding data/state before acting
- Red herrings or misleading error messages

APPROVAL CRITERIA (APPROVE if task):
- References specific file paths in the task description
- Requires 5+ distinct steps to solve
- Has a clear, verifiable output file
- Can be executed in a Docker container
- Requires investigation/debugging (not just execution)

REJECTION CRITERIA (reject if ANY apply):
- Requires external cloud services (AWS, Azure, GCP)
- Requires real network infrastructure
- Has no concrete file paths or outputs
- Is purely theoretical without executable components
- Can be solved with a single command or trivial pipeline
- Root cause is immediately obvious from the description

You MUST respond with ONLY a valid JSON object in this exact format:
{
  "complexity_score": <float between 0.0 and 1.0>,
  "memorization_risk": <float between 0.0 and 1.0>,
  "estimated_thinking_time_minutes": <integer>,
  "requires_genuine_reasoning": <true or false>,
  "rejection_reasons": ["<reason1>", "<reason2>"] or [],
  "improvement_suggestions": ["<suggestion1>", "<suggestion2>"],
  "reasoning": "<detailed explanation of your assessment>"
}

CRITICAL: Your entire response must be ONLY the JSON object."#;

/// User prompt template for task validation.
const TASK_VALIDATION_USER_TEMPLATE: &str = r#"Task to validate:
Title: {title}
Description: {description}
Category: {category}
Required Skills: {skills}

Analyze this task for complexity, memorization risk, and genuine reasoning requirements."#;

/// Represents a task idea to be validated.
///
/// This struct contains the essential information about a task that needs
/// validation before being used in benchmarks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdea {
    /// Title or name of the task.
    pub title: String,
    /// Detailed description of what the task requires.
    pub description: String,
    /// Category of the task (e.g., "debugging", "file_manipulation").
    pub category: String,
    /// List of skills required to complete the task.
    pub required_skills: Vec<String>,
}

impl TaskIdea {
    /// Creates a new task idea.
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        category: impl Into<String>,
        required_skills: Vec<String>,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            category: category.into(),
            required_skills,
        }
    }

    /// Creates a task idea with minimal required fields for testing.
    pub fn minimal(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            category: String::new(),
            required_skills: Vec::new(),
        }
    }
}

/// Assessment result from task validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationAssessment {
    /// Whether the task passes all validation criteria.
    pub is_valid: bool,
    /// Complexity score from 0.0 to 1.0 (higher = more complex).
    pub complexity_score: f64,
    /// Risk that the task can be solved from memorization (0.0 to 1.0).
    pub memorization_risk: f64,
    /// Estimated time in minutes for an expert to solve the task.
    pub estimated_thinking_time_minutes: u32,
    /// Whether the task requires genuine reasoning vs simple recall.
    pub requires_genuine_reasoning: bool,
    /// Reasons why the task was rejected (empty if valid).
    pub rejection_reasons: Vec<String>,
    /// Suggestions for improving the task.
    pub improvement_suggestions: Vec<String>,
    /// LLM's detailed reasoning about the assessment.
    pub reasoning: String,
}

impl ValidationAssessment {
    /// Creates a validation assessment that passed all checks.
    pub fn valid(
        complexity_score: f64,
        memorization_risk: f64,
        estimated_thinking_time_minutes: u32,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            is_valid: true,
            complexity_score,
            memorization_risk,
            estimated_thinking_time_minutes,
            requires_genuine_reasoning: true,
            rejection_reasons: Vec::new(),
            improvement_suggestions: Vec::new(),
            reasoning: reasoning.into(),
        }
    }

    /// Creates a validation assessment that failed checks.
    pub fn invalid(
        complexity_score: f64,
        memorization_risk: f64,
        estimated_thinking_time_minutes: u32,
        rejection_reasons: Vec<String>,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            is_valid: false,
            complexity_score,
            memorization_risk,
            estimated_thinking_time_minutes,
            requires_genuine_reasoning: false,
            rejection_reasons,
            improvement_suggestions: Vec::new(),
            reasoning: reasoning.into(),
        }
    }
}

/// Configuration for the Task Validator Agent.
#[derive(Debug, Clone)]
pub struct TaskValidatorConfig {
    /// Minimum complexity score to pass validation (0.0 to 1.0).
    pub min_complexity_score: f64,
    /// Minimum thinking time in minutes required.
    pub min_thinking_time_minutes: u32,
    /// Maximum memorization risk allowed (reject if above this).
    pub rejection_threshold: f64,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for TaskValidatorConfig {
    fn default() -> Self {
        Self {
            min_complexity_score: 0.55, // Increased to ensure tasks require real investigation
            min_thinking_time_minutes: 5, // Tasks should require at least 5 mins of thinking
            rejection_threshold: 0.4,   // Stricter: reject if memorization risk > 40%
            temperature: 0.3,
            max_tokens: 3000,
        }
    }
}

impl TaskValidatorConfig {
    /// Creates a new configuration with the minimum complexity score.
    pub fn with_min_complexity_score(mut self, score: f64) -> Self {
        self.min_complexity_score = score.clamp(0.0, 1.0);
        self
    }

    /// Sets the minimum thinking time in minutes.
    pub fn with_min_thinking_time_minutes(mut self, minutes: u32) -> Self {
        self.min_thinking_time_minutes = minutes;
        self
    }

    /// Sets the rejection threshold for memorization risk.
    pub fn with_rejection_threshold(mut self, threshold: f64) -> Self {
        self.rejection_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets the temperature for LLM generation.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the maximum tokens for LLM response.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Task Validator Agent that uses LLM to assess task quality.
///
/// This agent validates that generated tasks meet quality criteria:
/// - Sufficient complexity requiring multiple reasoning steps
/// - Low memorization risk (cannot be solved from training data)
/// - Requires genuine problem-solving, not pattern matching
/// - Estimated thinking time meets minimum threshold
pub struct TaskValidatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: TaskValidatorConfig,
}

impl std::fmt::Debug for TaskValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl TaskValidatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "task_validator";

    /// Creates a new task validator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: TaskValidatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new task validator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, TaskValidatorConfig::default())
    }

    /// Validates whether a task idea meets quality criteria.
    ///
    /// # Arguments
    ///
    /// * `task_idea` - The task idea to validate
    ///
    /// # Returns
    ///
    /// A `ValidationAssessment` containing the validation results.
    ///
    /// # Retry Logic
    ///
    /// This method will retry up to 3 times on parse failures to handle
    /// truncated or malformed JSON responses from the LLM.
    pub async fn validate_task(&self, task_idea: &TaskIdea) -> AgentResult<ValidationAssessment> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.attempt_validate_task(task_idea).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        task_title = %task_idea.title,
                        "Task validation failed, retrying..."
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.expect("should have an error after 3 failed attempts"))
    }

    /// Attempts a single validation of a task idea.
    async fn attempt_validate_task(
        &self,
        task_idea: &TaskIdea,
    ) -> AgentResult<ValidationAssessment> {
        let prompt = self.format_validation_prompt(task_idea);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(TASK_VALIDATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        let mut assessment = self.parse_response(content)?;

        // Apply threshold checks to determine validity
        assessment.is_valid = self.check_thresholds(&assessment);

        Ok(assessment)
    }

    /// Validates multiple task ideas in batch.
    ///
    /// # Arguments
    ///
    /// * `ideas` - Slice of task ideas to validate
    ///
    /// # Returns
    ///
    /// A vector of tuples containing each task idea and its assessment.
    pub async fn batch_validate(
        &self,
        ideas: &[TaskIdea],
    ) -> AgentResult<Vec<(TaskIdea, ValidationAssessment)>> {
        let mut results = Vec::with_capacity(ideas.len());

        for idea in ideas {
            let assessment = self.validate_task(idea).await?;
            results.push((idea.clone(), assessment));
        }

        Ok(results)
    }

    /// Checks if the assessment meets all configured thresholds.
    fn check_thresholds(&self, assessment: &ValidationAssessment) -> bool {
        // Must meet minimum complexity score
        if assessment.complexity_score < self.config.min_complexity_score {
            return false;
        }

        // Must meet minimum thinking time
        if assessment.estimated_thinking_time_minutes < self.config.min_thinking_time_minutes {
            return false;
        }

        // Must not exceed memorization risk threshold
        if assessment.memorization_risk > self.config.rejection_threshold {
            return false;
        }

        // Must require genuine reasoning
        if !assessment.requires_genuine_reasoning {
            return false;
        }

        // Must not have any rejection reasons
        if !assessment.rejection_reasons.is_empty() {
            return false;
        }

        true
    }

    /// Formats the validation prompt with task details.
    fn format_validation_prompt(&self, task: &TaskIdea) -> String {
        let skills_str = if task.required_skills.is_empty() {
            "(not specified)".to_string()
        } else {
            task.required_skills.join(", ")
        };

        let category = if task.category.is_empty() {
            "(not specified)"
        } else {
            &task.category
        };

        TASK_VALIDATION_USER_TEMPLATE
            .replace("{title}", &task.title)
            .replace("{description}", &task.description)
            .replace("{category}", category)
            .replace("{skills}", &skills_str)
    }

    /// Parses the LLM response into a ValidationAssessment.
    fn parse_response(&self, content: &str) -> AgentResult<ValidationAssessment> {
        let json_content = self.extract_json(content)?;

        let parsed: TaskValidationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        Ok(ValidationAssessment {
            is_valid: false, // Will be set by check_thresholds
            complexity_score: parsed.complexity_score.clamp(0.0, 1.0),
            memorization_risk: parsed.memorization_risk.clamp(0.0, 1.0),
            estimated_thinking_time_minutes: parsed.estimated_thinking_time_minutes,
            requires_genuine_reasoning: parsed.requires_genuine_reasoning,
            rejection_reasons: parsed.rejection_reasons,
            improvement_suggestions: parsed.improvement_suggestions,
            reasoning: parsed.reasoning,
        })
    }

    /// Extracts JSON from the response, handling potential markdown code blocks and mixed content.
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

/// Response structure from LLM task validation.
#[derive(Debug, Deserialize)]
struct TaskValidationResponse {
    complexity_score: f64,
    memorization_risk: f64,
    estimated_thinking_time_minutes: u32,
    requires_genuine_reasoning: bool,
    #[serde(default)]
    rejection_reasons: Vec<String>,
    #[serde(default)]
    improvement_suggestions: Vec<String>,
    reasoning: String,
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
    async fn test_task_validation_pass() {
        let mock_response = r#"{
            "complexity_score": 0.85,
            "memorization_risk": 0.2,
            "estimated_thinking_time_minutes": 15,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": ["Consider adding more edge cases"],
            "reasoning": "This task requires multi-step reasoning and cannot be solved through simple memorization."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::new(
            "Debug Memory Leak",
            "Identify and fix a memory leak in a long-running service by analyzing heap dumps and tracking object allocations.",
            "debugging",
            vec!["memory-profiling".to_string(), "heap-analysis".to_string()],
        );

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(result.is_valid);
        assert!((result.complexity_score - 0.85).abs() < 0.01);
        assert!((result.memorization_risk - 0.2).abs() < 0.01);
        assert_eq!(result.estimated_thinking_time_minutes, 15);
        assert!(result.requires_genuine_reasoning);
        assert!(result.rejection_reasons.is_empty());
    }

    #[tokio::test]
    async fn test_task_validation_fail_low_complexity() {
        let mock_response = r#"{
            "complexity_score": 0.3,
            "memorization_risk": 0.2,
            "estimated_thinking_time_minutes": 10,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": ["Add more complexity"],
            "reasoning": "Task is straightforward."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::minimal("Simple Task", "A very simple task.");

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(!result.is_valid, "Task should fail due to low complexity");
        assert!((result.complexity_score - 0.3).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_task_validation_fail_high_memorization_risk() {
        let mock_response = r#"{
            "complexity_score": 0.8,
            "memorization_risk": 0.7,
            "estimated_thinking_time_minutes": 10,
            "requires_genuine_reasoning": false,
            "rejection_reasons": ["This is a common interview question with well-known solution"],
            "improvement_suggestions": ["Use a unique problem variant"],
            "reasoning": "This task can be solved from memorized patterns."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::new(
            "Implement Quicksort",
            "Implement the quicksort algorithm to sort an array of integers.",
            "algorithms",
            vec!["sorting".to_string()],
        );

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(
            !result.is_valid,
            "Task should fail due to high memorization risk"
        );
        assert!((result.memorization_risk - 0.7).abs() < 0.01);
        assert!(!result.rejection_reasons.is_empty());
    }

    #[tokio::test]
    async fn test_task_validation_fail_insufficient_thinking_time() {
        let mock_response = r#"{
            "complexity_score": 0.7,
            "memorization_risk": 0.2,
            "estimated_thinking_time_minutes": 2,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": [],
            "reasoning": "Task can be completed quickly."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::minimal("Quick Task", "A task that doesn't take long.");

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(
            !result.is_valid,
            "Task should fail due to insufficient thinking time"
        );
        assert_eq!(result.estimated_thinking_time_minutes, 2);
    }

    #[tokio::test]
    async fn test_task_validation_fail_no_genuine_reasoning() {
        let mock_response = r#"{
            "complexity_score": 0.7,
            "memorization_risk": 0.3,
            "estimated_thinking_time_minutes": 10,
            "requires_genuine_reasoning": false,
            "rejection_reasons": ["Can be solved with a single command"],
            "improvement_suggestions": [],
            "reasoning": "This is a simple lookup task."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::minimal(
            "Lookup Task",
            "Find information that can be easily looked up.",
        );

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(
            !result.is_valid,
            "Task should fail because it doesn't require genuine reasoning"
        );
        assert!(!result.requires_genuine_reasoning);
    }

    #[tokio::test]
    async fn test_json_extraction_from_code_block() {
        let mock_response = r#"Here's my analysis:

```json
{
    "complexity_score": 0.9,
    "memorization_risk": 0.1,
    "estimated_thinking_time_minutes": 20,
    "requires_genuine_reasoning": true,
    "rejection_reasons": [],
    "improvement_suggestions": [],
    "reasoning": "Excellent task."
}
```

The task is appropriate."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::minimal("Complex Task", "A genuinely complex task.");

        let result = agent
            .validate_task(&task)
            .await
            .expect("validation should succeed");

        assert!(result.is_valid);
        assert!((result.complexity_score - 0.9).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_batch_validate() {
        let mock_response = r#"{
            "complexity_score": 0.8,
            "memorization_risk": 0.2,
            "estimated_thinking_time_minutes": 10,
            "requires_genuine_reasoning": true,
            "rejection_reasons": [],
            "improvement_suggestions": [],
            "reasoning": "Good task."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let tasks = vec![
            TaskIdea::minimal("Task 1", "First task"),
            TaskIdea::minimal("Task 2", "Second task"),
            TaskIdea::minimal("Task 3", "Third task"),
        ];

        let results = agent
            .batch_validate(&tasks)
            .await
            .expect("batch validation should succeed");

        assert_eq!(results.len(), 3);
        for (task, assessment) in &results {
            assert!(assessment.is_valid);
            assert!(!task.title.is_empty());
        }
    }

    #[test]
    fn test_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::new(
            "Debug Race Condition",
            "Find and fix a race condition in concurrent code.",
            "debugging",
            vec!["concurrency".to_string(), "threading".to_string()],
        );

        let prompt = agent.format_validation_prompt(&task);

        assert!(prompt.contains("Debug Race Condition"));
        assert!(prompt.contains("race condition"));
        assert!(prompt.contains("debugging"));
        assert!(prompt.contains("concurrency, threading"));
    }

    #[test]
    fn test_prompt_building_empty_fields() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let task = TaskIdea::minimal("Title Only", "Description only");

        let prompt = agent.format_validation_prompt(&task);

        assert!(prompt.contains("Title Only"));
        assert!(prompt.contains("Description only"));
        assert!(prompt.contains("(not specified)"));
    }

    #[test]
    fn test_check_thresholds_all_pass() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let assessment = ValidationAssessment {
            is_valid: false,
            complexity_score: 0.8,
            memorization_risk: 0.2,
            estimated_thinking_time_minutes: 10,
            requires_genuine_reasoning: true,
            rejection_reasons: Vec::new(),
            improvement_suggestions: Vec::new(),
            reasoning: "Good task".to_string(),
        };

        assert!(agent.check_thresholds(&assessment));
    }

    #[test]
    fn test_check_thresholds_low_complexity() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let assessment = ValidationAssessment {
            is_valid: false,
            complexity_score: 0.3, // Below default 0.4 (lowered for practical terminal tasks)
            memorization_risk: 0.2,
            estimated_thinking_time_minutes: 10,
            requires_genuine_reasoning: true,
            rejection_reasons: Vec::new(),
            improvement_suggestions: Vec::new(),
            reasoning: "Low complexity".to_string(),
        };

        assert!(!agent.check_thresholds(&assessment));
    }

    #[test]
    fn test_check_thresholds_high_memorization_risk() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let assessment = ValidationAssessment {
            is_valid: false,
            complexity_score: 0.8,
            memorization_risk: 0.6, // Above default 0.4
            estimated_thinking_time_minutes: 10,
            requires_genuine_reasoning: true,
            rejection_reasons: Vec::new(),
            improvement_suggestions: Vec::new(),
            reasoning: "High memorization risk".to_string(),
        };

        assert!(!agent.check_thresholds(&assessment));
    }

    #[test]
    fn test_check_thresholds_with_rejection_reasons() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let assessment = ValidationAssessment {
            is_valid: false,
            complexity_score: 0.8,
            memorization_risk: 0.2,
            estimated_thinking_time_minutes: 10,
            requires_genuine_reasoning: true,
            rejection_reasons: vec!["Common interview question".to_string()],
            improvement_suggestions: Vec::new(),
            reasoning: "Has rejection reasons".to_string(),
        };

        assert!(!agent.check_thresholds(&assessment));
    }

    #[test]
    fn test_config_builder_pattern() {
        let config = TaskValidatorConfig::default()
            .with_min_complexity_score(0.7)
            .with_min_thinking_time_minutes(10)
            .with_rejection_threshold(0.3)
            .with_temperature(0.5)
            .with_max_tokens(2000);

        assert!((config.min_complexity_score - 0.7).abs() < 0.01);
        assert_eq!(config.min_thinking_time_minutes, 10);
        assert!((config.rejection_threshold - 0.3).abs() < 0.01);
        assert!((config.temperature - 0.5).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
    }

    #[test]
    fn test_config_clamping() {
        let config = TaskValidatorConfig::default()
            .with_min_complexity_score(1.5) // Should clamp to 1.0
            .with_rejection_threshold(-0.5) // Should clamp to 0.0
            .with_temperature(3.0); // Should clamp to 2.0

        assert!((config.min_complexity_score - 1.0).abs() < 0.01);
        assert!((config.rejection_threshold - 0.0).abs() < 0.01);
        assert!((config.temperature - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_task_idea_creation() {
        let task = TaskIdea::new(
            "Test Task",
            "Test Description",
            "testing",
            vec!["skill1".to_string(), "skill2".to_string()],
        );

        assert_eq!(task.title, "Test Task");
        assert_eq!(task.description, "Test Description");
        assert_eq!(task.category, "testing");
        assert_eq!(task.required_skills.len(), 2);
    }

    #[test]
    fn test_task_idea_minimal() {
        let task = TaskIdea::minimal("Minimal Task", "Minimal Description");

        assert_eq!(task.title, "Minimal Task");
        assert_eq!(task.description, "Minimal Description");
        assert!(task.category.is_empty());
        assert!(task.required_skills.is_empty());
    }

    #[test]
    fn test_validation_assessment_valid() {
        let assessment = ValidationAssessment::valid(0.8, 0.2, 15, "Task is valid");

        assert!(assessment.is_valid);
        assert!((assessment.complexity_score - 0.8).abs() < 0.01);
        assert!((assessment.memorization_risk - 0.2).abs() < 0.01);
        assert_eq!(assessment.estimated_thinking_time_minutes, 15);
        assert!(assessment.requires_genuine_reasoning);
        assert!(assessment.rejection_reasons.is_empty());
    }

    #[test]
    fn test_validation_assessment_invalid() {
        let assessment = ValidationAssessment::invalid(
            0.3,
            0.8,
            2,
            vec!["Too simple".to_string()],
            "Task is invalid",
        );

        assert!(!assessment.is_valid);
        assert!((assessment.complexity_score - 0.3).abs() < 0.01);
        assert!((assessment.memorization_risk - 0.8).abs() < 0.01);
        assert_eq!(assessment.estimated_thinking_time_minutes, 2);
        assert!(!assessment.requires_genuine_reasoning);
        assert!(!assessment.rejection_reasons.is_empty());
    }

    #[test]
    fn test_extract_json_direct() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let content = r#"{"complexity_score": 0.8, "reasoning": "test"}"#;
        let result = agent.extract_json(content);
        assert!(result.is_ok());
        assert!(result.expect("json extracted").contains("complexity_score"));
    }

    #[test]
    fn test_extract_json_with_surrounding_text() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let content = r#"Here is the result: {"complexity_score": 0.8} end of response"#;
        let result = agent.extract_json(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_json_no_json() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = TaskValidatorAgent::with_defaults(mock_provider);

        let content = "This response contains no JSON at all.";
        let result = agent.extract_json(content);
        assert!(result.is_err());
    }
}
