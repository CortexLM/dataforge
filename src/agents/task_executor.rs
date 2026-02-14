//! Task Executor Agent for creating complete synthetic benchmark tasks.
//!
//! This agent is PRIVILEGED - it knows the solution approach/methodology and creates
//! complete task specifications including:
//! - Problem statement (what other LLMs will see)
//! - Hidden solution approach (kept secret)
//! - Verification tests (to validate solutions)
//! - Difficulty scoring criteria
//!
//! The solution approach must NOT be inferable from the problem statement alone.
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::agents::task_executor::{TaskExecutorAgent, TaskExecutorConfig};
//! use swe_forge::agents::task_validator::{TaskIdea, ValidationAssessment};
//! use swe_forge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! // Create LLM client
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//!
//! // Configure the executor
//! let config = TaskExecutorConfig::default();
//! let executor = TaskExecutorAgent::new(llm_client, config);
//!
//! // Create a task from an idea and validation assessment
//! let idea = TaskIdea::new(
//!     "Find Error",
//!     "Find a specific error in logs",
//!     "debugging",
//!     vec!["grep".to_string()],
//! );
//! let assessment = ValidationAssessment::valid(0.65, 0.2, 5, "Good task");
//!
//! let task = executor.create_task(&idea, &assessment).await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::anti_hardcoding::CanaryConfig;
use crate::difficulty::DifficultyLevel;
use crate::llm::{GenerationRequest, LlmProvider, Message};
use crate::utils::json_extraction::{try_extract_json_from_response, JsonExtractionError};

use super::error::{AgentError, AgentResult};
use super::task_validator::{TaskIdea, ValidationAssessment};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Task Executor Agent.
#[derive(Debug, Clone)]
pub struct TaskExecutorConfig {
    /// Temperature for LLM generation (controls creativity). Default: 0.5.
    pub temperature: f64,
    /// Maximum tokens for LLM response. Default: 4000.
    pub max_tokens: u32,
    /// Whether to include canary tokens for contamination detection. Default: true.
    pub include_canary: bool,
    /// Prefix for canary tokens. Default: "DATAFORGE_CANARY_".
    pub canary_prefix: String,
    /// Base seed for deterministic generation.
    pub base_seed: u64,
}

impl Default for TaskExecutorConfig {
    fn default() -> Self {
        Self {
            temperature: 0.5,
            max_tokens: 6000,
            include_canary: true,
            canary_prefix: "DATAFORGE_CANARY_".to_string(),
            base_seed: 42,
        }
    }
}

impl TaskExecutorConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the temperature for LLM generation.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Set the maximum tokens for LLM response.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set whether to include canary tokens.
    pub fn with_include_canary(mut self, include: bool) -> Self {
        self.include_canary = include;
        self
    }

    /// Set the canary token prefix.
    pub fn with_canary_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.canary_prefix = prefix.into();
        self
    }

    /// Set the base seed for deterministic generation.
    pub fn with_base_seed(mut self, seed: u64) -> Self {
        self.base_seed = seed;
        self
    }
}

// ============================================================================
// Task Specification Types
// ============================================================================

/// A complete synthetic task specification.
///
/// This contains everything needed to present a task to test-takers,
/// validate their solutions, and score their performance - while keeping
/// the solution approach hidden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticTask {
    /// Unique identifier for this task.
    pub id: String,
    /// Version of the task specification format.
    pub version: String,
    /// Problem statement visible to test-takers (NO solution hints).
    pub problem_statement: String,
    /// Hidden solution methodology (kept secret from test-takers).
    pub hidden_solution: HiddenSolution,
    /// Specification for how to validate solutions.
    pub verification: VerificationSpec,
    /// Difficulty scoring information.
    pub difficulty: DifficultyScoring,
    /// Task metadata (category, tags, etc.).
    pub metadata: TaskMetadata,
    /// Anti-memorization configuration.
    pub anti_memorization: AntiMemorizationConfig,
    /// When this task was created.
    pub created_at: DateTime<Utc>,
}

impl SyntheticTask {
    /// Create a new synthetic task with required fields.
    pub fn new(
        problem_statement: impl Into<String>,
        hidden_solution: HiddenSolution,
        verification: VerificationSpec,
        difficulty: DifficultyScoring,
        metadata: TaskMetadata,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            version: "1.0.0".to_string(),
            problem_statement: problem_statement.into(),
            hidden_solution,
            verification,
            difficulty,
            metadata,
            anti_memorization: AntiMemorizationConfig::default(),
            created_at: Utc::now(),
        }
    }

    /// Set custom anti-memorization configuration.
    pub fn with_anti_memorization(mut self, config: AntiMemorizationConfig) -> Self {
        self.anti_memorization = config;
        self
    }

    /// Check if this task has canary protection enabled.
    pub fn has_canary(&self) -> bool {
        !self.anti_memorization.canary_token.is_empty()
    }
}

/// Hidden solution information kept secret from test-takers.
///
/// This contains the methodology and approach that test-takers must
/// discover through their own reasoning and work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiddenSolution {
    /// High-level solution methodology/approach.
    pub approach: String,
    /// Critical insights or realizations needed to solve the task.
    pub key_insights: Vec<String>,
    /// Example commands or code that form the solution.
    pub reference_commands: Vec<String>,
    /// Expected time to complete in seconds.
    pub expected_time_seconds: u32,
    /// Number of distinct steps in the solution.
    pub step_count: u32,
}

impl HiddenSolution {
    /// Create a new hidden solution.
    pub fn new(approach: impl Into<String>) -> Self {
        Self {
            approach: approach.into(),
            key_insights: Vec::new(),
            reference_commands: Vec::new(),
            expected_time_seconds: 300,
            step_count: 1,
        }
    }

    /// Add key insights.
    pub fn with_key_insights(
        mut self,
        insights: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.key_insights = insights.into_iter().map(|i| i.into()).collect();
        self
    }

    /// Add reference commands.
    pub fn with_reference_commands(
        mut self,
        commands: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.reference_commands = commands.into_iter().map(|c| c.into()).collect();
        self
    }

    /// Set expected completion time.
    pub fn with_expected_time_seconds(mut self, seconds: u32) -> Self {
        self.expected_time_seconds = seconds;
        self
    }

    /// Set step count.
    pub fn with_step_count(mut self, count: u32) -> Self {
        self.step_count = count;
        self
    }
}

/// Specification for how to verify/validate solutions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSpec {
    /// Criteria that define a successful solution.
    pub success_criteria: Vec<String>,
    /// Criteria for awarding partial credit.
    pub partial_credit_criteria: Vec<PartialCreditItem>,
    /// Automated checks to run against the solution.
    pub automated_checks: Vec<AutomatedCheck>,
    /// Whether manual human review is required.
    pub manual_review_required: bool,
}

impl VerificationSpec {
    /// Create a new verification specification.
    pub fn new() -> Self {
        Self {
            success_criteria: Vec::new(),
            partial_credit_criteria: Vec::new(),
            automated_checks: Vec::new(),
            manual_review_required: false,
        }
    }

    /// Add success criteria.
    pub fn with_success_criteria(
        mut self,
        criteria: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.success_criteria = criteria.into_iter().map(|c| c.into()).collect();
        self
    }

    /// Add partial credit criteria.
    pub fn with_partial_credit(
        mut self,
        items: impl IntoIterator<Item = PartialCreditItem>,
    ) -> Self {
        self.partial_credit_criteria = items.into_iter().collect();
        self
    }

    /// Add automated checks.
    pub fn with_automated_checks(
        mut self,
        checks: impl IntoIterator<Item = AutomatedCheck>,
    ) -> Self {
        self.automated_checks = checks.into_iter().collect();
        self
    }

    /// Set whether manual review is required.
    pub fn with_manual_review(mut self, required: bool) -> Self {
        self.manual_review_required = required;
        self
    }
}

impl Default for VerificationSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// A criterion for awarding partial credit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialCreditItem {
    /// Description of what this criterion checks.
    pub criterion: String,
    /// Points awarded if this criterion is met (0.0 to 1.0).
    pub points: f64,
}

impl PartialCreditItem {
    /// Create a new partial credit item.
    pub fn new(criterion: impl Into<String>, points: f64) -> Self {
        Self {
            criterion: criterion.into(),
            points: points.clamp(0.0, 1.0),
        }
    }
}

/// An automated check to validate solution correctness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatedCheck {
    /// Type of check to perform.
    pub check_type: CheckType,
    /// Target of the check (file path, command, etc.).
    pub target: String,
    /// Expected value or pattern.
    pub expected: String,
}

impl AutomatedCheck {
    /// Create a new automated check.
    pub fn new(
        check_type: CheckType,
        target: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            check_type,
            target: target.into(),
            expected: expected.into(),
        }
    }

    /// Create a file exists check.
    pub fn file_exists(path: impl Into<String>) -> Self {
        Self::new(CheckType::FileExists, path, "true")
    }

    /// Create an output contains check.
    pub fn output_contains(target: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self::new(CheckType::OutputContains, target, pattern)
    }

    /// Create a command succeeds check.
    pub fn command_succeeds(command: impl Into<String>) -> Self {
        Self::new(CheckType::CommandSucceeds, command, "0")
    }

    /// Create an exit code check.
    pub fn exit_code(command: impl Into<String>, expected_code: i32) -> Self {
        Self::new(CheckType::ExitCode, command, expected_code.to_string())
    }
}

/// Types of automated checks that can be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    /// Check if a file exists at the specified path.
    FileExists,
    /// Check if output contains a specific substring.
    OutputContains,
    /// Check if output matches a regex pattern.
    OutputMatches,
    /// Check if a command executes successfully (exit code 0).
    CommandSucceeds,
    /// Check if a command returns a specific exit code.
    ExitCode,
    /// Custom check with user-defined logic.
    Custom,
}

/// Difficulty scoring information for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyScoring {
    /// Overall difficulty level.
    pub level: DifficultyLevel,
    /// Factors contributing to complexity.
    pub complexity_factors: Vec<String>,
    /// Base score awarded for completion.
    pub base_score: f64,
    /// Whether time bonuses are available.
    pub time_bonus_eligible: bool,
}

impl DifficultyScoring {
    /// Create new difficulty scoring with a level.
    pub fn new(level: DifficultyLevel) -> Self {
        let base_score = level.base_points();
        Self {
            level,
            complexity_factors: Vec::new(),
            base_score,
            time_bonus_eligible: true,
        }
    }

    /// Add complexity factors.
    pub fn with_complexity_factors(
        mut self,
        factors: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.complexity_factors = factors.into_iter().map(|f| f.into()).collect();
        self
    }

    /// Set base score.
    pub fn with_base_score(mut self, score: f64) -> Self {
        self.base_score = score;
        self
    }

    /// Set time bonus eligibility.
    pub fn with_time_bonus_eligible(mut self, eligible: bool) -> Self {
        self.time_bonus_eligible = eligible;
        self
    }
}

/// Metadata about a synthetic task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Primary category (e.g., "debugging", "file_manipulation").
    pub category: String,
    /// Subcategory for finer classification.
    pub subcategory: String,
    /// Tags for searchability.
    pub tags: Vec<String>,
    /// ID of the source TaskIdea this was created from.
    pub source_idea_id: String,
}

impl TaskMetadata {
    /// Create new task metadata.
    pub fn new(category: impl Into<String>, source_idea_id: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            subcategory: String::new(),
            tags: Vec::new(),
            source_idea_id: source_idea_id.into(),
        }
    }

    /// Set subcategory.
    pub fn with_subcategory(mut self, subcategory: impl Into<String>) -> Self {
        self.subcategory = subcategory.into();
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(|t| t.into()).collect();
        self
    }
}

/// Configuration for anti-memorization measures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiMemorizationConfig {
    /// Unique canary token embedded in the task.
    pub canary_token: String,
    /// Dynamic values that are randomized per task instance.
    pub dynamic_values: HashMap<String, String>,
    /// Level of obfuscation (0 = none, 3 = maximum).
    pub obfuscation_level: u8,
}

impl Default for AntiMemorizationConfig {
    fn default() -> Self {
        Self {
            canary_token: String::new(),
            dynamic_values: HashMap::new(),
            obfuscation_level: 1,
        }
    }
}

impl AntiMemorizationConfig {
    /// Create a new anti-memorization config with a canary token.
    pub fn new(canary_token: impl Into<String>) -> Self {
        Self {
            canary_token: canary_token.into(),
            dynamic_values: HashMap::new(),
            obfuscation_level: 1,
        }
    }

    /// Add a dynamic value.
    pub fn with_dynamic_value(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.dynamic_values.insert(key.into(), value.into());
        self
    }

    /// Add multiple dynamic values.
    pub fn with_dynamic_values(mut self, values: HashMap<String, String>) -> Self {
        self.dynamic_values.extend(values);
        self
    }

    /// Set obfuscation level.
    pub fn with_obfuscation_level(mut self, level: u8) -> Self {
        self.obfuscation_level = level.min(3);
        self
    }
}

// ============================================================================
// Task Executor Agent
// ============================================================================

/// System prompt for the privileged task creator.
const TASK_CREATION_SYSTEM_PROMPT: &str = r#"You are a PRIVILEGED benchmark task creator. You have the UNFAIR ADVANTAGE of knowing the solution.

Your job is to create a complete task specification where:
1. The PROBLEM STATEMENT reveals NOTHING about the solution approach
2. The HIDDEN SOLUTION contains the methodology other LLMs must discover
3. The problem should be solvable through work, but the answer is NOT obvious
4. The task requires GENUINE INVESTIGATION - symptoms don't directly reveal the cause

CRITICAL RULES FOR HARD TASKS:
- Problem statement must describe SYMPTOMS not causes
- Problem statement must NOT hint at the root cause or solution approach
- Problem statement must NOT mention specific tools/commands that would solve the problem
- Include MISLEADING DETAILS that could send solvers down wrong paths
- The solution must require 8-20 discrete steps (investigation + implementation)
- Include edge cases that naive approaches will miss
- Verification tests must check RESULTS not methodology

DIFFICULTY CALIBRATION:
- Easy tasks: 1-3 steps, obvious solution path, 90% expected success rate
- Medium tasks: 3-8 steps, some investigation needed, 70% expected success rate
- Hard tasks: 8-20 steps, non-obvious root cause, multiple files to check, 40% expected success rate

Include anti-memorization measures (use the provided canary_token in appropriate places).

You must respond with ONLY valid JSON, no markdown formatting or code blocks."#;

/// Template for the user prompt sent to the LLM.
const TASK_CREATION_USER_PROMPT_TEMPLATE: &str = r#"Create a complete benchmark task specification based on:

Source Task Idea:
- Title: {title}
- Description: {description}
- Category: {category}
- Subcategory: {subcategory}

Validation Assessment:
- Complexity Score: {complexity_score}
- Estimated Thinking Time: {thinking_time} seconds

Canary Token for Anti-Memorization: {canary_token}

Generate a complete task specification as JSON with this exact structure:
{{
  "problem_statement": "clear description WITHOUT solution hints - should describe WHAT needs to be accomplished without revealing HOW",
  "hidden_solution": {{
    "approach": "high-level methodology that describes the solution strategy",
    "key_insights": ["critical insight 1", "critical insight 2"],
    "reference_commands": ["example command 1", "example command 2"],
    "expected_time_seconds": {expected_time},
    "step_count": {step_count}
  }},
  "verification": {{
    "success_criteria": ["criterion 1 that checks RESULT not METHOD", "criterion 2"],
    "partial_credit": [
      {{"criterion": "partial completion criterion", "points": 0.25}}
    ],
    "automated_checks": [
      {{"type": "FileExists", "target": "path/to/file", "expected": "true"}},
      {{"type": "OutputContains", "target": "command", "expected": "expected output"}}
    ]
  }},
  "difficulty": {{
    "level": "{difficulty_level}",
    "complexity_factors": ["factor 1", "factor 2"],
    "base_score": {base_score}
  }},
  "tags": ["tag1", "tag2"]
}}"#;

/// Response structure from LLM for parsing.
#[derive(Debug, Clone, Deserialize)]
struct LlmTaskResponse {
    problem_statement: String,
    hidden_solution: LlmHiddenSolution,
    verification: LlmVerification,
    difficulty: LlmDifficulty,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmHiddenSolution {
    approach: String,
    key_insights: Vec<String>,
    reference_commands: Vec<String>,
    expected_time_seconds: u32,
    step_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmVerification {
    success_criteria: Vec<String>,
    partial_credit: Vec<LlmPartialCredit>,
    automated_checks: Vec<LlmAutomatedCheck>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmPartialCredit {
    criterion: String,
    points: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmAutomatedCheck {
    #[serde(rename = "type")]
    check_type: String,
    target: String,
    expected: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmDifficulty {
    level: String,
    complexity_factors: Vec<String>,
    base_score: f64,
}

/// Task Executor Agent that creates complete synthetic benchmark tasks.
///
/// This agent is privileged - it knows the solution approach and creates
/// task specifications that separate the problem statement (visible) from
/// the hidden solution (secret).
pub struct TaskExecutorAgent {
    /// LLM client for generation.
    llm_client: Arc<dyn LlmProvider>,
    /// Agent configuration.
    config: TaskExecutorConfig,
}

impl TaskExecutorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "task_executor";

    /// Create a new Task Executor Agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: TaskExecutorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Create a complete synthetic task from an idea and validation assessment.
    ///
    /// # Arguments
    ///
    /// * `idea` - The task idea to transform
    /// * `assessment` - Validation assessment with complexity information
    ///
    /// # Returns
    ///
    /// A complete `SyntheticTask` specification, or an error.
    pub async fn create_task(
        &self,
        idea: &TaskIdea,
        assessment: &ValidationAssessment,
    ) -> AgentResult<SyntheticTask> {
        // Generate canary token if enabled (use title as identifier since TaskIdea doesn't have id)
        let canary = if self.config.include_canary {
            self.generate_canary(&idea.title, self.config.base_seed)
                .await
        } else {
            String::new()
        };

        // Determine difficulty level from assessment
        let difficulty_level = self.determine_difficulty_level(assessment);

        // Build the prompt
        let prompt = self.build_prompt(idea, assessment, &canary, difficulty_level);

        // Call LLM
        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(TASK_CREATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        // Parse the LLM response
        let mut task = self.parse_llm_response(content, idea, &canary)?;

        // Inject dynamic values
        self.inject_dynamic_values(&mut task).await;

        Ok(task)
    }

    /// Generate a canary token for anti-memorization.
    pub async fn generate_canary(&self, task_id: &str, seed: u64) -> String {
        let canary_config = CanaryConfig::generate(task_id, seed);
        format!("{}{}", self.config.canary_prefix, canary_config.canary_id)
    }

    /// Inject dynamic values into a task for anti-memorization.
    pub async fn inject_dynamic_values(&self, task: &mut SyntheticTask) {
        // Generate timestamp-based dynamic value
        let timestamp = Utc::now().timestamp();
        task.anti_memorization
            .dynamic_values
            .insert("generation_timestamp".to_string(), timestamp.to_string());

        // Generate a random session ID
        let session_id = Uuid::new_v4().to_string();
        task.anti_memorization
            .dynamic_values
            .insert("session_id".to_string(), session_id);

        // If obfuscation level > 0, add more dynamic elements
        if task.anti_memorization.obfuscation_level > 0 {
            let random_suffix = Uuid::new_v4().to_string()[..8].to_string();
            task.anti_memorization
                .dynamic_values
                .insert("random_suffix".to_string(), random_suffix);
        }
    }

    /// Determine difficulty level from validation assessment.
    ///
    /// Difficulty is determined by complexity score, with thresholds adjusted
    /// to ensure hard tasks require significant investigation:
    /// - Easy: complexity < 0.40 (simple, direct tasks)
    /// - Medium: complexity 0.40-0.60 (some investigation needed)
    /// - Hard: complexity >= 0.60 (non-obvious root cause, multi-step investigation)
    fn determine_difficulty_level(&self, assessment: &ValidationAssessment) -> DifficultyLevel {
        let score = assessment.complexity_score;
        if score < 0.40 {
            DifficultyLevel::Easy
        } else if score < 0.60 {
            DifficultyLevel::Medium
        } else {
            DifficultyLevel::Hard
        }
    }

    /// Build the user prompt for the LLM.
    fn build_prompt(
        &self,
        idea: &TaskIdea,
        assessment: &ValidationAssessment,
        canary: &str,
        difficulty_level: DifficultyLevel,
    ) -> String {
        let (min_time, max_time) = difficulty_level.expected_time_range();
        // Convert minutes to seconds for expected_time calculation
        let thinking_time_seconds = assessment.estimated_thinking_time_minutes * 60;
        let expected_time = thinking_time_seconds.clamp(min_time, max_time);

        let (min_steps, max_steps) = difficulty_level.command_steps_range();
        let step_count = match difficulty_level {
            DifficultyLevel::Easy => min_steps + 1,
            DifficultyLevel::Medium => (min_steps + max_steps) / 2,
            DifficultyLevel::Hard => max_steps - 2,
        };

        let difficulty_str = match difficulty_level {
            DifficultyLevel::Easy => "easy",
            DifficultyLevel::Medium => "medium",
            DifficultyLevel::Hard => "hard",
        };

        // TaskIdea doesn't have subcategory field, so we use an empty string
        let subcategory = String::new();

        TASK_CREATION_USER_PROMPT_TEMPLATE
            .replace("{title}", &idea.title)
            .replace("{description}", &idea.description)
            .replace("{category}", &idea.category)
            .replace("{subcategory}", &subcategory)
            .replace(
                "{complexity_score}",
                &format!("{:.2}", assessment.complexity_score),
            )
            .replace(
                "{thinking_time}",
                &(assessment.estimated_thinking_time_minutes * 60).to_string(),
            )
            .replace("{canary_token}", canary)
            .replace("{expected_time}", &expected_time.to_string())
            .replace("{step_count}", &step_count.to_string())
            .replace("{difficulty_level}", difficulty_str)
            .replace("{base_score}", &difficulty_level.base_points().to_string())
    }

    /// Parse the LLM response into a SyntheticTask.
    fn parse_llm_response(
        &self,
        content: &str,
        idea: &TaskIdea,
        canary: &str,
    ) -> AgentResult<SyntheticTask> {
        // Try to extract JSON from the response (handle markdown code blocks)
        let result = try_extract_json_from_response(content);
        let json_content = result.into_result_with_context(content).map_err(|e| {
            match &e {
                JsonExtractionError::Truncated { partial_preview, unclosed_braces, unclosed_brackets } => {
                    AgentError::ResponseParseError(format!(
                        "JSON appears truncated: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                        unclosed_braces, unclosed_brackets, partial_preview
                    ))
                }
                JsonExtractionError::NotFound { content_preview } => {
                    AgentError::ResponseParseError(format!(
                        "Could not extract JSON from response. Content starts with: '{}'",
                        content_preview
                    ))
                }
            }
        })?;

        let llm_response: LlmTaskResponse = serde_json::from_str(&json_content).map_err(|e| {
            // Safely truncate to avoid char boundary issues
            let truncated: String = json_content.chars().take(500).collect();
            AgentError::ResponseParseError(format!(
                "Failed to parse LLM response as JSON: {}. Content: {}",
                e, truncated
            ))
        })?;

        // Convert LLM response to our domain types
        let hidden_solution = HiddenSolution::new(&llm_response.hidden_solution.approach)
            .with_key_insights(llm_response.hidden_solution.key_insights)
            .with_reference_commands(llm_response.hidden_solution.reference_commands)
            .with_expected_time_seconds(llm_response.hidden_solution.expected_time_seconds)
            .with_step_count(llm_response.hidden_solution.step_count);

        let partial_credit: Vec<PartialCreditItem> = llm_response
            .verification
            .partial_credit
            .into_iter()
            .map(|pc| PartialCreditItem::new(&pc.criterion, pc.points))
            .collect();

        let automated_checks: Vec<AutomatedCheck> = llm_response
            .verification
            .automated_checks
            .into_iter()
            .map(|ac| {
                let check_type = parse_check_type(&ac.check_type);
                AutomatedCheck::new(check_type, &ac.target, &ac.expected)
            })
            .collect();

        let verification = VerificationSpec::new()
            .with_success_criteria(llm_response.verification.success_criteria)
            .with_partial_credit(partial_credit)
            .with_automated_checks(automated_checks);

        let difficulty_level = parse_difficulty_level(&llm_response.difficulty.level);
        let difficulty = DifficultyScoring::new(difficulty_level)
            .with_complexity_factors(llm_response.difficulty.complexity_factors)
            .with_base_score(llm_response.difficulty.base_score);

        // Use the task title as source_idea_id since TaskIdea doesn't have an id field
        let metadata = TaskMetadata::new(&idea.category, &idea.title).with_tags(llm_response.tags);

        let anti_memorization = if !canary.is_empty() {
            AntiMemorizationConfig::new(canary)
        } else {
            AntiMemorizationConfig::default()
        };

        let task = SyntheticTask::new(
            llm_response.problem_statement,
            hidden_solution,
            verification,
            difficulty,
            metadata,
        )
        .with_anti_memorization(anti_memorization);

        Ok(task)
    }

    /// Get the agent configuration.
    pub fn config(&self) -> &TaskExecutorConfig {
        &self.config
    }
}

/// Parse a check type string into CheckType enum.
fn parse_check_type(s: &str) -> CheckType {
    match s.to_lowercase().as_str() {
        "fileexists" | "file_exists" => CheckType::FileExists,
        "outputcontains" | "output_contains" => CheckType::OutputContains,
        "outputmatches" | "output_matches" => CheckType::OutputMatches,
        "commandsucceeds" | "command_succeeds" => CheckType::CommandSucceeds,
        "exitcode" | "exit_code" => CheckType::ExitCode,
        _ => CheckType::Custom,
    }
}

/// Parse a difficulty level string into DifficultyLevel enum.
fn parse_difficulty_level(s: &str) -> DifficultyLevel {
    match s.to_lowercase().as_str() {
        "easy" => DifficultyLevel::Easy,
        "hard" => DifficultyLevel::Hard,
        _ => DifficultyLevel::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use crate::utils::json_extraction::extract_json_from_response;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        response: Mutex<String>,
    }

    impl MockLlmProvider {
        fn new(response: &str) -> Self {
            Self {
                response: Mutex::new(response.to_string()),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, LlmError> {
            let content = self.response.lock().expect("lock poisoned").clone();
            Ok(GenerationResponse {
                id: "test-id".to_string(),
                model: "test-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            })
        }
    }

    fn mock_llm_response() -> String {
        r#"{
            "problem_statement": "Given a directory of log files, identify which file contains the most error entries and report the count.",
            "hidden_solution": {
                "approach": "Use grep to count ERROR occurrences in each file, then compare counts",
                "key_insights": ["grep -c counts matches per file", "Use sort -n to find maximum"],
                "reference_commands": ["grep -c 'ERROR' *.log", "grep -c 'ERROR' *.log | sort -t: -k2 -n | tail -1"],
                "expected_time_seconds": 180,
                "step_count": 3
            },
            "verification": {
                "success_criteria": ["Correct file identified", "Correct count reported"],
                "partial_credit": [
                    {"criterion": "Found at least one error file", "points": 0.25},
                    {"criterion": "Counted errors in all files", "points": 0.5}
                ],
                "automated_checks": [
                    {"type": "OutputContains", "target": "result.txt", "expected": "app.log"},
                    {"type": "ExitCode", "target": "validate.sh", "expected": "0"}
                ]
            },
            "difficulty": {
                "level": "medium",
                "complexity_factors": ["Multiple files to analyze", "Requires comparison logic"],
                "base_score": 25.0
            },
            "tags": ["log-analysis", "grep", "debugging"]
        }"#.to_string()
    }

    #[test]
    fn test_task_idea_creation() {
        // Using the existing TaskIdea from task_validator module
        let idea = TaskIdea::new(
            "Find Errors",
            "Find error entries in log files",
            "debugging",
            vec!["grep".to_string(), "file-navigation".to_string()],
        );

        assert_eq!(idea.title, "Find Errors");
        assert_eq!(idea.category, "debugging");
        assert_eq!(idea.required_skills.len(), 2);
    }

    #[test]
    fn test_validation_assessment_creation() {
        // Using the existing ValidationAssessment from task_validator module
        let assessment = ValidationAssessment::valid(0.65, 0.2, 5, "Task seems well-scoped");

        assert!((assessment.complexity_score - 0.65).abs() < 0.01);
        assert_eq!(assessment.estimated_thinking_time_minutes, 5);
        assert!(assessment.is_valid);
    }

    #[test]
    fn test_config_builder() {
        let config = TaskExecutorConfig::new()
            .with_temperature(0.7)
            .with_max_tokens(2000)
            .with_include_canary(false)
            .with_canary_prefix("TEST_")
            .with_base_seed(123);

        assert!((config.temperature - 0.7).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
        assert!(!config.include_canary);
        assert_eq!(config.canary_prefix, "TEST_");
        assert_eq!(config.base_seed, 123);
    }

    #[test]
    fn test_hidden_solution_builder() {
        let solution = HiddenSolution::new("Use grep to find patterns")
            .with_key_insights(["insight1", "insight2"])
            .with_reference_commands(["grep pattern file.txt"])
            .with_expected_time_seconds(120)
            .with_step_count(2);

        assert_eq!(solution.approach, "Use grep to find patterns");
        assert_eq!(solution.key_insights.len(), 2);
        assert_eq!(solution.reference_commands.len(), 1);
        assert_eq!(solution.expected_time_seconds, 120);
        assert_eq!(solution.step_count, 2);
    }

    #[test]
    fn test_verification_spec_builder() {
        let spec = VerificationSpec::new()
            .with_success_criteria(["File exists", "Content correct"])
            .with_partial_credit([PartialCreditItem::new("Partial progress", 0.5)])
            .with_automated_checks([AutomatedCheck::file_exists("/tmp/output.txt")])
            .with_manual_review(true);

        assert_eq!(spec.success_criteria.len(), 2);
        assert_eq!(spec.partial_credit_criteria.len(), 1);
        assert_eq!(spec.automated_checks.len(), 1);
        assert!(spec.manual_review_required);
    }

    #[test]
    fn test_automated_check_constructors() {
        let file_check = AutomatedCheck::file_exists("/path/to/file");
        assert_eq!(file_check.check_type, CheckType::FileExists);
        assert_eq!(file_check.target, "/path/to/file");

        let output_check = AutomatedCheck::output_contains("cmd", "pattern");
        assert_eq!(output_check.check_type, CheckType::OutputContains);

        let cmd_check = AutomatedCheck::command_succeeds("echo hello");
        assert_eq!(cmd_check.check_type, CheckType::CommandSucceeds);

        let exit_check = AutomatedCheck::exit_code("cmd", 1);
        assert_eq!(exit_check.check_type, CheckType::ExitCode);
        assert_eq!(exit_check.expected, "1");
    }

    #[test]
    fn test_difficulty_scoring() {
        let scoring = DifficultyScoring::new(DifficultyLevel::Medium)
            .with_complexity_factors(["factor1", "factor2"])
            .with_base_score(30.0)
            .with_time_bonus_eligible(false);

        assert_eq!(scoring.level, DifficultyLevel::Medium);
        assert_eq!(scoring.complexity_factors.len(), 2);
        assert!((scoring.base_score - 30.0).abs() < 0.01);
        assert!(!scoring.time_bonus_eligible);
    }

    #[test]
    fn test_task_metadata() {
        let metadata = TaskMetadata::new("debugging", "idea-123")
            .with_subcategory("log-analysis")
            .with_tags(["tag1", "tag2"]);

        assert_eq!(metadata.category, "debugging");
        assert_eq!(metadata.source_idea_id, "idea-123");
        assert_eq!(metadata.subcategory, "log-analysis");
        assert_eq!(metadata.tags.len(), 2);
    }

    #[test]
    fn test_anti_memorization_config() {
        let config = AntiMemorizationConfig::new("CANARY_123")
            .with_dynamic_value("key1", "value1")
            .with_obfuscation_level(2);

        assert_eq!(config.canary_token, "CANARY_123");
        assert_eq!(
            config.dynamic_values.get("key1"),
            Some(&"value1".to_string())
        );
        assert_eq!(config.obfuscation_level, 2);
    }

    #[test]
    fn test_synthetic_task_creation() {
        let solution = HiddenSolution::new("Test approach");
        let verification = VerificationSpec::new();
        let difficulty = DifficultyScoring::new(DifficultyLevel::Easy);
        let metadata = TaskMetadata::new("test", "idea-1");

        let task = SyntheticTask::new(
            "Solve this problem",
            solution,
            verification,
            difficulty,
            metadata,
        );

        assert_eq!(task.version, "1.0.0");
        assert_eq!(task.problem_statement, "Solve this problem");
        assert!(!task.has_canary());
    }

    #[test]
    fn test_extract_json_from_response() {
        // Test raw JSON
        let raw = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_response(raw), raw);

        // Test markdown code fence
        let markdown = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(markdown), r#"{"key": "value"}"#);

        // Test generic code fence
        let generic = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(generic), r#"{"key": "value"}"#);

        // Test with surrounding text
        let surrounded = "Here is the JSON:\n{\"key\": \"value\"}\nEnd.";
        assert_eq!(
            extract_json_from_response(surrounded),
            r#"{"key": "value"}"#
        );
    }

    #[test]
    fn test_parse_check_type() {
        assert_eq!(parse_check_type("FileExists"), CheckType::FileExists);
        assert_eq!(parse_check_type("file_exists"), CheckType::FileExists);
        assert_eq!(
            parse_check_type("OutputContains"),
            CheckType::OutputContains
        );
        assert_eq!(
            parse_check_type("CommandSucceeds"),
            CheckType::CommandSucceeds
        );
        assert_eq!(parse_check_type("ExitCode"), CheckType::ExitCode);
        assert_eq!(parse_check_type("unknown"), CheckType::Custom);
    }

    #[test]
    fn test_parse_difficulty_level() {
        assert_eq!(parse_difficulty_level("easy"), DifficultyLevel::Easy);
        assert_eq!(parse_difficulty_level("EASY"), DifficultyLevel::Easy);
        assert_eq!(parse_difficulty_level("medium"), DifficultyLevel::Medium);
        assert_eq!(parse_difficulty_level("hard"), DifficultyLevel::Hard);
        assert_eq!(parse_difficulty_level("unknown"), DifficultyLevel::Medium);
    }

    #[tokio::test]
    async fn test_task_executor_create_task() {
        let mock_llm = Arc::new(MockLlmProvider::new(&mock_llm_response()));
        let config = TaskExecutorConfig::new().with_include_canary(true);
        let agent = TaskExecutorAgent::new(mock_llm, config);

        let idea = TaskIdea::new(
            "Log Analysis",
            "Find errors in logs",
            "debugging",
            vec!["grep".to_string()],
        );
        let assessment = ValidationAssessment::valid(0.5, 0.2, 3, "Good task");

        let task = agent
            .create_task(&idea, &assessment)
            .await
            .expect("task creation should succeed");

        assert!(!task.problem_statement.is_empty());
        assert!(!task.hidden_solution.approach.is_empty());
        assert!(!task.verification.success_criteria.is_empty());
        assert_eq!(task.difficulty.level, DifficultyLevel::Medium);
        assert_eq!(task.metadata.category, "debugging");
        assert!(task.has_canary());
    }

    #[tokio::test]
    async fn test_generate_canary() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let config = TaskExecutorConfig::new().with_canary_prefix("TEST_CANARY_");
        let agent = TaskExecutorAgent::new(mock_llm, config);

        let canary1 = agent.generate_canary("task-1", 42).await;
        let canary2 = agent.generate_canary("task-1", 42).await;
        let canary3 = agent.generate_canary("task-2", 42).await;

        // Same task and seed should produce same canary
        assert_eq!(canary1, canary2);
        // Different task should produce different canary
        assert_ne!(canary1, canary3);
        // Should have correct prefix
        assert!(canary1.starts_with("TEST_CANARY_"));
    }

    #[tokio::test]
    async fn test_inject_dynamic_values() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let config = TaskExecutorConfig::new();
        let agent = TaskExecutorAgent::new(mock_llm, config);

        let solution = HiddenSolution::new("Test approach");
        let verification = VerificationSpec::new();
        let difficulty = DifficultyScoring::new(DifficultyLevel::Easy);
        let metadata = TaskMetadata::new("test", "idea-1");
        let mut task = SyntheticTask::new("Problem", solution, verification, difficulty, metadata);

        agent.inject_dynamic_values(&mut task).await;

        assert!(task
            .anti_memorization
            .dynamic_values
            .contains_key("generation_timestamp"));
        assert!(task
            .anti_memorization
            .dynamic_values
            .contains_key("session_id"));
        assert!(task
            .anti_memorization
            .dynamic_values
            .contains_key("random_suffix"));
    }

    #[test]
    fn test_determine_difficulty_level() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let config = TaskExecutorConfig::new();
        let agent = TaskExecutorAgent::new(mock_llm, config);

        let easy = ValidationAssessment::valid(0.2, 0.1, 1, "Easy task");
        assert_eq!(
            agent.determine_difficulty_level(&easy),
            DifficultyLevel::Easy
        );

        let medium = ValidationAssessment::valid(0.5, 0.2, 5, "Medium task");
        assert_eq!(
            agent.determine_difficulty_level(&medium),
            DifficultyLevel::Medium
        );

        let hard = ValidationAssessment::valid(0.8, 0.1, 15, "Hard task");
        assert_eq!(
            agent.determine_difficulty_level(&hard),
            DifficultyLevel::Hard
        );
    }

    #[test]
    fn test_build_prompt() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let config = TaskExecutorConfig::new();
        let agent = TaskExecutorAgent::new(mock_llm, config);

        let idea = TaskIdea::new(
            "Test Task",
            "Test description",
            "testing",
            vec!["skill1".to_string()],
        );
        let assessment = ValidationAssessment::valid(0.5, 0.2, 5, "Good task");

        let prompt = agent.build_prompt(&idea, &assessment, "CANARY_123", DifficultyLevel::Medium);

        assert!(prompt.contains("Test Task"));
        assert!(prompt.contains("Test description"));
        assert!(prompt.contains("testing"));
        assert!(prompt.contains("CANARY_123"));
        assert!(prompt.contains("0.50")); // complexity score formatted
    }

    #[test]
    fn test_partial_credit_item_clamping() {
        let item = PartialCreditItem::new("criterion", 1.5);
        assert!((item.points - 1.0).abs() < 0.01);

        let item2 = PartialCreditItem::new("criterion", -0.5);
        assert!((item2.points - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_anti_memorization_obfuscation_clamping() {
        let config = AntiMemorizationConfig::new("canary").with_obfuscation_level(10);
        assert_eq!(config.obfuscation_level, 3);
    }
}
