//! Synthetic Generator Agent for creating DevOps benchmark problems from scratch.
//!
//! This agent generates synthetic DevOps problems including:
//! - QEMU installation and configuration
//! - Systemd service configuration
//! - Database recovery and backup scenarios
//! - Network troubleshooting
//! - Docker and Kubernetes setup
//! - Terraform deployments
//!
//! Uses high-temperature LLM calls for creative problem generation.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::difficulty::DifficultyLevel;
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};

/// System prompt for synthetic problem generation.
const SYNTHETIC_GENERATION_PROMPT: &str = r#"You are an expert DevOps engineer creating challenging benchmark problems.

Generate a realistic DevOps problem scenario with an intentional bug that needs to be fixed.

Requirements:
1. The problem must be REALISTIC - something a DevOps engineer might encounter
2. The bug must be SUBTLE but FIXABLE - not obvious from the problem statement
3. Include ALL necessary setup code to reproduce the environment
4. Include TEST COMMANDS that verify the fix works
5. The Dockerfile must be complete and runnable

Output as JSON:
{
  "problem_statement": "<detailed description of the problem WITHOUT revealing the bug>",
  "setup_code": "<bash/shell code to set up the environment>",
  "intentional_bug": "<description of the bug that was introduced>",
  "solution_patch": "<the fix for the bug as a diff or code snippet>",
  "test_commands": ["cmd1", "cmd2"],
  "dockerfile": "<complete Dockerfile content>",
  "hints": ["hint1", "hint2"]
}

IMPORTANT: Output ONLY the JSON object, no additional text."#;

// ============================================================================
// Category Types
// ============================================================================

/// Categories for synthetic DevOps problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyntheticCategory {
    /// QEMU/KVM installation and configuration.
    QemuInstallation,
    /// Systemd service configuration and troubleshooting.
    SystemdConfiguration,
    /// PostgreSQL database recovery scenarios.
    PostgresRecovery,
    /// MySQL/MariaDB backup and restore.
    MysqlBackup,
    /// Network troubleshooting (DNS, firewall, routing).
    NetworkTroubleshooting,
    /// Docker configuration and Compose issues.
    DockerConfiguration,
    /// Kubernetes cluster setup and debugging.
    KubernetesSetup,
    /// Terraform deployment and state management.
    TerraformDeployment,
}

impl SyntheticCategory {
    /// Returns all available categories.
    pub fn all() -> Vec<SyntheticCategory> {
        vec![
            SyntheticCategory::QemuInstallation,
            SyntheticCategory::SystemdConfiguration,
            SyntheticCategory::PostgresRecovery,
            SyntheticCategory::MysqlBackup,
            SyntheticCategory::NetworkTroubleshooting,
            SyntheticCategory::DockerConfiguration,
            SyntheticCategory::KubernetesSetup,
            SyntheticCategory::TerraformDeployment,
        ]
    }

    /// Returns the display name for this category.
    pub fn display_name(&self) -> &'static str {
        match self {
            SyntheticCategory::QemuInstallation => "QEMU Installation",
            SyntheticCategory::SystemdConfiguration => "Systemd Configuration",
            SyntheticCategory::PostgresRecovery => "PostgreSQL Recovery",
            SyntheticCategory::MysqlBackup => "MySQL Backup",
            SyntheticCategory::NetworkTroubleshooting => "Network Troubleshooting",
            SyntheticCategory::DockerConfiguration => "Docker Configuration",
            SyntheticCategory::KubernetesSetup => "Kubernetes Setup",
            SyntheticCategory::TerraformDeployment => "Terraform Deployment",
        }
    }

    /// Returns a description of this category.
    pub fn description(&self) -> &'static str {
        match self {
            SyntheticCategory::QemuInstallation => {
                "QEMU/KVM virtual machine installation and configuration challenges"
            }
            SyntheticCategory::SystemdConfiguration => {
                "Systemd unit file creation, service management, and troubleshooting"
            }
            SyntheticCategory::PostgresRecovery => {
                "PostgreSQL database backup, recovery, and replication scenarios"
            }
            SyntheticCategory::MysqlBackup => {
                "MySQL/MariaDB backup strategies, restore procedures, and data migration"
            }
            SyntheticCategory::NetworkTroubleshooting => {
                "DNS resolution, firewall rules, routing tables, and connectivity issues"
            }
            SyntheticCategory::DockerConfiguration => {
                "Docker daemon configuration, Compose files, and container networking"
            }
            SyntheticCategory::KubernetesSetup => {
                "Kubernetes cluster configuration, pod debugging, and service mesh"
            }
            SyntheticCategory::TerraformDeployment => {
                "Terraform state management, module development, and cloud deployments"
            }
        }
    }

    /// Maps to the benchmark category string.
    pub fn to_benchmark_category(&self) -> &'static str {
        match self {
            SyntheticCategory::QemuInstallation => "system-administration",
            SyntheticCategory::SystemdConfiguration => "system-administration",
            SyntheticCategory::PostgresRecovery => "debugging",
            SyntheticCategory::MysqlBackup => "debugging",
            SyntheticCategory::NetworkTroubleshooting => "networking",
            SyntheticCategory::DockerConfiguration => "containers",
            SyntheticCategory::KubernetesSetup => "containers",
            SyntheticCategory::TerraformDeployment => "infrastructure",
        }
    }
}

impl std::fmt::Display for SyntheticCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for the synthetic generator.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Categories to generate problems for.
    pub categories: Vec<SyntheticCategory>,
    /// Distribution of difficulty levels.
    pub difficulty_distribution: HashMap<DifficultyLevel, f64>,
    /// Whether to include the solution in the output.
    pub include_solution: bool,
    /// Temperature for LLM generation (higher = more creative).
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        let mut difficulty_distribution = HashMap::new();
        difficulty_distribution.insert(DifficultyLevel::Easy, 0.2);
        difficulty_distribution.insert(DifficultyLevel::Medium, 0.5);
        difficulty_distribution.insert(DifficultyLevel::Hard, 0.3);

        Self {
            categories: SyntheticCategory::all(),
            difficulty_distribution,
            include_solution: true,
            temperature: 0.9,
            max_tokens: 4000,
        }
    }
}

impl GeneratorConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the categories to generate problems for.
    pub fn with_categories(mut self, categories: Vec<SyntheticCategory>) -> Self {
        self.categories = categories;
        self
    }

    /// Set a single category to focus on.
    pub fn with_category(mut self, category: SyntheticCategory) -> Self {
        self.categories = vec![category];
        self
    }

    /// Set the difficulty distribution.
    pub fn with_difficulty_distribution(
        mut self,
        distribution: HashMap<DifficultyLevel, f64>,
    ) -> Self {
        self.difficulty_distribution = distribution;
        self
    }

    /// Set whether to include solutions.
    pub fn with_include_solution(mut self, include: bool) -> Self {
        self.include_solution = include;
        self
    }

    /// Set the temperature for generation.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Set the maximum tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Select a difficulty level based on the distribution.
    pub fn select_difficulty(&self) -> DifficultyLevel {
        use rand::RngExt;

        let total: f64 = self.difficulty_distribution.values().sum();
        if total == 0.0 {
            return DifficultyLevel::Medium;
        }

        let mut rng = rand::rng();
        let roll: f64 = rng.random_range(0.0..total);

        let mut cumulative = 0.0;
        for (level, weight) in &self.difficulty_distribution {
            cumulative += weight;
            if roll < cumulative {
                return *level;
            }
        }

        DifficultyLevel::Medium
    }

    /// Select a random category from the configured list.
    pub fn select_category(&self) -> SyntheticCategory {
        use rand::seq::IndexedRandom;

        self.categories
            .choose(&mut rand::rng())
            .copied()
            .unwrap_or(SyntheticCategory::DockerConfiguration)
    }
}

// ============================================================================
// Synthetic Problem Types
// ============================================================================

/// A generated synthetic DevOps problem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticProblem {
    /// Unique identifier for this problem.
    pub id: String,
    /// Category of the problem.
    pub category: SyntheticCategory,
    /// Problem statement (what the user sees).
    pub problem_statement: String,
    /// Setup code to create the environment.
    pub setup_code: String,
    /// Description of the intentional bug.
    pub intentional_bug: String,
    /// Solution patch to fix the bug.
    pub solution_patch: String,
    /// Commands to test the fix.
    pub test_commands: Vec<String>,
    /// Complete Dockerfile for the environment.
    pub dockerfile: String,
    /// Difficulty level.
    pub difficulty: DifficultyLevel,
    /// Optional hints for solving.
    #[serde(default)]
    pub hints: Vec<String>,
}

/// Builder for creating `SyntheticProblem` instances.
#[derive(Debug, Clone)]
pub struct SyntheticProblemBuilder {
    category: SyntheticCategory,
    problem_statement: String,
    setup_code: String,
    intentional_bug: String,
    solution_patch: String,
    test_commands: Vec<String>,
    dockerfile: String,
    difficulty: DifficultyLevel,
    hints: Vec<String>,
}

impl SyntheticProblemBuilder {
    /// Create a new builder with required fields.
    pub fn new(category: SyntheticCategory, problem_statement: impl Into<String>) -> Self {
        Self {
            category,
            problem_statement: problem_statement.into(),
            setup_code: String::new(),
            intentional_bug: String::new(),
            solution_patch: String::new(),
            test_commands: Vec::new(),
            dockerfile: String::new(),
            difficulty: DifficultyLevel::Medium,
            hints: Vec::new(),
        }
    }

    /// Set the setup code.
    pub fn setup_code(mut self, code: impl Into<String>) -> Self {
        self.setup_code = code.into();
        self
    }

    /// Set the intentional bug description.
    pub fn intentional_bug(mut self, bug: impl Into<String>) -> Self {
        self.intentional_bug = bug.into();
        self
    }

    /// Set the solution patch.
    pub fn solution_patch(mut self, patch: impl Into<String>) -> Self {
        self.solution_patch = patch.into();
        self
    }

    /// Set the test commands.
    pub fn test_commands(mut self, commands: Vec<String>) -> Self {
        self.test_commands = commands;
        self
    }

    /// Set the Dockerfile content.
    pub fn dockerfile(mut self, dockerfile: impl Into<String>) -> Self {
        self.dockerfile = dockerfile.into();
        self
    }

    /// Set the difficulty level.
    pub fn difficulty(mut self, difficulty: DifficultyLevel) -> Self {
        self.difficulty = difficulty;
        self
    }

    /// Add hints for solving.
    pub fn hints<I, S>(mut self, hints: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.hints.extend(hints.into_iter().map(|s| s.into()));
        self
    }

    /// Build the `SyntheticProblem`.
    pub fn build(self) -> SyntheticProblem {
        SyntheticProblem {
            id: Uuid::new_v4().to_string(),
            category: self.category,
            problem_statement: self.problem_statement,
            setup_code: self.setup_code,
            intentional_bug: self.intentional_bug,
            solution_patch: self.solution_patch,
            test_commands: self.test_commands,
            dockerfile: self.dockerfile,
            difficulty: self.difficulty,
            hints: self.hints,
        }
    }
}

impl SyntheticProblem {
    /// Create a builder for a new synthetic problem.
    pub fn builder(
        category: SyntheticCategory,
        problem_statement: impl Into<String>,
    ) -> SyntheticProblemBuilder {
        SyntheticProblemBuilder::new(category, problem_statement)
    }

    /// Add hints to the problem.
    pub fn with_hints<I, S>(mut self, hints: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.hints.extend(hints.into_iter().map(|s| s.into()));
        self
    }

    /// Get the benchmark category string.
    pub fn benchmark_category(&self) -> &'static str {
        self.category.to_benchmark_category()
    }
}

// ============================================================================
// LLM Response Types
// ============================================================================

/// Response from synthetic problem generation LLM call.
#[derive(Debug, Clone, Deserialize)]
struct SyntheticGenerationResponse {
    problem_statement: String,
    setup_code: String,
    intentional_bug: String,
    solution_patch: String,
    test_commands: Vec<String>,
    dockerfile: String,
    #[serde(default)]
    hints: Vec<String>,
}

// ============================================================================
// Synthetic Generator Agent
// ============================================================================

/// Agent that generates synthetic DevOps problems.
pub struct SyntheticGeneratorAgent {
    llm: Arc<dyn LlmProvider>,
    config: GeneratorConfig,
}

impl std::fmt::Debug for SyntheticGeneratorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntheticGeneratorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl SyntheticGeneratorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "synthetic_generator";

    /// Create a new synthetic generator agent.
    pub fn new(llm: Arc<dyn LlmProvider>, config: GeneratorConfig) -> Self {
        Self { llm, config }
    }

    /// Create a new synthetic generator with default configuration.
    pub fn with_defaults(llm: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm, GeneratorConfig::default())
    }

    /// Generate synthetic problems.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for generation (overrides agent config)
    /// * `count` - Number of problems to generate
    ///
    /// # Returns
    ///
    /// A vector of generated synthetic problems.
    pub async fn generate(
        &self,
        config: &GeneratorConfig,
        count: usize,
    ) -> AgentResult<Vec<SyntheticProblem>> {
        let mut problems = Vec::with_capacity(count);

        for _ in 0..count {
            let category = config.select_category();
            let difficulty = config.select_difficulty();

            match self.generate_single(category, difficulty).await {
                Ok(problem) => problems.push(problem),
                Err(e) => {
                    // Log error but continue generating
                    tracing::warn!(
                        "Failed to generate problem for category {:?}: {}",
                        category,
                        e
                    );
                }
            }
        }

        if problems.is_empty() && count > 0 {
            return Err(AgentError::GenerationFailed(
                "Failed to generate any problems".to_string(),
            ));
        }

        Ok(problems)
    }

    /// Generate a single synthetic problem.
    pub async fn generate_single(
        &self,
        category: SyntheticCategory,
        difficulty: DifficultyLevel,
    ) -> AgentResult<SyntheticProblem> {
        let prompt = self.build_generation_prompt(category, difficulty);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(SYNTHETIC_GENERATION_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        let parsed = self.parse_generation_response(content)?;

        let problem = SyntheticProblem::builder(category, parsed.problem_statement)
            .setup_code(parsed.setup_code)
            .intentional_bug(parsed.intentional_bug)
            .solution_patch(parsed.solution_patch)
            .test_commands(parsed.test_commands)
            .dockerfile(parsed.dockerfile)
            .difficulty(difficulty)
            .hints(parsed.hints)
            .build();

        Ok(problem)
    }

    /// Build the prompt for problem generation.
    fn build_generation_prompt(
        &self,
        category: SyntheticCategory,
        difficulty: DifficultyLevel,
    ) -> String {
        let difficulty_guidance = match difficulty {
            DifficultyLevel::Easy => {
                "The bug should be relatively straightforward to find and fix. \
                 A junior engineer should be able to solve it in 5-10 minutes. \
                 Example: missing configuration value, typo in filename."
            }
            DifficultyLevel::Medium => {
                "The bug should require some investigation and understanding. \
                 A mid-level engineer should solve it in 15-30 minutes. \
                 Example: incorrect permissions, missing dependency, race condition."
            }
            DifficultyLevel::Hard => {
                "The bug should be subtle and require deep expertise. \
                 A senior engineer might need 30-60 minutes to diagnose. \
                 Example: complex timing issue, security misconfiguration, data corruption."
            }
        };

        format!(
            "Generate a {} difficulty problem for category: {}\n\n\
             Category Description: {}\n\n\
             Difficulty Guidance:\n{}\n\n\
             Create a realistic scenario that a DevOps engineer might encounter in production.",
            format!("{:?}", difficulty).to_lowercase(),
            category.display_name(),
            category.description(),
            difficulty_guidance
        )
    }

    /// Parse the LLM response for problem generation.
    fn parse_generation_response(&self, content: &str) -> AgentResult<SyntheticGenerationResponse> {
        let json_content = self.extract_json(content)?;

        serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))
    }

    /// Extract JSON from the response, handling potential markdown code blocks.
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

    /// Generate problems for a specific category.
    pub async fn generate_for_category(
        &self,
        category: SyntheticCategory,
        count: usize,
    ) -> AgentResult<Vec<SyntheticProblem>> {
        let config = GeneratorConfig::new().with_category(category);
        self.generate(&config, count).await
    }

    /// Generate problems filling gaps in coverage.
    ///
    /// Given a set of existing categories, generates problems for
    /// categories that are underrepresented.
    pub async fn generate_filling_gaps(
        &self,
        existing_counts: &HashMap<SyntheticCategory, usize>,
        target_per_category: usize,
    ) -> AgentResult<Vec<SyntheticProblem>> {
        let mut problems = Vec::new();

        for category in SyntheticCategory::all() {
            let existing = existing_counts.get(&category).copied().unwrap_or(0);
            if existing < target_per_category {
                let needed = target_per_category - existing;
                match self.generate_for_category(category, needed).await {
                    Ok(mut generated) => problems.append(&mut generated),
                    Err(e) => {
                        tracing::warn!("Failed to generate problems for {:?}: {}", category, e);
                    }
                }
            }
        }

        Ok(problems)
    }
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
    fn test_synthetic_category_all() {
        let categories = SyntheticCategory::all();
        assert_eq!(categories.len(), 8);
    }

    #[test]
    fn test_synthetic_category_display() {
        assert_eq!(
            SyntheticCategory::QemuInstallation.display_name(),
            "QEMU Installation"
        );
        assert_eq!(
            SyntheticCategory::DockerConfiguration.display_name(),
            "Docker Configuration"
        );
    }

    #[test]
    fn test_synthetic_category_benchmark_mapping() {
        assert_eq!(
            SyntheticCategory::QemuInstallation.to_benchmark_category(),
            "system-administration"
        );
        assert_eq!(
            SyntheticCategory::DockerConfiguration.to_benchmark_category(),
            "containers"
        );
        assert_eq!(
            SyntheticCategory::NetworkTroubleshooting.to_benchmark_category(),
            "networking"
        );
    }

    #[test]
    fn test_generator_config_defaults() {
        let config = GeneratorConfig::default();

        assert_eq!(config.categories.len(), 8);
        assert!(config.include_solution);
        assert!((config.temperature - 0.9).abs() < 0.01);
        assert_eq!(config.max_tokens, 4000);
    }

    #[test]
    fn test_generator_config_builder() {
        let config = GeneratorConfig::new()
            .with_category(SyntheticCategory::DockerConfiguration)
            .with_temperature(1.2)
            .with_include_solution(false)
            .with_max_tokens(8000);

        assert_eq!(config.categories.len(), 1);
        assert_eq!(config.categories[0], SyntheticCategory::DockerConfiguration);
        assert!((config.temperature - 1.2).abs() < 0.01);
        assert!(!config.include_solution);
        assert_eq!(config.max_tokens, 8000);
    }

    #[test]
    fn test_generator_config_difficulty_distribution() {
        let mut distribution = HashMap::new();
        distribution.insert(DifficultyLevel::Easy, 1.0);
        distribution.insert(DifficultyLevel::Medium, 0.0);
        distribution.insert(DifficultyLevel::Hard, 0.0);

        let config = GeneratorConfig::new().with_difficulty_distribution(distribution);

        // With only Easy having weight, it should always be selected
        let selected = config.select_difficulty();
        assert_eq!(selected, DifficultyLevel::Easy);
    }

    #[test]
    fn test_synthetic_problem_creation() {
        let problem = SyntheticProblem::builder(
            SyntheticCategory::DockerConfiguration,
            "Docker container fails to start",
        )
        .setup_code("docker-compose up -d")
        .intentional_bug("Port binding conflict")
        .solution_patch("Change port from 80 to 8080")
        .test_commands(vec!["curl localhost:8080".to_string()])
        .dockerfile("FROM nginx:latest")
        .difficulty(DifficultyLevel::Medium)
        .hints(["Check port bindings", "Look at docker logs"])
        .build();

        assert_eq!(problem.category, SyntheticCategory::DockerConfiguration);
        assert!(!problem.id.is_empty());
        assert_eq!(problem.hints.len(), 2);
        assert_eq!(problem.benchmark_category(), "containers");
    }

    #[tokio::test]
    async fn test_generate_single() {
        let mock_response = r#"{
            "problem_statement": "Your PostgreSQL database is not accepting connections",
            "setup_code": "systemctl start postgresql",
            "intentional_bug": "pg_hba.conf misconfigured to reject all connections",
            "solution_patch": "Add 'host all all 127.0.0.1/32 md5' to pg_hba.conf",
            "test_commands": ["psql -U postgres -c 'SELECT 1'"],
            "dockerfile": "FROM postgres:15\nCOPY pg_hba.conf /etc/postgresql/",
            "hints": ["Check authentication configuration"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = SyntheticGeneratorAgent::with_defaults(mock_provider);

        let result = agent
            .generate_single(SyntheticCategory::PostgresRecovery, DifficultyLevel::Medium)
            .await;

        assert!(result.is_ok());
        let problem = result.expect("should generate problem");
        assert_eq!(problem.category, SyntheticCategory::PostgresRecovery);
        assert!(!problem.problem_statement.is_empty());
        assert!(!problem.solution_patch.is_empty());
    }

    #[tokio::test]
    async fn test_generate_multiple() {
        let mock_response = r#"{
            "problem_statement": "Service fails to start",
            "setup_code": "systemctl start myservice",
            "intentional_bug": "Missing dependency",
            "solution_patch": "apt install libfoo",
            "test_commands": ["systemctl status myservice"],
            "dockerfile": "FROM ubuntu:24.04",
            "hints": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = SyntheticGeneratorAgent::with_defaults(mock_provider);

        let config = GeneratorConfig::new();
        let result = agent.generate(&config, 3).await;

        assert!(result.is_ok());
        let problems = result.expect("should generate problems");
        assert_eq!(problems.len(), 3);
    }

    #[test]
    fn test_generation_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = SyntheticGeneratorAgent::with_defaults(mock_provider);

        let prompt = agent
            .build_generation_prompt(SyntheticCategory::KubernetesSetup, DifficultyLevel::Hard);

        assert!(prompt.contains("Kubernetes Setup"));
        assert!(prompt.contains("hard"));
        assert!(prompt.contains("senior engineer"));
    }

    #[test]
    fn test_json_extraction() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = SyntheticGeneratorAgent::with_defaults(mock_provider);

        // Test direct JSON
        let result = agent.extract_json(r#"{"key": "value"}"#);
        assert!(result.is_ok());

        // Test JSON in code block
        let result = agent.extract_json("```json\n{\"key\": \"value\"}\n```");
        assert!(result.is_ok());

        // Test JSON with surrounding text
        let result = agent.extract_json("Here is the result: {\"key\": \"value\"} end");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_for_category() {
        let mock_response = r#"{
            "problem_statement": "Network unreachable",
            "setup_code": "ip route add default via 10.0.0.1",
            "intentional_bug": "Wrong gateway",
            "solution_patch": "ip route del default; ip route add default via 192.168.1.1",
            "test_commands": ["ping -c 1 8.8.8.8"],
            "dockerfile": "FROM alpine:latest",
            "hints": ["Check routing table"]
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = SyntheticGeneratorAgent::with_defaults(mock_provider);

        let result = agent
            .generate_for_category(SyntheticCategory::NetworkTroubleshooting, 2)
            .await;

        assert!(result.is_ok());
        let problems = result.expect("should generate problems");
        assert_eq!(problems.len(), 2);
        for problem in &problems {
            assert_eq!(problem.category, SyntheticCategory::NetworkTroubleshooting);
        }
    }

    #[test]
    fn test_category_to_string() {
        assert_eq!(
            SyntheticCategory::TerraformDeployment.to_string(),
            "Terraform Deployment"
        );
    }

    #[test]
    fn test_temperature_clamping() {
        let config = GeneratorConfig::new().with_temperature(3.0);
        assert!((config.temperature - 2.0).abs() < 0.01);

        let config = GeneratorConfig::new().with_temperature(-1.0);
        assert!((config.temperature - 0.0).abs() < 0.01);
    }
}
