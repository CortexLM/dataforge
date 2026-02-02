//! Problem Crafter Agent for reformulating problem statements.
//!
//! This agent takes analyzed tasks and crafts clear, pedagogical problem
//! statements suitable for AI benchmarks.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::analyzer_agent::AnalyzedTask;
use super::error::{AgentError, AgentResult};

/// System prompt for problem crafting.
const CRAFTING_SYSTEM_PROMPT: &str = r#"You are an expert benchmark task designer creating clear, autonomous problem statements for AI evaluation.

Your job is to transform a raw task description into a well-structured benchmark problem that:
1. Is SELF-CONTAINED - all necessary information is provided
2. Has CLEAR OBJECTIVES - what needs to be accomplished is unambiguous
3. Is PEDAGOGICALLY STRUCTURED - context → problem → requirements → constraints
4. REMOVES EXTERNAL DEPENDENCIES - no links to external resources that might change
5. PROVIDES CONTEXT - includes relevant background information

Guidelines:
- Replace external references (links, forum posts, etc.) with inline information
- Add technical context needed to understand the problem
- Structure the problem logically: scenario → task → acceptance criteria
- Make success criteria measurable and verifiable
- Optionally include hints that guide without giving away the solution

Output Format:
You MUST respond with ONLY a JSON object in this exact format:
{
  "crafted_statement": "<the reformulated problem statement>",
  "hints": ["hint1", "hint2", ...],
  "context_provided": "<summary of technical context added>",
  "external_refs_removed": ["ref1", "ref2", ...]
}

Do not include any text outside the JSON object."#;

/// User prompt template for problem crafting.
const CRAFTING_USER_TEMPLATE: &str = r#"Transform the following task into a well-structured benchmark problem:

Category: {category}
Subcategory: {subcategory}
Difficulty: {difficulty}
Estimated Time: {time} minutes

Original Title: {title}

Original Description:
{description}

Technical Context:
{technical_context}

Required Skills: {skills}
Complexity Factors: {complexity_factors}

{hints_instruction}

Create a clear, self-contained problem statement that an AI can attempt without external resources.
Maximum length: {max_length} characters."#;

/// Configuration for the Problem Crafter Agent.
#[derive(Debug, Clone)]
pub struct CrafterConfig {
    /// Maximum length of the crafted statement in characters.
    pub max_length: usize,
    /// Whether to include hints in the output.
    pub include_hints: bool,
    /// Whether to structure the problem pedagogically.
    pub pedagogical_structure: bool,
    /// Whether to remove external references.
    pub remove_external_refs: bool,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for CrafterConfig {
    fn default() -> Self {
        Self {
            max_length: 2000,
            include_hints: true,
            pedagogical_structure: true,
            remove_external_refs: true,
            temperature: 0.5,
            max_tokens: 1500,
        }
    }
}

impl CrafterConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum length for the crafted statement.
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }

    /// Enables or disables hint inclusion.
    pub fn with_include_hints(mut self, include: bool) -> Self {
        self.include_hints = include;
        self
    }

    /// Enables or disables pedagogical structure.
    pub fn with_pedagogical_structure(mut self, enabled: bool) -> Self {
        self.pedagogical_structure = enabled;
        self
    }

    /// Enables or disables external reference removal.
    pub fn with_remove_external_refs(mut self, enabled: bool) -> Self {
        self.remove_external_refs = enabled;
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

/// A crafted problem statement ready for benchmark use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftedProblem {
    /// The original problem statement from the analyzed task.
    pub original_statement: String,
    /// The reformulated, well-structured problem statement.
    pub crafted_statement: String,
    /// Optional hints that guide without giving away the solution.
    pub hints: Vec<String>,
    /// Summary of technical context that was added.
    pub context_provided: String,
    /// External references that were removed or replaced.
    pub external_refs_removed: Vec<String>,
}

impl CraftedProblem {
    /// Creates a new crafted problem.
    pub fn new(
        original_statement: impl Into<String>,
        crafted_statement: impl Into<String>,
        hints: Vec<String>,
        context_provided: impl Into<String>,
        external_refs_removed: Vec<String>,
    ) -> Self {
        Self {
            original_statement: original_statement.into(),
            crafted_statement: crafted_statement.into(),
            hints,
            context_provided: context_provided.into(),
            external_refs_removed,
        }
    }

    /// Returns true if hints are available.
    pub fn has_hints(&self) -> bool {
        !self.hints.is_empty()
    }

    /// Returns the number of external references that were removed.
    pub fn removed_refs_count(&self) -> usize {
        self.external_refs_removed.len()
    }

    /// Returns the length of the crafted statement in characters.
    pub fn statement_length(&self) -> usize {
        self.crafted_statement.len()
    }
}

/// Problem Crafter Agent that reformulates problem statements.
///
/// This agent takes analyzed tasks and creates clear, self-contained
/// problem statements suitable for AI benchmarks.
pub struct ProblemCrafterAgent {
    llm: Arc<dyn LlmProvider>,
}

impl std::fmt::Debug for ProblemCrafterAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProblemCrafterAgent")
            .finish_non_exhaustive()
    }
}

impl ProblemCrafterAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "problem_crafter";

    /// Creates a new problem crafter agent with the given LLM provider.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Crafts a problem statement from an analyzed task.
    ///
    /// # Arguments
    ///
    /// * `task` - The analyzed task to craft a problem from.
    /// * `config` - Configuration for the crafting process.
    ///
    /// # Returns
    ///
    /// A `CraftedProblem` with the reformulated statement.
    pub async fn craft(
        &self,
        task: &AnalyzedTask,
        config: &CrafterConfig,
    ) -> AgentResult<CraftedProblem> {
        let prompt = self.build_crafting_prompt(task, config);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(CRAFTING_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(config.temperature)
        .with_max_tokens(config.max_tokens);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_crafting_response(task, content)
    }

    /// Crafts multiple problems in batch.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The analyzed tasks to craft problems from.
    /// * `config` - Configuration for the crafting process.
    ///
    /// # Returns
    ///
    /// A vector of crafted problems. Failed crafts are logged but not included.
    pub async fn craft_batch(
        &self,
        tasks: &[AnalyzedTask],
        config: &CrafterConfig,
    ) -> AgentResult<Vec<CraftedProblem>> {
        let mut crafted_problems = Vec::with_capacity(tasks.len());

        for task in tasks {
            match self.craft(task, config).await {
                Ok(problem) => crafted_problems.push(problem),
                Err(e) => {
                    tracing::warn!("Failed to craft problem for '{}': {}", task.title(), e);
                }
            }
        }

        if crafted_problems.is_empty() && !tasks.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Failed to craft any problems".to_string(),
            ));
        }

        Ok(crafted_problems)
    }

    /// Builds the user prompt for problem crafting.
    fn build_crafting_prompt(&self, task: &AnalyzedTask, config: &CrafterConfig) -> String {
        let difficulty_str = match task.difficulty {
            crate::difficulty::DifficultyLevel::Easy => "Easy",
            crate::difficulty::DifficultyLevel::Medium => "Medium",
            crate::difficulty::DifficultyLevel::Hard => "Hard",
        };

        let skills_str = if task.required_skills.is_empty() {
            "Not specified".to_string()
        } else {
            task.required_skills.join(", ")
        };

        let complexity_str = if task.complexity_factors.is_empty() {
            "Not specified".to_string()
        } else {
            task.complexity_factors.join(", ")
        };

        let hints_instruction = if config.include_hints {
            "Include 2-4 hints that guide towards the solution without giving it away."
        } else {
            "Do not include any hints."
        };

        CRAFTING_USER_TEMPLATE
            .replace("{category}", task.category.display_name())
            .replace("{subcategory}", &task.subcategory)
            .replace("{difficulty}", difficulty_str)
            .replace("{time}", &task.estimated_time_minutes.to_string())
            .replace("{title}", task.title())
            .replace("{description}", task.description())
            .replace("{technical_context}", &task.technical_context)
            .replace("{skills}", &skills_str)
            .replace("{complexity_factors}", &complexity_str)
            .replace("{hints_instruction}", hints_instruction)
            .replace("{max_length}", &config.max_length.to_string())
    }

    /// Parses the LLM response into a CraftedProblem.
    fn parse_crafting_response(
        &self,
        task: &AnalyzedTask,
        content: &str,
    ) -> AgentResult<CraftedProblem> {
        let json_content = self.extract_json(content)?;

        let parsed: CraftingResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        Ok(CraftedProblem::new(
            task.description(),
            parsed.crafted_statement,
            parsed.hints.unwrap_or_default(),
            parsed.context_provided,
            parsed.external_refs_removed.unwrap_or_default(),
        ))
    }

    /// Extracts JSON from the response, handling potential markdown code blocks.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        let trimmed = content.trim();

        // If it already starts with '{', use as-is
        if trimmed.starts_with('{') {
            if let Some(end) = find_matching_brace(trimmed) {
                return Ok(trimmed[..=end].to_string());
            }
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
            if let Some(end) = find_matching_brace(&trimmed[start..]) {
                return Ok(trimmed[start..=start + end].to_string());
            }
        }

        Err(AgentError::ResponseParseError(
            "Could not extract JSON from response".to_string(),
        ))
    }
}

/// Helper function to find the matching closing brace for a JSON object.
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '{' if !in_string => {
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }

    None
}

/// Response structure from LLM crafting.
#[derive(Debug, Deserialize)]
struct CraftingResponse {
    crafted_statement: String,
    hints: Option<Vec<String>>,
    context_provided: String,
    external_refs_removed: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::analyzer_agent::TaskCategory;
    use crate::agents::collector_agent::{CollectedTask, TaskSource};
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
                    prompt_tokens: 150,
                    completion_tokens: 200,
                    total_tokens: 350,
                },
            })
        }
    }

    fn create_test_analyzed_task() -> AnalyzedTask {
        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Fix nginx reverse proxy",
            "My nginx reverse proxy returns 502 errors when forwarding requests to upstream...",
        )
        .with_tags(vec!["nginx".to_string(), "reverse-proxy".to_string()]);

        AnalyzedTask::new(
            task,
            TaskCategory::Configuration,
            "web-server",
            DifficultyLevel::Medium,
            vec!["nginx".to_string(), "networking".to_string()],
            "Nginx reverse proxy configuration for upstream services",
            15,
            vec![
                "upstream timeout".to_string(),
                "header forwarding".to_string(),
            ],
        )
    }

    #[test]
    fn test_crafter_config_defaults() {
        let config = CrafterConfig::default();
        assert_eq!(config.max_length, 2000);
        assert!(config.include_hints);
        assert!(config.pedagogical_structure);
        assert!(config.remove_external_refs);
    }

    #[test]
    fn test_crafter_config_builder() {
        let config = CrafterConfig::new()
            .with_max_length(1500)
            .with_include_hints(false)
            .with_pedagogical_structure(true)
            .with_remove_external_refs(false)
            .with_temperature(0.7)
            .with_max_tokens(2000);

        assert_eq!(config.max_length, 1500);
        assert!(!config.include_hints);
        assert!(config.pedagogical_structure);
        assert!(!config.remove_external_refs);
        assert!((config.temperature - 0.7).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
    }

    #[test]
    fn test_crafted_problem_creation() {
        let problem = CraftedProblem::new(
            "Original problem description",
            "# Problem Statement\n\nYou have an nginx server...",
            vec![
                "Check the upstream timeout".to_string(),
                "Verify headers".to_string(),
            ],
            "Added nginx configuration context",
            vec!["https://example.com/docs".to_string()],
        );

        assert!(problem.has_hints());
        assert_eq!(problem.hints.len(), 2);
        assert_eq!(problem.removed_refs_count(), 1);
        assert!(problem.statement_length() > 0);
    }

    #[test]
    fn test_crafted_problem_no_hints() {
        let problem = CraftedProblem::new("Original", "Crafted", Vec::new(), "Context", Vec::new());

        assert!(!problem.has_hints());
        assert_eq!(problem.removed_refs_count(), 0);
    }

    #[tokio::test]
    async fn test_craft_success() {
        let mock_response = r#"{
            "crafted_statement": "Scenario: You are managing an nginx reverse proxy server. Problem: Users report 502 errors. Task: Diagnose and fix the nginx configuration. Acceptance Criteria: Requests are properly forwarded, No 502 errors under normal load.",
            "hints": [
                "Check the upstream block configuration",
                "Verify the proxy_pass directive",
                "Consider timeout settings for slow upstream responses"
            ],
            "context_provided": "Added information about nginx proxy_pass and upstream configuration",
            "external_refs_removed": ["https://nginx.org/docs", "https://stackoverflow.com/q/12345"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = ProblemCrafterAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let config = CrafterConfig::default();

        let problem = agent.craft(&task, &config).await.expect("should succeed");

        assert!(!problem.crafted_statement.is_empty());
        assert!(problem.crafted_statement.contains("Scenario"));
        assert!(problem.has_hints());
        assert_eq!(problem.hints.len(), 3);
        assert_eq!(problem.removed_refs_count(), 2);
    }

    #[tokio::test]
    async fn test_craft_with_markdown_response() {
        let mock_response = r#"Here's the crafted problem:

```json
{
    "crafted_statement": "You need to fix a server configuration issue.",
    "hints": ["Check logs first"],
    "context_provided": "Server configuration basics",
    "external_refs_removed": []
}
```

This problem tests configuration skills."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = ProblemCrafterAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let config = CrafterConfig::default();

        let problem = agent.craft(&task, &config).await.expect("should succeed");

        assert!(!problem.crafted_statement.is_empty());
        assert!(problem.has_hints());
    }

    #[tokio::test]
    async fn test_craft_without_optional_fields() {
        let mock_response = r#"{
            "crafted_statement": "Simple problem statement",
            "context_provided": "Basic context"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = ProblemCrafterAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let config = CrafterConfig::new().with_include_hints(false);

        let problem = agent.craft(&task, &config).await.expect("should succeed");

        assert!(!problem.crafted_statement.is_empty());
        assert!(!problem.has_hints()); // hints was None, defaulted to empty vec
        assert_eq!(problem.removed_refs_count(), 0);
    }

    #[tokio::test]
    async fn test_craft_batch() {
        let mock_response = r#"{
            "crafted_statement": "Batch problem statement",
            "hints": [],
            "context_provided": "Batch context",
            "external_refs_removed": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = ProblemCrafterAgent::new(mock_provider);

        let tasks = vec![create_test_analyzed_task(), create_test_analyzed_task()];

        let config = CrafterConfig::default();
        let problems = agent
            .craft_batch(&tasks, &config)
            .await
            .expect("should succeed");

        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn test_build_crafting_prompt_includes_config() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = ProblemCrafterAgent::new(mock_provider);

        let task = create_test_analyzed_task();

        // With hints
        let config_with_hints = CrafterConfig::new().with_include_hints(true);
        let prompt = agent.build_crafting_prompt(&task, &config_with_hints);
        assert!(prompt.contains("Include 2-4 hints"));

        // Without hints
        let config_no_hints = CrafterConfig::new().with_include_hints(false);
        let prompt = agent.build_crafting_prompt(&task, &config_no_hints);
        assert!(prompt.contains("Do not include any hints"));
    }

    #[test]
    fn test_find_matching_brace() {
        assert_eq!(find_matching_brace(r#"{}"#), Some(1));
        assert_eq!(find_matching_brace(r#"{"a": 1}"#), Some(7));
        assert_eq!(find_matching_brace(r#"{"a": {"b": 2}}"#), Some(14));
        assert_eq!(find_matching_brace(r#"{"a": "}"}"#), Some(9));
        assert_eq!(find_matching_brace(r#"{"#), None);
    }

    #[test]
    fn test_agent_name_constant() {
        assert_eq!(ProblemCrafterAgent::AGENT_NAME, "problem_crafter");
    }
}
