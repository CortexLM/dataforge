//! Environment Builder Agent for creating reproducible task environments.
//!
//! This agent builds Docker-based environments for benchmark tasks, handling:
//! - Repository cloning at specific commits
//! - Dockerfile generation with proper dependencies
//! - Canary token injection for anti-hardcoding detection
//! - Support for multiple runtime environments (Python, Node, Rust, Go)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::difficulty::DifficultyLevel;
use crate::docker::{select_base_image, DockerfileBuilder, DockerfileConfig};
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::error::{AgentError, AgentResult};

/// System prompt for dependency analysis.
const DEPENDENCY_ANALYSIS_PROMPT: &str = r#"You are an expert at analyzing software projects and determining their dependencies.

Given a problem description and optional repository information, identify ALL required dependencies for the environment.

Output as JSON:
{
  "runtime": "python|node|rust|go|multi",
  "runtime_version": "<version>",
  "package_manager": "pip|poetry|npm|yarn|cargo|go",
  "dependencies": ["dep1", "dep2"],
  "system_packages": ["pkg1", "pkg2"],
  "setup_commands": ["cmd1", "cmd2"]
}

IMPORTANT: Output ONLY the JSON object, no additional text."#;

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for environment building.
#[derive(Debug, Clone)]
pub struct EnvironmentConfig {
    /// Base images for different runtimes.
    pub base_images: HashMap<String, String>,
    /// Git clone depth (None for full clone).
    pub clone_depth: Option<u32>,
    /// Whether to include development dependencies.
    pub include_dev_deps: bool,
    /// Whether to inject canary tokens for anti-hardcoding.
    pub canary_injection: bool,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        let mut base_images = HashMap::new();
        base_images.insert(
            "python".to_string(),
            "dataforge/python-3.13:latest".to_string(),
        );
        base_images.insert("node".to_string(), "dataforge/node-22:latest".to_string());
        base_images.insert("rust".to_string(), "dataforge/rust-1.80:latest".to_string());
        base_images.insert("go".to_string(), "dataforge/go-1.22:latest".to_string());
        base_images.insert(
            "multi".to_string(),
            "dataforge/multi-lang:latest".to_string(),
        );
        base_images.insert(
            "ubuntu".to_string(),
            "dataforge/ubuntu-24.04:latest".to_string(),
        );

        Self {
            base_images,
            clone_depth: Some(1),
            include_dev_deps: false,
            canary_injection: true,
        }
    }
}

impl EnvironmentConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the clone depth for git operations.
    pub fn with_clone_depth(mut self, depth: Option<u32>) -> Self {
        self.clone_depth = depth;
        self
    }

    /// Set whether to include development dependencies.
    pub fn with_dev_deps(mut self, include: bool) -> Self {
        self.include_dev_deps = include;
        self
    }

    /// Set whether to inject canary tokens.
    pub fn with_canary_injection(mut self, inject: bool) -> Self {
        self.canary_injection = inject;
        self
    }

    /// Add or override a base image for a runtime.
    pub fn with_base_image(mut self, runtime: impl Into<String>, image: impl Into<String>) -> Self {
        self.base_images.insert(runtime.into(), image.into());
        self
    }

    /// Get the base image for a runtime.
    pub fn get_base_image(&self, runtime: &str) -> &str {
        self.base_images
            .get(runtime)
            .map(|s| s.as_str())
            .unwrap_or("dataforge/ubuntu-24.04:latest")
    }
}

// ============================================================================
// Task Analysis Types
// ============================================================================

/// Analyzed task information for environment building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedTask {
    /// Unique task identifier.
    pub task_id: String,
    /// Problem description.
    pub problem_statement: String,
    /// Optional repository URL.
    pub repo_url: Option<String>,
    /// Optional commit SHA to checkout.
    pub commit_sha: Option<String>,
    /// Category of the task.
    pub category: String,
    /// Difficulty level.
    pub difficulty: DifficultyLevel,
    /// Additional context for dependency analysis.
    pub context: HashMap<String, String>,
}

impl AnalyzedTask {
    /// Create a new analyzed task.
    pub fn new(
        task_id: impl Into<String>,
        problem_statement: impl Into<String>,
        category: impl Into<String>,
        difficulty: DifficultyLevel,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            problem_statement: problem_statement.into(),
            repo_url: None,
            commit_sha: None,
            category: category.into(),
            difficulty,
            context: HashMap::new(),
        }
    }

    /// Set the repository URL.
    pub fn with_repo(mut self, url: impl Into<String>) -> Self {
        self.repo_url = Some(url.into());
        self
    }

    /// Set the commit SHA.
    pub fn with_commit(mut self, sha: impl Into<String>) -> Self {
        self.commit_sha = Some(sha.into());
        self
    }

    /// Add context for dependency analysis.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Built Environment Types
// ============================================================================

/// A successfully built environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltEnvironment {
    /// Path to the working directory.
    pub workdir_path: PathBuf,
    /// Generated Dockerfile content.
    pub dockerfile_content: String,
    /// Repository URL if cloned.
    pub repo_url: Option<String>,
    /// Commit SHA if checked out.
    pub commit_sha: Option<String>,
    /// Canary token for anti-hardcoding detection.
    pub canary_token: String,
    /// List of dependencies installed.
    pub dependencies: Vec<String>,
    /// Runtime version used.
    pub runtime_version: String,
    /// Whether the clone was successful.
    pub clone_successful: bool,
}

impl BuiltEnvironment {
    /// Create a new built environment.
    pub fn new(workdir_path: PathBuf, dockerfile_content: String) -> Self {
        Self {
            workdir_path,
            dockerfile_content,
            repo_url: None,
            commit_sha: None,
            canary_token: generate_canary_token(),
            dependencies: Vec::new(),
            runtime_version: String::new(),
            clone_successful: false,
        }
    }

    /// Set the repository information.
    pub fn with_repo_info(
        mut self,
        url: Option<String>,
        sha: Option<String>,
        cloned: bool,
    ) -> Self {
        self.repo_url = url;
        self.commit_sha = sha;
        self.clone_successful = cloned;
        self
    }

    /// Set the dependencies.
    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Set the runtime version.
    pub fn with_runtime_version(mut self, version: impl Into<String>) -> Self {
        self.runtime_version = version.into();
        self
    }

    /// Set a custom canary token.
    pub fn with_canary_token(mut self, token: impl Into<String>) -> Self {
        self.canary_token = token.into();
        self
    }
}

/// Generate a unique canary token.
fn generate_canary_token() -> String {
    format!(
        "DATAFORGE_CANARY_{}",
        Uuid::new_v4().to_string().replace('-', "_").to_uppercase()
    )
}

// ============================================================================
// LLM Response Types
// ============================================================================

/// Response from dependency analysis LLM call.
#[derive(Debug, Clone, Deserialize)]
struct DependencyAnalysisResponse {
    runtime: String,
    runtime_version: String,
    #[serde(default)]
    package_manager: String,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    system_packages: Vec<String>,
    #[serde(default)]
    setup_commands: Vec<String>,
}

// ============================================================================
// Environment Builder Agent
// ============================================================================

/// Agent that builds reproducible environments for benchmark tasks.
pub struct EnvironmentBuilderAgent {
    llm: Arc<dyn LlmProvider>,
    config: EnvironmentConfig,
}

impl std::fmt::Debug for EnvironmentBuilderAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvironmentBuilderAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl EnvironmentBuilderAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "environment_builder";

    /// Create a new environment builder agent.
    pub fn new(llm: Arc<dyn LlmProvider>, config: EnvironmentConfig) -> Self {
        Self { llm, config }
    }

    /// Create a new environment builder with default configuration.
    pub fn with_defaults(llm: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm, EnvironmentConfig::default())
    }

    /// Build an environment for a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The analyzed task to build an environment for
    /// * `output_dir` - Directory where the environment files will be created
    ///
    /// # Returns
    ///
    /// A `BuiltEnvironment` containing the environment configuration.
    pub async fn build(
        &self,
        task: &AnalyzedTask,
        output_dir: &Path,
    ) -> AgentResult<BuiltEnvironment> {
        // Analyze dependencies using LLM
        let analysis = self.analyze_dependencies(task).await?;

        // Determine the appropriate base image
        let base_image = self.select_base_image(&analysis.runtime, &task.category);

        // Generate the Dockerfile
        let dockerfile_content = self.generate_dockerfile(task, &base_image, &analysis);

        // Create the working directory path
        let workdir_path = output_dir.join(&task.task_id);

        // Generate canary token if enabled
        let canary_token = if self.config.canary_injection {
            generate_canary_token()
        } else {
            String::new()
        };

        // Build the environment result
        let mut environment = BuiltEnvironment::new(workdir_path, dockerfile_content)
            .with_dependencies(analysis.dependencies.clone())
            .with_runtime_version(analysis.runtime_version.clone())
            .with_canary_token(canary_token);

        // Add repo info if available
        if task.repo_url.is_some() || task.commit_sha.is_some() {
            environment = environment.with_repo_info(
                task.repo_url.clone(),
                task.commit_sha.clone(),
                task.repo_url.is_some(),
            );
        }

        Ok(environment)
    }

    /// Analyze dependencies for a task using LLM.
    async fn analyze_dependencies(
        &self,
        task: &AnalyzedTask,
    ) -> AgentResult<DependencyAnalysisResponse> {
        let prompt = self.build_dependency_prompt(task);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(DEPENDENCY_ANALYSIS_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(0.3)
        .with_max_tokens(1000);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_dependency_response(content)
    }

    /// Build the prompt for dependency analysis.
    fn build_dependency_prompt(&self, task: &AnalyzedTask) -> String {
        let mut prompt = format!(
            "Analyze the following task and determine its environment dependencies.\n\n\
             Task ID: {}\n\
             Category: {}\n\
             Difficulty: {:?}\n\n\
             Problem Statement:\n{}\n",
            task.task_id, task.category, task.difficulty, task.problem_statement
        );

        if let Some(ref repo_url) = task.repo_url {
            prompt.push_str(&format!("\nRepository: {}\n", repo_url));
        }

        if let Some(ref commit_sha) = task.commit_sha {
            prompt.push_str(&format!("Commit: {}\n", commit_sha));
        }

        if !task.context.is_empty() {
            prompt.push_str("\nAdditional Context:\n");
            for (key, value) in &task.context {
                prompt.push_str(&format!("- {}: {}\n", key, value));
            }
        }

        prompt
    }

    /// Parse the LLM response for dependency analysis.
    fn parse_dependency_response(&self, content: &str) -> AgentResult<DependencyAnalysisResponse> {
        let json_content = self.extract_json(content)?;

        serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))
    }

    /// Extract JSON from the response, handling potential markdown code blocks.
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

    /// Select the appropriate base image for a runtime.
    fn select_base_image(&self, runtime: &str, category: &str) -> String {
        // First try to get from config
        if let Some(image) = self.config.base_images.get(runtime) {
            return image.clone();
        }

        // Fall back to the docker module's selection logic
        let requirements: Vec<String> = vec![runtime.to_string()];
        select_base_image(category, &requirements)
    }

    /// Generate a Dockerfile for the task.
    fn generate_dockerfile(
        &self,
        task: &AnalyzedTask,
        base_image: &str,
        analysis: &DependencyAnalysisResponse,
    ) -> String {
        let mut env_vars = HashMap::new();
        env_vars.insert("TASK_ID".to_string(), task.task_id.clone());
        env_vars.insert("TASK_CATEGORY".to_string(), task.category.clone());

        // Add canary token if enabled
        if self.config.canary_injection {
            let canary = generate_canary_token();
            env_vars.insert("CANARY_TOKEN".to_string(), canary);
        }

        let config = DockerfileConfig {
            base_image: base_image.to_string(),
            task_id: task.task_id.clone(),
            category: task.category.clone(),
            difficulty: format!("{:?}", task.difficulty).to_lowercase(),
            packages: analysis.system_packages.clone(),
            copy_paths: Vec::new(),
            env_vars,
            user: "user".to_string(),
            workdir: "/home/user/workspace".to_string(),
        };

        let mut dockerfile = DockerfileBuilder::new(config).build();

        // Add runtime-specific setup
        dockerfile.push_str(&self.generate_runtime_setup(analysis));

        dockerfile
    }

    /// Generate runtime-specific setup commands.
    fn generate_runtime_setup(&self, analysis: &DependencyAnalysisResponse) -> String {
        let mut setup = String::new();

        if analysis.dependencies.is_empty() && analysis.setup_commands.is_empty() {
            return setup;
        }

        setup.push_str("\n# Runtime-specific setup\n");

        match analysis.runtime.as_str() {
            "python" => {
                if !analysis.dependencies.is_empty() {
                    let deps = analysis.dependencies.join(" ");
                    if analysis.package_manager == "poetry" {
                        setup.push_str(&format!(
                            "RUN poetry add {} || pip install {}\n",
                            deps, deps
                        ));
                    } else {
                        setup.push_str(&format!("RUN pip install --no-cache-dir {}\n", deps));
                    }
                }
            }
            "node" => {
                if !analysis.dependencies.is_empty() {
                    let deps = analysis.dependencies.join(" ");
                    if analysis.package_manager == "yarn" {
                        setup.push_str(&format!("RUN yarn add {}\n", deps));
                    } else {
                        setup.push_str(&format!("RUN npm install {}\n", deps));
                    }
                }
            }
            "rust" => {
                // Rust dependencies are typically handled via Cargo.toml
                // We just ensure the toolchain is ready
                setup.push_str("RUN rustup update stable\n");
            }
            "go" => {
                if !analysis.dependencies.is_empty() {
                    for dep in &analysis.dependencies {
                        setup.push_str(&format!("RUN go get {}\n", dep));
                    }
                }
            }
            _ => {}
        }

        // Add any custom setup commands
        for cmd in &analysis.setup_commands {
            setup.push_str(&format!("RUN {}\n", cmd));
        }

        setup
    }

    /// Generate git clone commands for the Dockerfile.
    pub fn generate_clone_commands(&self, task: &AnalyzedTask) -> Vec<String> {
        let mut commands = Vec::new();

        if let Some(ref repo_url) = task.repo_url {
            let depth_arg = match self.config.clone_depth {
                Some(depth) => format!("--depth {}", depth),
                None => String::new(),
            };

            commands.push(format!(
                "git clone {} {} /home/user/workspace/repo",
                depth_arg, repo_url
            ));

            if let Some(ref commit_sha) = task.commit_sha {
                commands.push(format!(
                    "cd /home/user/workspace/repo && git checkout {}",
                    commit_sha
                ));
            }
        }

        commands
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
    fn test_environment_config_defaults() {
        let config = EnvironmentConfig::default();

        assert!(config.base_images.contains_key("python"));
        assert!(config.base_images.contains_key("node"));
        assert!(config.base_images.contains_key("rust"));
        assert!(config.base_images.contains_key("go"));
        assert_eq!(config.clone_depth, Some(1));
        assert!(!config.include_dev_deps);
        assert!(config.canary_injection);
    }

    #[test]
    fn test_environment_config_builder() {
        let config = EnvironmentConfig::new()
            .with_clone_depth(Some(5))
            .with_dev_deps(true)
            .with_canary_injection(false)
            .with_base_image("custom", "my-image:latest");

        assert_eq!(config.clone_depth, Some(5));
        assert!(config.include_dev_deps);
        assert!(!config.canary_injection);
        assert_eq!(config.get_base_image("custom"), "my-image:latest");
    }

    #[test]
    fn test_analyzed_task_creation() {
        let task = AnalyzedTask::new(
            "task-001",
            "Fix the memory leak in the application",
            "debugging",
            DifficultyLevel::Medium,
        )
        .with_repo("https://github.com/example/repo")
        .with_commit("abc123")
        .with_context("language", "python");

        assert_eq!(task.task_id, "task-001");
        assert_eq!(
            task.repo_url,
            Some("https://github.com/example/repo".to_string())
        );
        assert_eq!(task.commit_sha, Some("abc123".to_string()));
        assert_eq!(task.context.get("language"), Some(&"python".to_string()));
    }

    #[test]
    fn test_built_environment_creation() {
        let env = BuiltEnvironment::new(
            PathBuf::from("/tmp/task-001"),
            "FROM ubuntu:24.04\n".to_string(),
        )
        .with_dependencies(vec!["numpy".to_string(), "pandas".to_string()])
        .with_runtime_version("3.13")
        .with_repo_info(
            Some("https://github.com/example/repo".to_string()),
            Some("abc123".to_string()),
            true,
        );

        assert_eq!(env.workdir_path, PathBuf::from("/tmp/task-001"));
        assert_eq!(env.dependencies.len(), 2);
        assert_eq!(env.runtime_version, "3.13");
        assert!(env.clone_successful);
        assert!(!env.canary_token.is_empty());
    }

    #[test]
    fn test_canary_token_generation() {
        let token1 = generate_canary_token();
        let token2 = generate_canary_token();

        assert!(token1.starts_with("DATAFORGE_CANARY_"));
        assert!(token2.starts_with("DATAFORGE_CANARY_"));
        assert_ne!(token1, token2);
    }

    #[tokio::test]
    async fn test_build_environment() {
        let mock_response = r#"{
            "runtime": "python",
            "runtime_version": "3.13",
            "package_manager": "pip",
            "dependencies": ["numpy", "pandas"],
            "system_packages": ["git", "curl"],
            "setup_commands": []
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let task = AnalyzedTask::new(
            "test-task-001",
            "Analyze data using pandas",
            "data-science",
            DifficultyLevel::Medium,
        );

        let output_dir = Path::new("/tmp/environments");
        let result = agent
            .build(&task, output_dir)
            .await
            .expect("build should succeed");

        assert!(result
            .dockerfile_content
            .contains("dataforge/python-3.13:latest"));
        assert!(result.dockerfile_content.contains("pip install"));
        assert_eq!(result.dependencies, vec!["numpy", "pandas"]);
        assert_eq!(result.runtime_version, "3.13");
    }

    #[test]
    fn test_dependency_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let task = AnalyzedTask::new(
            "task-001",
            "Fix the bug",
            "debugging",
            DifficultyLevel::Hard,
        )
        .with_repo("https://github.com/example/repo")
        .with_context("framework", "django");

        let prompt = agent.build_dependency_prompt(&task);

        assert!(prompt.contains("task-001"));
        assert!(prompt.contains("debugging"));
        assert!(prompt.contains("Fix the bug"));
        assert!(prompt.contains("https://github.com/example/repo"));
        assert!(prompt.contains("framework: django"));
    }

    #[test]
    fn test_clone_commands_generation() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let config = EnvironmentConfig::new().with_clone_depth(Some(1));
        let agent = EnvironmentBuilderAgent::new(mock_provider, config);

        let task = AnalyzedTask::new(
            "task-001",
            "Fix the bug",
            "debugging",
            DifficultyLevel::Medium,
        )
        .with_repo("https://github.com/example/repo")
        .with_commit("abc123");

        let commands = agent.generate_clone_commands(&task);

        assert_eq!(commands.len(), 2);
        assert!(commands[0].contains("--depth 1"));
        assert!(commands[0].contains("https://github.com/example/repo"));
        assert!(commands[1].contains("git checkout abc123"));
    }

    #[test]
    fn test_clone_commands_no_repo() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let task = AnalyzedTask::new(
            "task-001",
            "Simple task",
            "file-operations",
            DifficultyLevel::Easy,
        );

        let commands = agent.generate_clone_commands(&task);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_json_extraction() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        // Test direct JSON
        let result = agent.extract_json(r#"{"key": "value"}"#);
        assert!(result.is_ok());
        assert_eq!(result.expect("valid json"), r#"{"key": "value"}"#);

        // Test JSON in code block
        let result = agent.extract_json("```json\n{\"key\": \"value\"}\n```");
        assert!(result.is_ok());

        // Test JSON with surrounding text
        let result = agent.extract_json("Here is the result: {\"key\": \"value\"} end");
        assert!(result.is_ok());
    }

    #[test]
    fn test_runtime_setup_python() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let analysis = DependencyAnalysisResponse {
            runtime: "python".to_string(),
            runtime_version: "3.13".to_string(),
            package_manager: "pip".to_string(),
            dependencies: vec!["numpy".to_string(), "pandas".to_string()],
            system_packages: vec![],
            setup_commands: vec![],
        };

        let setup = agent.generate_runtime_setup(&analysis);

        assert!(setup.contains("pip install"));
        assert!(setup.contains("numpy pandas"));
    }

    #[test]
    fn test_runtime_setup_node() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let analysis = DependencyAnalysisResponse {
            runtime: "node".to_string(),
            runtime_version: "22".to_string(),
            package_manager: "npm".to_string(),
            dependencies: vec!["express".to_string(), "lodash".to_string()],
            system_packages: vec![],
            setup_commands: vec![],
        };

        let setup = agent.generate_runtime_setup(&analysis);

        assert!(setup.contains("npm install"));
        assert!(setup.contains("express lodash"));
    }

    #[test]
    fn test_runtime_setup_yarn() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = EnvironmentBuilderAgent::with_defaults(mock_provider);

        let analysis = DependencyAnalysisResponse {
            runtime: "node".to_string(),
            runtime_version: "22".to_string(),
            package_manager: "yarn".to_string(),
            dependencies: vec!["react".to_string()],
            system_packages: vec![],
            setup_commands: vec![],
        };

        let setup = agent.generate_runtime_setup(&analysis);

        assert!(setup.contains("yarn add"));
        assert!(setup.contains("react"));
    }
}
