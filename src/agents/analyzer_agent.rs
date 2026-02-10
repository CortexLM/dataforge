//! Analyzer Agent for technical context extraction and task categorization.
//!
//! This agent extracts technical context from collected tasks and categorizes them
//! for the benchmark generation system.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::collector_agent::CollectedTask;
use super::error::{AgentError, AgentResult};
use crate::difficulty::DifficultyLevel;
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for task analysis.
const ANALYSIS_SYSTEM_PROMPT: &str = r#"You are an expert software engineer analyzing tasks for AI benchmark creation.

Your job is to:
1. CATEGORIZE the task into a primary category
2. Identify a SUBCATEGORY for more specific classification
3. Estimate the DIFFICULTY level based on required expertise and steps
4. Extract REQUIRED SKILLS needed to complete the task
5. Provide TECHNICAL CONTEXT explaining the problem domain
6. Estimate TIME to complete for an experienced professional
7. List COMPLEXITY FACTORS that make this task challenging

Categories:
- debugging: Error investigation, log analysis, crash debugging
- security: Vulnerability detection, hardening, incident response
- configuration: Service setup, environment configuration
- database: SQL, migrations, query optimization, data integrity
- networking: DNS, firewall, proxy, protocol issues
- containerization: Docker, Kubernetes, orchestration
- file_manipulation: Text processing, search/replace, file organization
- system_admin: User management, permissions, service operations
- other: Tasks that don't fit other categories

Difficulty Levels:
- easy: Single-step, 1-5 minutes, basic knowledge
- medium: Multi-step, 5-15 minutes, moderate expertise
- hard: Complex, 15-60 minutes, deep domain knowledge

Output Format:
You MUST respond with ONLY a JSON object in this exact format:
{
  "category": "<category from list above>",
  "subcategory": "<specific subcategory string>",
  "difficulty": "easy|medium|hard",
  "required_skills": ["skill1", "skill2", ...],
  "technical_context": "<explanation of the problem domain and relevant concepts>",
  "estimated_time_minutes": <integer>,
  "complexity_factors": ["factor1", "factor2", ...]
}

Do not include any text outside the JSON object."#;

/// User prompt template for task analysis.
const ANALYSIS_USER_TEMPLATE: &str = r#"Analyze the following task for benchmark creation:

Source: {source}
Title: {title}

Description:
{description}

Tags: {tags}
Code snippets available: {has_code}

Provide a detailed analysis of this task."#;

/// Categories for analyzed tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    /// Error investigation, log analysis, crash debugging.
    Debugging,
    /// Vulnerability detection, hardening, incident response.
    Security,
    /// Service setup, environment configuration.
    Configuration,
    /// SQL, migrations, query optimization, data integrity.
    Database,
    /// DNS, firewall, proxy, protocol issues.
    Networking,
    /// Docker, Kubernetes, orchestration.
    Containerization,
    /// Text processing, search/replace, file organization.
    FileManipulation,
    /// User management, permissions, service operations.
    SystemAdmin,
    /// Tasks that don't fit other categories.
    Other,
}

impl TaskCategory {
    /// Returns all available task categories.
    pub fn all() -> Vec<TaskCategory> {
        vec![
            TaskCategory::Debugging,
            TaskCategory::Security,
            TaskCategory::Configuration,
            TaskCategory::Database,
            TaskCategory::Networking,
            TaskCategory::Containerization,
            TaskCategory::FileManipulation,
            TaskCategory::SystemAdmin,
            TaskCategory::Other,
        ]
    }

    /// Returns the display name for this category.
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskCategory::Debugging => "Debugging",
            TaskCategory::Security => "Security",
            TaskCategory::Configuration => "Configuration",
            TaskCategory::Database => "Database",
            TaskCategory::Networking => "Networking",
            TaskCategory::Containerization => "Containerization",
            TaskCategory::FileManipulation => "File Manipulation",
            TaskCategory::SystemAdmin => "System Administration",
            TaskCategory::Other => "Other",
        }
    }

    /// Parses a category from a string.
    pub fn parse(s: &str) -> Option<TaskCategory> {
        match s.to_lowercase().trim() {
            "debugging" | "debug" => Some(TaskCategory::Debugging),
            "security" | "sec" => Some(TaskCategory::Security),
            "configuration" | "config" => Some(TaskCategory::Configuration),
            "database" | "db" | "sql" => Some(TaskCategory::Database),
            "networking" | "network" | "net" => Some(TaskCategory::Networking),
            "containerization" | "container" | "docker" | "kubernetes" | "k8s" => {
                Some(TaskCategory::Containerization)
            }
            "file_manipulation" | "file-manipulation" | "file" | "files" => {
                Some(TaskCategory::FileManipulation)
            }
            "system_admin" | "system-admin" | "sysadmin" | "admin" => {
                Some(TaskCategory::SystemAdmin)
            }
            "other" => Some(TaskCategory::Other),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Configuration for the Analyzer Agent.
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Categories to consider during analysis.
    pub categories: Vec<TaskCategory>,
    /// Whether to estimate difficulty.
    pub difficulty_estimation: bool,
    /// Whether to extract required skills.
    pub skills_extraction: bool,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            categories: TaskCategory::all(),
            difficulty_estimation: true,
            skills_extraction: true,
            temperature: 0.3,
            max_tokens: 800,
        }
    }
}

impl AnalyzerConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the categories to consider.
    pub fn with_categories(mut self, categories: Vec<TaskCategory>) -> Self {
        self.categories = categories;
        self
    }

    /// Enables or disables difficulty estimation.
    pub fn with_difficulty_estimation(mut self, enabled: bool) -> Self {
        self.difficulty_estimation = enabled;
        self
    }

    /// Enables or disables skills extraction.
    pub fn with_skills_extraction(mut self, enabled: bool) -> Self {
        self.skills_extraction = enabled;
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

/// An analyzed task with extracted context and categorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedTask {
    /// The original collected task.
    pub original: CollectedTask,
    /// Primary category for the task.
    pub category: TaskCategory,
    /// Subcategory within the primary category.
    pub subcategory: String,
    /// Estimated difficulty level.
    pub difficulty: DifficultyLevel,
    /// Skills required to complete the task.
    pub required_skills: Vec<String>,
    /// Technical context explaining the problem domain.
    pub technical_context: String,
    /// Estimated time to complete in minutes.
    pub estimated_time_minutes: u32,
    /// Factors that contribute to task complexity.
    pub complexity_factors: Vec<String>,
}

impl AnalyzedTask {
    /// Creates a new analyzed task.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        original: CollectedTask,
        category: TaskCategory,
        subcategory: impl Into<String>,
        difficulty: DifficultyLevel,
        required_skills: Vec<String>,
        technical_context: impl Into<String>,
        estimated_time_minutes: u32,
        complexity_factors: Vec<String>,
    ) -> Self {
        Self {
            original,
            category,
            subcategory: subcategory.into(),
            difficulty,
            required_skills,
            technical_context: technical_context.into(),
            estimated_time_minutes,
            complexity_factors,
        }
    }

    /// Returns the task title.
    pub fn title(&self) -> &str {
        &self.original.title
    }

    /// Returns the task description.
    pub fn description(&self) -> &str {
        &self.original.description
    }

    /// Returns true if this task has code snippets.
    pub fn has_code(&self) -> bool {
        !self.original.code_snippets.is_empty()
    }

    /// Returns true if this task has a known solution.
    pub fn has_solution(&self) -> bool {
        self.original.solution.is_some()
    }
}

/// Analyzer Agent that extracts technical context and categorizes tasks.
///
/// This agent analyzes collected tasks to extract technical context,
/// categorize them, and estimate difficulty levels.
pub struct AnalyzerAgent {
    llm: Arc<dyn LlmProvider>,
}

impl std::fmt::Debug for AnalyzerAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyzerAgent").finish_non_exhaustive()
    }
}

impl AnalyzerAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "analyzer";

    /// Creates a new analyzer agent with the given LLM provider.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Analyzes a collected task to extract context and categorization.
    ///
    /// # Arguments
    ///
    /// * `task` - The collected task to analyze.
    /// * `config` - Configuration for the analysis.
    ///
    /// # Returns
    ///
    /// An `AnalyzedTask` with extracted context and categorization.
    pub async fn analyze(
        &self,
        task: &CollectedTask,
        config: &AnalyzerConfig,
    ) -> AgentResult<AnalyzedTask> {
        let prompt = self.build_analysis_prompt(task);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(ANALYSIS_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(config.temperature)
        .with_max_tokens(config.max_tokens);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_analysis_response(task.clone(), content, config)
    }

    /// Analyzes multiple tasks in batch.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The collected tasks to analyze.
    /// * `config` - Configuration for the analysis.
    ///
    /// # Returns
    ///
    /// A vector of analyzed tasks. Failed analyses are logged but not included.
    pub async fn analyze_batch(
        &self,
        tasks: &[CollectedTask],
        config: &AnalyzerConfig,
    ) -> AgentResult<Vec<AnalyzedTask>> {
        let mut analyzed_tasks = Vec::with_capacity(tasks.len());

        for task in tasks {
            match self.analyze(task, config).await {
                Ok(analyzed) => analyzed_tasks.push(analyzed),
                Err(e) => {
                    tracing::warn!("Failed to analyze task '{}': {}", task.title, e);
                }
            }
        }

        if analyzed_tasks.is_empty() && !tasks.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Failed to analyze any tasks".to_string(),
            ));
        }

        Ok(analyzed_tasks)
    }

    /// Builds the user prompt for task analysis.
    fn build_analysis_prompt(&self, task: &CollectedTask) -> String {
        let tags_str = if task.tags.is_empty() {
            "none".to_string()
        } else {
            task.tags.join(", ")
        };

        let has_code = if task.code_snippets.is_empty() {
            "No"
        } else {
            "Yes"
        };

        ANALYSIS_USER_TEMPLATE
            .replace("{source}", task.source.display_name())
            .replace("{title}", &task.title)
            .replace("{description}", &task.description)
            .replace("{tags}", &tags_str)
            .replace("{has_code}", has_code)
    }

    /// Parses the LLM response into an AnalyzedTask.
    fn parse_analysis_response(
        &self,
        task: CollectedTask,
        content: &str,
        config: &AnalyzerConfig,
    ) -> AgentResult<AnalyzedTask> {
        let json_content = self.extract_json(content)?;

        let parsed: AnalysisResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        // Parse category
        let category = TaskCategory::parse(&parsed.category).unwrap_or(TaskCategory::Other);

        // Validate category is in allowed list
        let final_category = if config.categories.contains(&category) {
            category
        } else {
            TaskCategory::Other
        };

        // Parse difficulty
        let difficulty = if config.difficulty_estimation {
            Self::parse_difficulty(&parsed.difficulty)?
        } else {
            DifficultyLevel::Medium
        };

        // Get skills
        let required_skills = if config.skills_extraction {
            parsed.required_skills
        } else {
            Vec::new()
        };

        Ok(AnalyzedTask::new(
            task,
            final_category,
            parsed.subcategory,
            difficulty,
            required_skills,
            parsed.technical_context,
            parsed.estimated_time_minutes,
            parsed.complexity_factors,
        ))
    }

    /// Parses a difficulty string into a DifficultyLevel.
    fn parse_difficulty(s: &str) -> AgentResult<DifficultyLevel> {
        match s.to_lowercase().trim() {
            "easy" => Ok(DifficultyLevel::Easy),
            "medium" => Ok(DifficultyLevel::Medium),
            "hard" => Ok(DifficultyLevel::Hard),
            other => Err(AgentError::InvalidDifficulty(format!(
                "Unknown difficulty '{}', expected easy/medium/hard",
                other
            ))),
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

/// Response structure from LLM analysis.
#[derive(Debug, Deserialize)]
struct AnalysisResponse {
    category: String,
    subcategory: String,
    difficulty: String,
    required_skills: Vec<String>,
    technical_context: String,
    estimated_time_minutes: u32,
    complexity_factors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::collector_agent::TaskSource;
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
                    completion_tokens: 100,
                    total_tokens: 200,
                },
            })
        }
    }

    #[test]
    fn test_task_category_all() {
        let categories = TaskCategory::all();
        assert_eq!(categories.len(), 9, "Should have 9 task categories");
    }

    #[test]
    fn test_task_category_display() {
        assert_eq!(TaskCategory::Debugging.display_name(), "Debugging");
        assert_eq!(TaskCategory::Security.display_name(), "Security");
        assert_eq!(
            TaskCategory::FileManipulation.display_name(),
            "File Manipulation"
        );
        assert_eq!(format!("{}", TaskCategory::Database), "Database");
    }

    #[test]
    fn test_task_category_from_str() {
        assert_eq!(
            TaskCategory::parse("debugging"),
            Some(TaskCategory::Debugging)
        );
        assert_eq!(
            TaskCategory::parse("SECURITY"),
            Some(TaskCategory::Security)
        );
        assert_eq!(
            TaskCategory::parse("docker"),
            Some(TaskCategory::Containerization)
        );
        assert_eq!(
            TaskCategory::parse("k8s"),
            Some(TaskCategory::Containerization)
        );
        assert_eq!(
            TaskCategory::parse("sysadmin"),
            Some(TaskCategory::SystemAdmin)
        );
        assert_eq!(TaskCategory::parse("invalid"), None);
    }

    #[test]
    fn test_analyzer_config_defaults() {
        let config = AnalyzerConfig::default();
        assert_eq!(config.categories.len(), 9);
        assert!(config.difficulty_estimation);
        assert!(config.skills_extraction);
    }

    #[test]
    fn test_analyzer_config_builder() {
        let config = AnalyzerConfig::new()
            .with_categories(vec![TaskCategory::Debugging, TaskCategory::Security])
            .with_difficulty_estimation(false)
            .with_skills_extraction(true)
            .with_temperature(0.5)
            .with_max_tokens(1000);

        assert_eq!(config.categories.len(), 2);
        assert!(!config.difficulty_estimation);
        assert!(config.skills_extraction);
        assert!((config.temperature - 0.5).abs() < 0.01);
        assert_eq!(config.max_tokens, 1000);
    }

    #[test]
    fn test_analyzed_task_creation() {
        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Debug memory leak",
            "Application memory grows over time...",
        )
        .with_code_snippets(vec!["fn main() {}".to_string()]);

        let analyzed = AnalyzedTask::new(
            task,
            TaskCategory::Debugging,
            "memory-debugging",
            DifficultyLevel::Hard,
            vec!["rust".to_string(), "profiling".to_string()],
            "Memory leak debugging requires understanding of heap allocation...",
            30,
            vec!["async code".to_string(), "hidden references".to_string()],
        );

        assert_eq!(analyzed.title(), "Debug memory leak");
        assert_eq!(analyzed.category, TaskCategory::Debugging);
        assert_eq!(analyzed.difficulty, DifficultyLevel::Hard);
        assert!(analyzed.has_code());
        assert!(!analyzed.has_solution());
        assert_eq!(analyzed.estimated_time_minutes, 30);
        assert_eq!(analyzed.complexity_factors.len(), 2);
    }

    #[tokio::test]
    async fn test_analyze_task_success() {
        let mock_response = r#"{
            "category": "debugging",
            "subcategory": "memory-debugging",
            "difficulty": "hard",
            "required_skills": ["rust", "memory profiling", "async patterns"],
            "technical_context": "Memory leak debugging in async Rust applications requires understanding of Arc, Weak references, and the tokio runtime.",
            "estimated_time_minutes": 45,
            "complexity_factors": ["async lifetime issues", "hidden strong references", "runtime-specific behavior"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = AnalyzerAgent::new(mock_provider);

        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Async memory leak in Rust",
            "My tokio application slowly consumes more memory...",
        )
        .with_tags(vec![
            "rust".to_string(),
            "tokio".to_string(),
            "memory".to_string(),
        ]);

        let config = AnalyzerConfig::default();
        let analyzed = agent.analyze(&task, &config).await.expect("should succeed");

        assert_eq!(analyzed.category, TaskCategory::Debugging);
        assert_eq!(analyzed.subcategory, "memory-debugging");
        assert_eq!(analyzed.difficulty, DifficultyLevel::Hard);
        assert_eq!(analyzed.required_skills.len(), 3);
        assert!(!analyzed.technical_context.is_empty());
        assert_eq!(analyzed.estimated_time_minutes, 45);
        assert_eq!(analyzed.complexity_factors.len(), 3);
    }

    #[tokio::test]
    async fn test_analyze_with_markdown_response() {
        let mock_response = r#"Here's my analysis:

```json
{
    "category": "security",
    "subcategory": "vulnerability-detection",
    "difficulty": "medium",
    "required_skills": ["web security", "SQL"],
    "technical_context": "SQL injection vulnerabilities allow attackers to manipulate database queries.",
    "estimated_time_minutes": 15,
    "complexity_factors": ["input validation", "parameterized queries"]
}
```

This is a common security issue."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = AnalyzerAgent::new(mock_provider);

        let task = CollectedTask::new(
            TaskSource::SecuritySources,
            "SQL Injection in Login",
            "The login form is vulnerable to SQL injection...",
        );

        let config = AnalyzerConfig::default();
        let analyzed = agent.analyze(&task, &config).await.expect("should succeed");

        assert_eq!(analyzed.category, TaskCategory::Security);
        assert_eq!(analyzed.difficulty, DifficultyLevel::Medium);
    }

    #[tokio::test]
    async fn test_analyze_filters_unknown_category() {
        let mock_response = r#"{
            "category": "unknown_category",
            "subcategory": "test",
            "difficulty": "easy",
            "required_skills": [],
            "technical_context": "Test context",
            "estimated_time_minutes": 5,
            "complexity_factors": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = AnalyzerAgent::new(mock_provider);

        let task = CollectedTask::new(TaskSource::Manual, "Test", "Test description");

        let config = AnalyzerConfig::default();
        let analyzed = agent.analyze(&task, &config).await.expect("should succeed");

        // Unknown category should default to Other
        assert_eq!(analyzed.category, TaskCategory::Other);
    }

    #[tokio::test]
    async fn test_analyze_respects_config_categories() {
        let mock_response = r#"{
            "category": "security",
            "subcategory": "test",
            "difficulty": "easy",
            "required_skills": [],
            "technical_context": "Test context",
            "estimated_time_minutes": 5,
            "complexity_factors": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = AnalyzerAgent::new(mock_provider);

        let task = CollectedTask::new(TaskSource::Manual, "Test", "Test description");

        // Config only allows Debugging category
        let config = AnalyzerConfig::new().with_categories(vec![TaskCategory::Debugging]);

        let analyzed = agent.analyze(&task, &config).await.expect("should succeed");

        // Security is not in allowed categories, should default to Other
        assert_eq!(analyzed.category, TaskCategory::Other);
    }

    #[tokio::test]
    async fn test_analyze_batch() {
        let mock_response = r#"{
            "category": "configuration",
            "subcategory": "service-setup",
            "difficulty": "easy",
            "required_skills": ["nginx"],
            "technical_context": "Web server configuration",
            "estimated_time_minutes": 10,
            "complexity_factors": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = AnalyzerAgent::new(mock_provider);

        let tasks = vec![
            CollectedTask::new(TaskSource::Manual, "Task 1", "Description 1"),
            CollectedTask::new(TaskSource::Manual, "Task 2", "Description 2"),
        ];

        let config = AnalyzerConfig::default();
        let analyzed = agent
            .analyze_batch(&tasks, &config)
            .await
            .expect("should succeed");

        assert_eq!(analyzed.len(), 2);
    }

    #[test]
    fn test_parse_difficulty() {
        assert_eq!(
            AnalyzerAgent::parse_difficulty("easy").unwrap(),
            DifficultyLevel::Easy
        );
        assert_eq!(
            AnalyzerAgent::parse_difficulty("MEDIUM").unwrap(),
            DifficultyLevel::Medium
        );
        assert_eq!(
            AnalyzerAgent::parse_difficulty("  hard  ").unwrap(),
            DifficultyLevel::Hard
        );
        assert!(AnalyzerAgent::parse_difficulty("invalid").is_err());
    }

    #[test]
    fn test_agent_name_constant() {
        assert_eq!(AnalyzerAgent::AGENT_NAME, "analyzer");
    }
}
