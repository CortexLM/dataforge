//! Task Ideator Agent for creative benchmark task generation.
//!
//! This agent uses HIGH TEMPERATURE LLM calls to generate diverse, creative task ideas
//! for the synthetic benchmark generation system. It produces novel task concepts that
//! challenge AI problem-solving capabilities.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::difficulty::DifficultyLevel;
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};

/// System prompt for creative task ideation.
const IDEATION_SYSTEM_PROMPT: &str = r#"You are a creative benchmark task designer generating CHALLENGING tasks for testing AI capabilities.

Requirements for generated tasks:
1. Tasks must require MULTI-STEP reasoning (5+ distinct steps)
2. Tasks must require DOMAIN EXPERTISE, not just pattern matching
3. Tasks must be SOLVABLE but with NO OBVIOUS ANSWER
4. Tasks should take an experienced human 10-30 minutes
5. Tasks must NOT have answers that can be memorized or looked up
6. Include specific anti-patterns to avoid in solutions

Your goal is to push the limits of AI problem-solving while ensuring tasks remain feasible for skilled practitioners."#;

/// User prompt template for task ideation.
const IDEATION_USER_TEMPLATE: &str = r#"Generate a creative, challenging task for the following category.

Category: {category}

The task should:
- Require deep domain expertise in {category}
- Have multiple valid solution approaches
- Not be solvable by simple memorization
- Challenge both reasoning and practical skills
- Take 10-30 minutes for an experienced professional

Output as JSON:
{
  "title": "Brief descriptive title (max 80 characters)",
  "description": "Detailed task description with context, constraints, and objectives. Be specific about the scenario, what files or systems are involved, and what success looks like.",
  "estimated_difficulty": "easy|medium|hard",
  "required_skills": ["skill1", "skill2", "skill3"],
  "anti_patterns": ["approach_to_avoid1", "approach_to_avoid2"]
}

IMPORTANT: Output ONLY the JSON object, no additional text."#;

/// Categories for task ideation that map to benchmark domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskCategory {
    /// Novel algorithm design and implementation challenges.
    AlgorithmDesign,
    /// Complex multi-service debugging scenarios.
    SystemDebugging,
    /// Vulnerability hunting and security analysis.
    SecurityAnalysis,
    /// Deployment, scaling, and infrastructure challenges.
    Infrastructure,
    /// Data pipeline design and transformations.
    DataEngineering,
    /// Binary analysis and protocol decoding.
    ReverseEngineering,
    /// Profiling and bottleneck identification.
    PerformanceOptimization,
    /// Multi-system orchestration tasks.
    IntegrationTasks,
    /// General debugging (maps to existing Debugging).
    Debugging,
    /// Security hardening and audit (maps to existing Security).
    Security,
    /// System admin tasks (maps to existing SystemAdministration).
    SystemAdministration,
    /// Code and build tasks (maps to existing SoftwareEngineering).
    SoftwareEngineering,
    /// File manipulation tasks (maps to existing FileOperations).
    FileOperations,
    /// Data analysis tasks (maps to existing DataScience).
    DataScience,
    /// Network configuration and diagnostics (maps to existing Networking).
    Networking,
    /// Container orchestration (maps to existing Containers).
    Containers,
}

impl TaskCategory {
    /// Returns all available task categories.
    pub fn all() -> Vec<TaskCategory> {
        vec![
            TaskCategory::AlgorithmDesign,
            TaskCategory::SystemDebugging,
            TaskCategory::SecurityAnalysis,
            TaskCategory::Infrastructure,
            TaskCategory::DataEngineering,
            TaskCategory::ReverseEngineering,
            TaskCategory::PerformanceOptimization,
            TaskCategory::IntegrationTasks,
            TaskCategory::Debugging,
            TaskCategory::Security,
            TaskCategory::SystemAdministration,
            TaskCategory::SoftwareEngineering,
            TaskCategory::FileOperations,
            TaskCategory::DataScience,
            TaskCategory::Networking,
            TaskCategory::Containers,
        ]
    }

    /// Returns the display name for this category.
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskCategory::AlgorithmDesign => "Algorithm Design",
            TaskCategory::SystemDebugging => "System Debugging",
            TaskCategory::SecurityAnalysis => "Security Analysis",
            TaskCategory::Infrastructure => "Infrastructure",
            TaskCategory::DataEngineering => "Data Engineering",
            TaskCategory::ReverseEngineering => "Reverse Engineering",
            TaskCategory::PerformanceOptimization => "Performance Optimization",
            TaskCategory::IntegrationTasks => "Integration Tasks",
            TaskCategory::Debugging => "Debugging",
            TaskCategory::Security => "Security",
            TaskCategory::SystemAdministration => "System Administration",
            TaskCategory::SoftwareEngineering => "Software Engineering",
            TaskCategory::FileOperations => "File Operations",
            TaskCategory::DataScience => "Data Science",
            TaskCategory::Networking => "Networking",
            TaskCategory::Containers => "Containers",
        }
    }

    /// Returns a description of what this category tests.
    pub fn description(&self) -> &'static str {
        match self {
            TaskCategory::AlgorithmDesign => {
                "Novel algorithm challenges requiring creative problem-solving and optimization"
            }
            TaskCategory::SystemDebugging => {
                "Complex multi-service debugging with distributed system issues"
            }
            TaskCategory::SecurityAnalysis => {
                "Vulnerability hunting, exploit analysis, and security assessment"
            }
            TaskCategory::Infrastructure => {
                "Deployment, scaling, and infrastructure automation challenges"
            }
            TaskCategory::DataEngineering => {
                "Data pipeline design, ETL transformations, and data quality tasks"
            }
            TaskCategory::ReverseEngineering => {
                "Binary analysis, protocol decoding, and system reverse engineering"
            }
            TaskCategory::PerformanceOptimization => {
                "Profiling, bottleneck identification, and performance tuning"
            }
            TaskCategory::IntegrationTasks => {
                "Multi-system orchestration and API integration challenges"
            }
            TaskCategory::Debugging => "Log analysis, error fixing, and crash investigation",
            TaskCategory::Security => "Security hardening, CTF challenges, and incident response",
            TaskCategory::SystemAdministration => {
                "Service configuration, user management, and system operations"
            }
            TaskCategory::SoftwareEngineering => {
                "Build systems, version control, and code refactoring"
            }
            TaskCategory::FileOperations => {
                "Text processing, search-replace, and file organization"
            }
            TaskCategory::DataScience => "Data wrangling, analysis, and ML workflow tasks",
            TaskCategory::Networking => "DNS, firewall, proxy, and network diagnostics",
            TaskCategory::Containers => "Docker operations, compose, and Kubernetes tasks",
        }
    }

    /// Maps this ideator category to existing benchmark categories.
    pub fn to_benchmark_category(&self) -> &'static str {
        match self {
            TaskCategory::AlgorithmDesign => "software-engineering",
            TaskCategory::SystemDebugging => "debugging",
            TaskCategory::SecurityAnalysis => "security",
            TaskCategory::Infrastructure => "system-administration",
            TaskCategory::DataEngineering => "data-science",
            TaskCategory::ReverseEngineering => "security",
            TaskCategory::PerformanceOptimization => "debugging",
            TaskCategory::IntegrationTasks => "software-engineering",
            TaskCategory::Debugging => "debugging",
            TaskCategory::Security => "security",
            TaskCategory::SystemAdministration => "system-administration",
            TaskCategory::SoftwareEngineering => "software-engineering",
            TaskCategory::FileOperations => "file-operations",
            TaskCategory::DataScience => "data-science",
            TaskCategory::Networking => "networking",
            TaskCategory::Containers => "containers",
        }
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// A generated task idea from the ideator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdea {
    /// Unique identifier for this idea.
    pub id: String,
    /// The category this task belongs to.
    pub category: TaskCategory,
    /// Subcategory within the main category.
    pub subcategory: String,
    /// Brief title for the task.
    pub title: String,
    /// Detailed description of the task.
    pub description: String,
    /// Estimated difficulty level.
    pub estimated_difficulty: DifficultyLevel,
    /// Skills required to complete this task.
    pub required_skills: Vec<String>,
    /// Approaches that should NOT be used (anti-patterns).
    pub anti_patterns: Vec<String>,
    /// Timestamp when this idea was created.
    pub created_at: DateTime<Utc>,
}

impl TaskIdea {
    /// Creates a new TaskIdea with all required fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        category: TaskCategory,
        subcategory: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        estimated_difficulty: DifficultyLevel,
        required_skills: Vec<String>,
        anti_patterns: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            category,
            subcategory: subcategory.into(),
            title: title.into(),
            description: description.into(),
            estimated_difficulty,
            required_skills,
            anti_patterns,
            created_at: Utc::now(),
        }
    }

    /// Returns the benchmark category string for this idea.
    pub fn benchmark_category(&self) -> &'static str {
        self.category.to_benchmark_category()
    }
}

/// Configuration for the Ideator Agent.
#[derive(Debug, Clone)]
pub struct IdeatorConfig {
    /// Temperature for LLM generation (0.9-1.2 for high creativity).
    pub temperature: f64,
    /// Nucleus sampling parameter.
    pub top_p: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for IdeatorConfig {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            max_tokens: 2000,
        }
    }
}

impl IdeatorConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the temperature (clamped to 0.9-1.2 for high creativity).
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.9, 1.2);
        self
    }

    /// Sets the top_p parameter.
    pub fn with_top_p(mut self, top_p: f64) -> Self {
        self.top_p = top_p.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum tokens for responses.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Task Ideator Agent that generates creative benchmark task ideas.
///
/// This agent uses high-temperature LLM calls to produce diverse, challenging
/// task concepts that push the boundaries of AI problem-solving.
pub struct IdeatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: IdeatorConfig,
}

impl std::fmt::Debug for IdeatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdeatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl IdeatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "task_ideator";

    /// Creates a new ideator agent with the given LLM client and configuration.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: IdeatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new ideator agent with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, IdeatorConfig::default())
    }

    /// Generates a single creative task idea.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to focus on. If None, a random category is used.
    ///
    /// # Returns
    ///
    /// A `TaskIdea` containing the generated task concept.
    pub async fn generate_task_idea(
        &self,
        category: Option<TaskCategory>,
    ) -> AgentResult<TaskIdea> {
        let selected_category = category.unwrap_or_else(|| {
            // Select a pseudo-random category based on current timestamp
            let categories = TaskCategory::all();
            let index = (Utc::now().timestamp_millis() as usize) % categories.len();
            categories[index]
        });

        let prompt = self.build_ideation_prompt(selected_category);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(IDEATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens)
        .with_top_p(self.config.top_p);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_idea_response(content, selected_category)
    }

    /// Generates a batch of task ideas.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of ideas to generate.
    /// * `categories` - Optional list of categories to use. If None, categories are cycled.
    ///
    /// # Returns
    ///
    /// A vector of `TaskIdea` instances.
    pub async fn generate_batch(
        &self,
        count: usize,
        categories: Option<Vec<TaskCategory>>,
    ) -> AgentResult<Vec<TaskIdea>> {
        let mut ideas = Vec::with_capacity(count);
        let available_categories = categories.unwrap_or_else(TaskCategory::all);

        for i in 0..count {
            let category_index = i % available_categories.len();
            let category = available_categories[category_index];

            match self.generate_task_idea(Some(category)).await {
                Ok(idea) => ideas.push(idea),
                Err(e) => {
                    // Log error but continue generating other ideas
                    tracing::warn!(
                        "Failed to generate idea {} for category {:?}: {}",
                        i,
                        category,
                        e
                    );
                }
            }
        }

        if ideas.is_empty() && count > 0 {
            return Err(AgentError::GenerationFailed(
                "Failed to generate any task ideas".to_string(),
            ));
        }

        Ok(ideas)
    }

    /// Builds the user prompt for task ideation.
    fn build_ideation_prompt(&self, category: TaskCategory) -> String {
        IDEATION_USER_TEMPLATE.replace("{category}", category.display_name())
    }

    /// Parses the LLM response into a TaskIdea.
    fn parse_idea_response(&self, content: &str, category: TaskCategory) -> AgentResult<TaskIdea> {
        let json_content = self.extract_json(content)?;

        let parsed: IdeaResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let difficulty = Self::parse_difficulty(&parsed.estimated_difficulty)?;

        // Generate subcategory from the task domain
        let subcategory = Self::infer_subcategory(&parsed.title, &parsed.description, category);

        Ok(TaskIdea::new(
            category,
            subcategory,
            parsed.title,
            parsed.description,
            difficulty,
            parsed.required_skills,
            parsed.anti_patterns,
        ))
    }

    /// Extracts JSON from the response, handling potential markdown code blocks.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        let trimmed = content.trim();

        // If it already starts with '{', use as-is
        if trimmed.starts_with('{') {
            // Find the matching closing brace
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

    /// Infers a subcategory based on the task title and description.
    fn infer_subcategory(title: &str, description: &str, category: TaskCategory) -> String {
        let combined = format!("{} {}", title.to_lowercase(), description.to_lowercase());

        // Match keywords to subcategories based on category
        match category {
            TaskCategory::AlgorithmDesign | TaskCategory::SoftwareEngineering => {
                if combined.contains("optimization") || combined.contains("performance") {
                    "optimization".to_string()
                } else if combined.contains("graph") || combined.contains("tree") {
                    "graph-algorithms".to_string()
                } else if combined.contains("concurrent") || combined.contains("parallel") {
                    "concurrency".to_string()
                } else if combined.contains("build") || combined.contains("compile") {
                    "build-systems".to_string()
                } else {
                    "general".to_string()
                }
            }
            TaskCategory::SystemDebugging | TaskCategory::Debugging => {
                if combined.contains("log") {
                    "log-analysis".to_string()
                } else if combined.contains("crash") || combined.contains("segfault") {
                    "crash-investigation".to_string()
                } else if combined.contains("memory") || combined.contains("leak") {
                    "memory-debugging".to_string()
                } else if combined.contains("performance") {
                    "performance-debugging".to_string()
                } else {
                    "error-fixing".to_string()
                }
            }
            TaskCategory::SecurityAnalysis | TaskCategory::Security => {
                if combined.contains("vulnerab") || combined.contains("exploit") {
                    "vulnerability-detection".to_string()
                } else if combined.contains("ctf") || combined.contains("challenge") {
                    "ctf-challenges".to_string()
                } else if combined.contains("harden") || combined.contains("secure") {
                    "hardening".to_string()
                } else if combined.contains("incident") || combined.contains("breach") {
                    "incident-response".to_string()
                } else {
                    "audit".to_string()
                }
            }
            TaskCategory::Infrastructure | TaskCategory::SystemAdministration => {
                if combined.contains("deploy") || combined.contains("provision") {
                    "deployment".to_string()
                } else if combined.contains("scale") || combined.contains("load") {
                    "scaling".to_string()
                } else if combined.contains("service") || combined.contains("daemon") {
                    "service-configuration".to_string()
                } else if combined.contains("user") || combined.contains("permission") {
                    "user-management".to_string()
                } else {
                    "automation".to_string()
                }
            }
            TaskCategory::DataEngineering | TaskCategory::DataScience => {
                if combined.contains("pipeline") || combined.contains("etl") {
                    "data-pipelines".to_string()
                } else if combined.contains("transform") || combined.contains("clean") {
                    "data-wrangling".to_string()
                } else if combined.contains("analysis") || combined.contains("insight") {
                    "analysis".to_string()
                } else if combined.contains("ml") || combined.contains("model") {
                    "ml-workflows".to_string()
                } else {
                    "data-processing".to_string()
                }
            }
            TaskCategory::ReverseEngineering => {
                if combined.contains("binary") || combined.contains("disassembl") {
                    "binary-analysis".to_string()
                } else if combined.contains("protocol") || combined.contains("packet") {
                    "protocol-analysis".to_string()
                } else if combined.contains("malware") {
                    "malware-analysis".to_string()
                } else {
                    "reverse-engineering".to_string()
                }
            }
            TaskCategory::PerformanceOptimization => {
                if combined.contains("profil") {
                    "profiling".to_string()
                } else if combined.contains("bottleneck") || combined.contains("slow") {
                    "bottleneck-analysis".to_string()
                } else if combined.contains("memory") || combined.contains("cache") {
                    "memory-optimization".to_string()
                } else {
                    "optimization".to_string()
                }
            }
            TaskCategory::IntegrationTasks => {
                if combined.contains("api") || combined.contains("rest") {
                    "api-integration".to_string()
                } else if combined.contains("workflow") || combined.contains("orchestrat") {
                    "workflow-orchestration".to_string()
                } else if combined.contains("message") || combined.contains("queue") {
                    "messaging".to_string()
                } else {
                    "system-integration".to_string()
                }
            }
            TaskCategory::FileOperations => {
                if combined.contains("search") || combined.contains("find") {
                    "search-replace".to_string()
                } else if combined.contains("archive") || combined.contains("compress") {
                    "archival".to_string()
                } else if combined.contains("text") || combined.contains("process") {
                    "text-processing".to_string()
                } else {
                    "file-organization".to_string()
                }
            }
            TaskCategory::Networking => {
                if combined.contains("dns") {
                    "dns-configuration".to_string()
                } else if combined.contains("firewall") || combined.contains("iptables") {
                    "firewall".to_string()
                } else if combined.contains("proxy") || combined.contains("reverse") {
                    "proxy-setup".to_string()
                } else if combined.contains("vpn") || combined.contains("tunnel") {
                    "vpn-tunneling".to_string()
                } else {
                    "diagnostics".to_string()
                }
            }
            TaskCategory::Containers => {
                if combined.contains("kubernetes") || combined.contains("k8s") {
                    "kubernetes".to_string()
                } else if combined.contains("compose") || combined.contains("multi-container") {
                    "compose".to_string()
                } else {
                    "docker-operations".to_string()
                }
            }
        }
    }

    /// Returns the agent configuration.
    pub fn config(&self) -> &IdeatorConfig {
        &self.config
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

/// Response structure from LLM idea generation.
#[derive(Debug, Deserialize)]
struct IdeaResponse {
    title: String,
    description: String,
    estimated_difficulty: String,
    required_skills: Vec<String>,
    anti_patterns: Vec<String>,
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
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            })
        }
    }

    #[test]
    fn test_task_category_all() {
        let categories = TaskCategory::all();
        assert_eq!(categories.len(), 16, "Should have 16 task categories");
    }

    #[test]
    fn test_task_category_display_name() {
        assert_eq!(
            TaskCategory::AlgorithmDesign.display_name(),
            "Algorithm Design"
        );
        assert_eq!(
            TaskCategory::SystemDebugging.display_name(),
            "System Debugging"
        );
        assert_eq!(
            TaskCategory::SecurityAnalysis.display_name(),
            "Security Analysis"
        );
    }

    #[test]
    fn test_task_category_to_benchmark() {
        assert_eq!(
            TaskCategory::AlgorithmDesign.to_benchmark_category(),
            "software-engineering"
        );
        assert_eq!(
            TaskCategory::SystemDebugging.to_benchmark_category(),
            "debugging"
        );
        assert_eq!(
            TaskCategory::SecurityAnalysis.to_benchmark_category(),
            "security"
        );
        assert_eq!(
            TaskCategory::Containers.to_benchmark_category(),
            "containers"
        );
    }

    #[test]
    fn test_ideator_config_defaults() {
        let config = IdeatorConfig::default();
        assert!((config.temperature - 1.0).abs() < 0.01);
        assert!((config.top_p - 0.95).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
    }

    #[test]
    fn test_ideator_config_builder() {
        let config = IdeatorConfig::new()
            .with_temperature(1.1)
            .with_top_p(0.9)
            .with_max_tokens(3000);

        assert!((config.temperature - 1.1).abs() < 0.01);
        assert!((config.top_p - 0.9).abs() < 0.01);
        assert_eq!(config.max_tokens, 3000);
    }

    #[test]
    fn test_ideator_config_temperature_clamping() {
        let config = IdeatorConfig::new().with_temperature(2.0);
        assert!(
            (config.temperature - 1.2).abs() < 0.01,
            "Temperature should be clamped to 1.2"
        );

        let config = IdeatorConfig::new().with_temperature(0.5);
        assert!(
            (config.temperature - 0.9).abs() < 0.01,
            "Temperature should be clamped to 0.9"
        );
    }

    #[test]
    fn test_task_idea_creation() {
        let idea = TaskIdea::new(
            TaskCategory::AlgorithmDesign,
            "optimization",
            "Test Task",
            "Test description",
            DifficultyLevel::Medium,
            vec!["skill1".to_string()],
            vec!["anti1".to_string()],
        );

        assert!(!idea.id.is_empty(), "ID should be generated");
        assert_eq!(idea.category, TaskCategory::AlgorithmDesign);
        assert_eq!(idea.title, "Test Task");
        assert_eq!(idea.estimated_difficulty, DifficultyLevel::Medium);
        assert_eq!(idea.benchmark_category(), "software-engineering");
    }

    #[tokio::test]
    async fn test_generate_task_idea_success() {
        let mock_response = r#"{
            "title": "Optimize Graph Traversal Algorithm",
            "description": "Given a weighted directed graph with 10,000 nodes, optimize the existing Dijkstra implementation to handle negative edge weights without using Bellman-Ford's O(VE) complexity.",
            "estimated_difficulty": "hard",
            "required_skills": ["graph algorithms", "dynamic programming", "complexity analysis"],
            "anti_patterns": ["brute force", "ignoring negative cycles", "memorization of standard algorithms"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = IdeatorAgent::with_defaults(mock_provider);

        let idea = agent
            .generate_task_idea(Some(TaskCategory::AlgorithmDesign))
            .await
            .expect("should generate idea");

        assert_eq!(idea.category, TaskCategory::AlgorithmDesign);
        assert_eq!(idea.title, "Optimize Graph Traversal Algorithm");
        assert_eq!(idea.estimated_difficulty, DifficultyLevel::Hard);
        assert_eq!(idea.required_skills.len(), 3);
        assert_eq!(idea.anti_patterns.len(), 3);
    }

    #[tokio::test]
    async fn test_generate_task_idea_with_markdown() {
        let mock_response = r#"Here is the generated task:

```json
{
    "title": "Debug Memory Leak in Async Service",
    "description": "A production service is experiencing gradual memory growth. Identify the source of the leak using profiling tools.",
    "estimated_difficulty": "medium",
    "required_skills": ["memory profiling", "async rust"],
    "anti_patterns": ["restarting the service"]
}
```

This task tests memory debugging skills."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = IdeatorAgent::with_defaults(mock_provider);

        let idea = agent
            .generate_task_idea(Some(TaskCategory::SystemDebugging))
            .await
            .expect("should extract JSON from markdown");

        assert_eq!(idea.title, "Debug Memory Leak in Async Service");
        assert_eq!(idea.estimated_difficulty, DifficultyLevel::Medium);
    }

    #[tokio::test]
    async fn test_generate_batch() {
        let mock_response = r#"{
            "title": "Test Task",
            "description": "Test description for batch generation.",
            "estimated_difficulty": "easy",
            "required_skills": ["testing"],
            "anti_patterns": ["skip tests"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = IdeatorAgent::with_defaults(mock_provider);

        let ideas = agent
            .generate_batch(
                3,
                Some(vec![TaskCategory::Debugging, TaskCategory::Security]),
            )
            .await
            .expect("should generate batch");

        assert_eq!(ideas.len(), 3);
        // Categories cycle through the provided list
        assert_eq!(ideas[0].category, TaskCategory::Debugging);
        assert_eq!(ideas[1].category, TaskCategory::Security);
        assert_eq!(ideas[2].category, TaskCategory::Debugging);
    }

    #[test]
    fn test_infer_subcategory() {
        // Test algorithm design subcategories - graph detection
        let subcategory = IdeatorAgent::infer_subcategory(
            "Graph Traversal Algorithm",
            "Implement breadth-first search for a directed graph",
            TaskCategory::AlgorithmDesign,
        );
        assert_eq!(subcategory, "graph-algorithms");

        // Test algorithm design subcategories - optimization detection
        let subcategory = IdeatorAgent::infer_subcategory(
            "Performance Tuning",
            "Optimization of database queries for faster response times",
            TaskCategory::AlgorithmDesign,
        );
        assert_eq!(subcategory, "optimization");

        // Test debugging subcategories
        let subcategory = IdeatorAgent::infer_subcategory(
            "Analyze Log Files",
            "Parse and analyze application logs to find errors",
            TaskCategory::Debugging,
        );
        assert_eq!(subcategory, "log-analysis");

        // Test security subcategories
        let subcategory = IdeatorAgent::infer_subcategory(
            "CTF Binary Challenge",
            "Solve this capture the flag challenge by analyzing the binary",
            TaskCategory::SecurityAnalysis,
        );
        assert_eq!(subcategory, "ctf-challenges");
    }

    #[test]
    fn test_find_matching_brace() {
        assert_eq!(find_matching_brace(r#"{}"#), Some(1));
        assert_eq!(find_matching_brace(r#"{"a": 1}"#), Some(7));
        assert_eq!(find_matching_brace(r#"{"a": {"b": 2}}"#), Some(14));
        assert_eq!(find_matching_brace(r#"{"a": "}"}"#), Some(9));
        assert_eq!(find_matching_brace(r#"{"a": "\"}"}"#), Some(11));
        assert_eq!(find_matching_brace(r#"{"#), None);
    }

    #[test]
    fn test_parse_difficulty() {
        assert_eq!(
            IdeatorAgent::parse_difficulty("easy").expect("valid"),
            DifficultyLevel::Easy
        );
        assert_eq!(
            IdeatorAgent::parse_difficulty("MEDIUM").expect("valid"),
            DifficultyLevel::Medium
        );
        assert_eq!(
            IdeatorAgent::parse_difficulty("  hard  ").expect("valid"),
            DifficultyLevel::Hard
        );
        assert!(IdeatorAgent::parse_difficulty("invalid").is_err());
    }

    #[test]
    fn test_task_category_description() {
        for category in TaskCategory::all() {
            let desc = category.description();
            assert!(
                !desc.is_empty(),
                "Category {:?} should have a description",
                category
            );
        }
    }

    #[test]
    fn test_task_category_display() {
        assert_eq!(
            format!("{}", TaskCategory::AlgorithmDesign),
            "Algorithm Design"
        );
        assert_eq!(
            format!("{}", TaskCategory::ReverseEngineering),
            "Reverse Engineering"
        );
    }

    #[tokio::test]
    async fn test_agent_name_constant() {
        assert_eq!(IdeatorAgent::AGENT_NAME, "task_ideator");
    }

    #[tokio::test]
    async fn test_agent_config_accessor() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let config = IdeatorConfig::new().with_temperature(1.1);
        let agent = IdeatorAgent::new(mock_provider, config);

        assert!((agent.config().temperature - 1.1).abs() < 0.01);
    }
}
