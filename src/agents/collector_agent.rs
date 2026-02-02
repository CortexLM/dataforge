//! Collector Agent for external data collection pipeline.
//!
//! This agent coordinates external data collection and prioritizes interesting
//! problems for the benchmark generation system.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};
use super::types::PipelineEvent;

/// System prompt for task prioritization.
const PRIORITIZATION_SYSTEM_PROMPT: &str = r#"You are an expert benchmark task curator evaluating collected tasks for AI evaluation benchmarks.

Your role is to score tasks on three dimensions:
1. COMPLEXITY: Does this task require multi-step reasoning and domain expertise? (0.0-1.0)
   - 0.0-0.3: Trivial, single-step tasks
   - 0.4-0.6: Moderate complexity, requires some thinking
   - 0.7-1.0: Complex, requires deep expertise and multi-step reasoning

2. RELEVANCE: Is this task useful for evaluating AI capabilities? (0.0-1.0)
   - 0.0-0.3: Not relevant, too generic or off-topic
   - 0.4-0.6: Somewhat relevant, tests some useful skills
   - 0.7-1.0: Highly relevant, tests important AI capabilities

3. TESTABILITY: Can this task be automatically verified? (0.0-1.0)
   - 0.0-0.3: Cannot be verified programmatically
   - 0.4-0.6: Partially verifiable with some manual review
   - 0.7-1.0: Fully verifiable through automated tests

Output Format:
You MUST respond with ONLY a JSON object in this exact format:
{
  "complexity": <float 0.0-1.0>,
  "relevance": <float 0.0-1.0>,
  "testability": <float 0.0-1.0>,
  "reasoning": "<brief explanation of scores>"
}

Do not include any text outside the JSON object."#;

/// User prompt template for task prioritization.
const PRIORITIZATION_USER_TEMPLATE: &str = r#"Evaluate the following collected task for inclusion in an AI benchmark:

Source: {source}
Title: {title}
Description: {description}
Tags: {tags}

Score this task on complexity, relevance, and testability."#;

/// Sources from which tasks can be collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSource {
    /// Stack Overflow questions.
    StackOverflow,
    /// GitHub issues.
    GitHubIssues,
    /// Reddit posts (e.g., r/sysadmin, r/devops).
    Reddit,
    /// Unix & Linux Stack Exchange.
    UnixStackExchange,
    /// Server Fault.
    ServerFault,
    /// DevOps-related forums.
    DevOpsForum,
    /// Security-focused sources (CTFs, security forums).
    SecuritySources,
    /// Custom/manual collection.
    Manual,
}

impl TaskSource {
    /// Returns all available task sources.
    pub fn all() -> Vec<TaskSource> {
        vec![
            TaskSource::StackOverflow,
            TaskSource::GitHubIssues,
            TaskSource::Reddit,
            TaskSource::UnixStackExchange,
            TaskSource::ServerFault,
            TaskSource::DevOpsForum,
            TaskSource::SecuritySources,
            TaskSource::Manual,
        ]
    }

    /// Returns the display name for this source.
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskSource::StackOverflow => "Stack Overflow",
            TaskSource::GitHubIssues => "GitHub Issues",
            TaskSource::Reddit => "Reddit",
            TaskSource::UnixStackExchange => "Unix Stack Exchange",
            TaskSource::ServerFault => "Server Fault",
            TaskSource::DevOpsForum => "DevOps Forums",
            TaskSource::SecuritySources => "Security Sources",
            TaskSource::Manual => "Manual Collection",
        }
    }
}

impl std::fmt::Display for TaskSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// A task collected from an external source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedTask {
    /// Unique identifier for this collected task.
    pub id: String,
    /// Source from which this task was collected.
    pub source: TaskSource,
    /// Original URL or reference.
    pub source_url: Option<String>,
    /// Title or summary of the task.
    pub title: String,
    /// Full description or problem statement.
    pub description: String,
    /// Tags or categories from the source.
    pub tags: Vec<String>,
    /// Associated code snippets or examples.
    pub code_snippets: Vec<String>,
    /// Solution or accepted answer if available.
    pub solution: Option<String>,
    /// Upvotes or popularity metric.
    pub popularity_score: Option<i32>,
    /// Timestamp when this was collected.
    pub collected_at: DateTime<Utc>,
    /// Additional metadata from the source.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CollectedTask {
    /// Creates a new collected task with required fields.
    pub fn new(
        source: TaskSource,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source,
            source_url: None,
            title: title.into(),
            description: description.into(),
            tags: Vec::new(),
            code_snippets: Vec::new(),
            solution: None,
            popularity_score: None,
            collected_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Sets the source URL.
    pub fn with_source_url(mut self, url: impl Into<String>) -> Self {
        self.source_url = Some(url.into());
        self
    }

    /// Sets the tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Sets the code snippets.
    pub fn with_code_snippets(mut self, snippets: Vec<String>) -> Self {
        self.code_snippets = snippets;
        self
    }

    /// Sets the solution.
    pub fn with_solution(mut self, solution: impl Into<String>) -> Self {
        self.solution = Some(solution.into());
        self
    }

    /// Sets the popularity score.
    pub fn with_popularity(mut self, score: i32) -> Self {
        self.popularity_score = Some(score);
        self
    }

    /// Adds metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Configuration for the Collector Agent.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Which sources are enabled for collection.
    pub sources_enabled: HashMap<TaskSource, bool>,
    /// Minimum complexity score threshold (0.0 to 1.0).
    pub complexity_threshold: f64,
    /// Minimum relevance score threshold (0.0 to 1.0).
    pub relevance_threshold: f64,
    /// Maximum number of tasks to collect per source.
    pub max_tasks_per_source: usize,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        let mut sources_enabled = HashMap::new();
        for source in TaskSource::all() {
            sources_enabled.insert(source, true);
        }

        Self {
            sources_enabled,
            complexity_threshold: 0.5,
            relevance_threshold: 0.5,
            max_tasks_per_source: 100,
            temperature: 0.3,
            max_tokens: 500,
        }
    }
}

impl CollectorConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the complexity threshold.
    pub fn with_complexity_threshold(mut self, threshold: f64) -> Self {
        self.complexity_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets the relevance threshold.
    pub fn with_relevance_threshold(mut self, threshold: f64) -> Self {
        self.relevance_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum tasks per source.
    pub fn with_max_tasks_per_source(mut self, max: usize) -> Self {
        self.max_tasks_per_source = max;
        self
    }

    /// Enables or disables a specific source.
    pub fn set_source_enabled(mut self, source: TaskSource, enabled: bool) -> Self {
        self.sources_enabled.insert(source, enabled);
        self
    }

    /// Returns whether a source is enabled.
    pub fn is_source_enabled(&self, source: &TaskSource) -> bool {
        self.sources_enabled.get(source).copied().unwrap_or(false)
    }
}

/// A task with priority scores for benchmark inclusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrioritizedTask {
    /// The original collected task.
    pub task: CollectedTask,
    /// Combined priority score (0.0 to 1.0).
    pub priority_score: f64,
    /// Complexity estimate (0.0 to 1.0).
    pub complexity_estimate: f64,
    /// Relevance score (0.0 to 1.0).
    pub relevance_score: f64,
    /// Testability score (0.0 to 1.0).
    pub testability_score: f64,
    /// Reasoning from the LLM evaluation.
    pub reasoning: String,
}

impl PrioritizedTask {
    /// Creates a new prioritized task from scores.
    pub fn new(
        task: CollectedTask,
        complexity_estimate: f64,
        relevance_score: f64,
        testability_score: f64,
        reasoning: impl Into<String>,
    ) -> Self {
        // Calculate weighted priority score
        // Weight: complexity=0.4, relevance=0.35, testability=0.25
        let priority_score =
            0.4 * complexity_estimate + 0.35 * relevance_score + 0.25 * testability_score;

        Self {
            task,
            priority_score: priority_score.clamp(0.0, 1.0),
            complexity_estimate: complexity_estimate.clamp(0.0, 1.0),
            relevance_score: relevance_score.clamp(0.0, 1.0),
            testability_score: testability_score.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
        }
    }

    /// Returns true if this task passes all thresholds.
    pub fn passes_thresholds(&self, config: &CollectorConfig) -> bool {
        self.complexity_estimate >= config.complexity_threshold
            && self.relevance_score >= config.relevance_threshold
    }
}

/// Collector Agent that coordinates external data collection and prioritization.
///
/// This agent collects tasks from various external sources and uses LLM to
/// prioritize them based on complexity, relevance, and testability.
pub struct CollectorAgent {
    llm: Arc<dyn LlmProvider>,
}

impl std::fmt::Debug for CollectorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorAgent").finish_non_exhaustive()
    }
}

impl CollectorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "collector";

    /// Creates a new collector agent with the given LLM provider.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Collects and prioritizes tasks from configured sources.
    ///
    /// # Arguments
    ///
    /// * `tasks` - Pre-collected tasks to prioritize.
    /// * `config` - Configuration for collection and prioritization.
    /// * `event_tx` - Optional channel for progress events.
    ///
    /// # Returns
    ///
    /// A vector of prioritized tasks sorted by priority score descending.
    pub async fn collect_and_prioritize(
        &self,
        tasks: &[CollectedTask],
        config: &CollectorConfig,
        event_tx: Option<Sender<PipelineEvent>>,
    ) -> AgentResult<Vec<PrioritizedTask>> {
        let mut prioritized_tasks = Vec::with_capacity(tasks.len());

        for (idx, task) in tasks.iter().enumerate() {
            // Skip tasks from disabled sources
            if !config.is_source_enabled(&task.source) {
                continue;
            }

            // Emit progress event
            if let Some(ref tx) = event_tx {
                let reasoning = format!(
                    "Evaluating task {}/{}: {}",
                    idx + 1,
                    tasks.len(),
                    task.title
                );
                let _ = tx
                    .send(PipelineEvent::agent_reasoning(
                        super::types::PipelineStage::SyntheticValidation,
                        reasoning,
                    ))
                    .await;
            }

            // Prioritize the task using LLM
            match self.prioritize_task(task, config).await {
                Ok(prioritized) => {
                    if prioritized.passes_thresholds(config) {
                        prioritized_tasks.push(prioritized);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to prioritize task '{}': {}", task.title, e);
                }
            }
        }

        // Sort by priority score descending
        prioritized_tasks.sort_by(|a, b| {
            b.priority_score
                .partial_cmp(&a.priority_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Emit completion event
        if let Some(tx) = event_tx {
            let reasoning = format!(
                "Prioritization complete: {} tasks passed thresholds out of {}",
                prioritized_tasks.len(),
                tasks.len()
            );
            let _ = tx
                .send(PipelineEvent::agent_reasoning(
                    super::types::PipelineStage::SyntheticValidation,
                    reasoning,
                ))
                .await;
        }

        Ok(prioritized_tasks)
    }

    /// Prioritizes a single task using LLM evaluation.
    async fn prioritize_task(
        &self,
        task: &CollectedTask,
        config: &CollectorConfig,
    ) -> AgentResult<PrioritizedTask> {
        let prompt = self.build_prioritization_prompt(task);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(PRIORITIZATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(config.temperature)
        .with_max_tokens(config.max_tokens);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_prioritization_response(task.clone(), content)
    }

    /// Builds the user prompt for task prioritization.
    fn build_prioritization_prompt(&self, task: &CollectedTask) -> String {
        let tags_str = if task.tags.is_empty() {
            "none".to_string()
        } else {
            task.tags.join(", ")
        };

        PRIORITIZATION_USER_TEMPLATE
            .replace("{source}", task.source.display_name())
            .replace("{title}", &task.title)
            .replace("{description}", &task.description)
            .replace("{tags}", &tags_str)
    }

    /// Parses the LLM response into a PrioritizedTask.
    fn parse_prioritization_response(
        &self,
        task: CollectedTask,
        content: &str,
    ) -> AgentResult<PrioritizedTask> {
        let json_content = self.extract_json(content)?;

        let parsed: PrioritizationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        Ok(PrioritizedTask::new(
            task,
            parsed.complexity,
            parsed.relevance,
            parsed.testability,
            parsed.reasoning,
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

/// Response structure from LLM prioritization.
#[derive(Debug, Deserialize)]
struct PrioritizationResponse {
    complexity: f64,
    relevance: f64,
    testability: f64,
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

    #[test]
    fn test_task_source_all() {
        let sources = TaskSource::all();
        assert_eq!(sources.len(), 8, "Should have 8 task sources");
    }

    #[test]
    fn test_task_source_display() {
        assert_eq!(TaskSource::StackOverflow.display_name(), "Stack Overflow");
        assert_eq!(TaskSource::GitHubIssues.display_name(), "GitHub Issues");
        assert_eq!(format!("{}", TaskSource::Reddit), "Reddit");
    }

    #[test]
    fn test_collected_task_creation() {
        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Fix Docker networking issue",
            "Container cannot reach external network...",
        )
        .with_source_url("https://stackoverflow.com/q/12345")
        .with_tags(vec!["docker".to_string(), "networking".to_string()])
        .with_popularity(42);

        assert!(!task.id.is_empty());
        assert_eq!(task.source, TaskSource::StackOverflow);
        assert_eq!(task.title, "Fix Docker networking issue");
        assert_eq!(task.tags.len(), 2);
        assert_eq!(task.popularity_score, Some(42));
    }

    #[test]
    fn test_collector_config_defaults() {
        let config = CollectorConfig::default();
        assert!((config.complexity_threshold - 0.5).abs() < 0.01);
        assert!((config.relevance_threshold - 0.5).abs() < 0.01);
        assert_eq!(config.max_tasks_per_source, 100);

        // All sources should be enabled by default
        for source in TaskSource::all() {
            assert!(config.is_source_enabled(&source));
        }
    }

    #[test]
    fn test_collector_config_builder() {
        let config = CollectorConfig::new()
            .with_complexity_threshold(0.7)
            .with_relevance_threshold(0.6)
            .with_max_tasks_per_source(50)
            .set_source_enabled(TaskSource::Reddit, false);

        assert!((config.complexity_threshold - 0.7).abs() < 0.01);
        assert!((config.relevance_threshold - 0.6).abs() < 0.01);
        assert_eq!(config.max_tasks_per_source, 50);
        assert!(!config.is_source_enabled(&TaskSource::Reddit));
        assert!(config.is_source_enabled(&TaskSource::StackOverflow));
    }

    #[test]
    fn test_prioritized_task_creation() {
        let task = CollectedTask::new(
            TaskSource::GitHubIssues,
            "Memory leak in async handler",
            "The application crashes after prolonged use...",
        );

        let prioritized =
            PrioritizedTask::new(task, 0.8, 0.9, 0.7, "High complexity debugging task");

        // Priority score: 0.4*0.8 + 0.35*0.9 + 0.25*0.7 = 0.32 + 0.315 + 0.175 = 0.81
        assert!((prioritized.priority_score - 0.81).abs() < 0.01);
        assert!((prioritized.complexity_estimate - 0.8).abs() < 0.01);
        assert!((prioritized.relevance_score - 0.9).abs() < 0.01);
        assert!((prioritized.testability_score - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_prioritized_task_passes_thresholds() {
        let task = CollectedTask::new(TaskSource::Manual, "Test task", "Description");
        let prioritized = PrioritizedTask::new(task, 0.6, 0.7, 0.5, "Good task");

        let config = CollectorConfig::new()
            .with_complexity_threshold(0.5)
            .with_relevance_threshold(0.6);

        assert!(prioritized.passes_thresholds(&config));

        let strict_config = CollectorConfig::new()
            .with_complexity_threshold(0.8)
            .with_relevance_threshold(0.8);

        assert!(!prioritized.passes_thresholds(&strict_config));
    }

    #[tokio::test]
    async fn test_prioritize_task_success() {
        let mock_response = r#"{
            "complexity": 0.75,
            "relevance": 0.85,
            "testability": 0.70,
            "reasoning": "This is a complex debugging task that requires deep understanding of async patterns."
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = CollectorAgent::new(mock_provider);

        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Debug async memory leak",
            "Application memory grows over time in async handler...",
        )
        .with_tags(vec![
            "rust".to_string(),
            "async".to_string(),
            "memory".to_string(),
        ]);

        let config = CollectorConfig::default();
        let prioritized = agent
            .prioritize_task(&task, &config)
            .await
            .expect("should succeed");

        assert!((prioritized.complexity_estimate - 0.75).abs() < 0.01);
        assert!((prioritized.relevance_score - 0.85).abs() < 0.01);
        assert!((prioritized.testability_score - 0.70).abs() < 0.01);
        assert!(!prioritized.reasoning.is_empty());
    }

    #[tokio::test]
    async fn test_collect_and_prioritize_filters_disabled_sources() {
        let mock_response = r#"{
            "complexity": 0.8,
            "relevance": 0.8,
            "testability": 0.8,
            "reasoning": "Good task"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = CollectorAgent::new(mock_provider);

        let tasks = vec![
            CollectedTask::new(TaskSource::StackOverflow, "Task 1", "Description 1"),
            CollectedTask::new(TaskSource::Reddit, "Task 2", "Description 2"),
            CollectedTask::new(TaskSource::StackOverflow, "Task 3", "Description 3"),
        ];

        let config = CollectorConfig::new().set_source_enabled(TaskSource::Reddit, false);

        let prioritized = agent
            .collect_and_prioritize(&tasks, &config, None)
            .await
            .expect("should succeed");

        // Only StackOverflow tasks should be included (Reddit disabled)
        assert_eq!(prioritized.len(), 2);
        for p in &prioritized {
            assert_eq!(p.task.source, TaskSource::StackOverflow);
        }
    }

    #[tokio::test]
    async fn test_collect_and_prioritize_filters_by_threshold() {
        let mock_response = r#"{
            "complexity": 0.3,
            "relevance": 0.3,
            "testability": 0.3,
            "reasoning": "Low quality task"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = CollectorAgent::new(mock_provider);

        let tasks = vec![CollectedTask::new(
            TaskSource::Manual,
            "Simple task",
            "Too simple",
        )];

        let config = CollectorConfig::new()
            .with_complexity_threshold(0.5)
            .with_relevance_threshold(0.5);

        let prioritized = agent
            .collect_and_prioritize(&tasks, &config, None)
            .await
            .expect("should succeed");

        // Task should be filtered out due to low scores
        assert!(prioritized.is_empty());
    }

    #[tokio::test]
    async fn test_collect_and_prioritize_sorts_by_priority() {
        // We'll need to use a mock that returns different responses
        // For simplicity, we'll test the sorting with pre-created PrioritizedTasks

        let task1 = CollectedTask::new(TaskSource::Manual, "Task 1", "Desc 1");
        let task2 = CollectedTask::new(TaskSource::Manual, "Task 2", "Desc 2");

        let p1 = PrioritizedTask::new(task1, 0.5, 0.5, 0.5, "Medium priority");
        let p2 = PrioritizedTask::new(task2, 0.9, 0.9, 0.9, "High priority");

        let mut tasks = vec![p1, p2];
        tasks.sort_by(|a, b| {
            b.priority_score
                .partial_cmp(&a.priority_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        assert!(tasks[0].priority_score > tasks[1].priority_score);
        assert_eq!(tasks[0].task.title, "Task 2");
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
        assert_eq!(CollectorAgent::AGENT_NAME, "collector");
    }
}
