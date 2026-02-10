//! Workspace Ideator Agent for generating realistic code project ideas.
//!
//! This agent generates ideas for complete code workspaces that can be used
//! to create benchmark tasks with injected vulnerabilities. It focuses on
//! creating realistic project concepts where security vulnerabilities would
//! make contextual sense.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::{AgentError, AgentResult};
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for workspace ideation.
const WORKSPACE_IDEATION_SYSTEM_PROMPT: &str = r#"You are a senior software architect designing REALISTIC code projects for security benchmark generation.

Your goal is to create detailed specifications for complete, working code projects that:
1. Represent real-world applications developers actually build
2. Have appropriate complexity for the chosen programming language
3. Contain natural places where security vulnerabilities could exist
4. Would be indistinguishable from production codebases

PROJECT TYPES TO CONSIDER:
- REST APIs (authentication, data processing, file handling)
- CLI tools (file manipulation, data transformation, system utilities)
- Web applications (user management, content systems, dashboards)
- Data pipelines (ETL processes, report generation, data validation)
- Microservices (payment processing, notification systems, search services)
- Libraries/SDKs (HTTP clients, database wrappers, serialization utilities)

LANGUAGE SELECTION CRITERIA:
- Python: Web APIs, data pipelines, automation scripts, ML services
- Rust: System tools, CLI applications, high-performance services, cryptography
- JavaScript/TypeScript: Web frontends, Node.js backends, serverless functions
- Go: Microservices, CLI tools, infrastructure utilities
- Java: Enterprise APIs, Android apps, data processing
- C/C++: System programming, embedded systems, performance-critical code

VULNERABILITY EMBEDDING OPPORTUNITIES:
Consider projects where these vulnerabilities would naturally occur:
- SQL injection: Database-backed applications
- XSS: Web applications with user-generated content
- Authentication bypass: Login systems, API auth
- Path traversal: File serving, upload handling
- Insecure deserialization: APIs accepting serialized data
- Race conditions: Concurrent resource access
- Memory issues: Low-level languages, buffer handling
- Hardcoded secrets: Configuration management, API integrations

CRITICAL REQUIREMENTS:
- Projects must be COMPLETE and FUNCTIONAL (not stubs)
- Code structure must match real-world patterns
- Dependencies must be realistic and commonly used
- File organization must follow language conventions
- The project should compile/run successfully before vulnerability injection"#;

/// User prompt template for workspace ideation.
const WORKSPACE_IDEATION_USER_TEMPLATE: &str = r#"Generate a detailed workspace specification for a realistic code project.

Constraints:
- Project Type Focus: {project_type}
- Target Language: {language}
- Complexity Level: {complexity}

Create a comprehensive project specification that a real developer might build.

You MUST respond with ONLY valid JSON:
{{
  "project_name": "descriptive-project-name",
  "description": "2-3 sentence description of what the project does",
  "language": "primary programming language",
  "framework": "main framework or 'none' if vanilla",
  "project_type": "api|cli|web|pipeline|microservice|library",
  "complexity": "simple|moderate|complex",
  "estimated_files": 5-20,
  "estimated_loc": 200-2000,
  "structure": {{
    "directories": ["src", "tests", "config"],
    "key_files": ["main entry point", "core module", "config file"]
  }},
  "dependencies": ["list", "of", "realistic", "dependencies"],
  "features": [
    "Feature 1: Brief description",
    "Feature 2: Brief description"
  ],
  "vulnerability_opportunities": [
    {{
      "type": "sql_injection|xss|auth_bypass|path_traversal|race_condition|etc",
      "location": "where in the code this could naturally occur",
      "context": "why this vulnerability makes sense here"
    }}
  ],
  "test_scenarios": [
    "Scenario 1: What to test",
    "Scenario 2: What to test"
  ]
}}"#;

/// Programming language for workspace generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgrammingLanguage {
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Go,
    Java,
    #[serde(rename = "c")]
    C,
    #[serde(rename = "cpp")]
    Cpp,
}

impl ProgrammingLanguage {
    /// Returns all supported languages.
    pub fn all() -> Vec<ProgrammingLanguage> {
        vec![
            ProgrammingLanguage::Python,
            ProgrammingLanguage::Rust,
            ProgrammingLanguage::JavaScript,
            ProgrammingLanguage::TypeScript,
            ProgrammingLanguage::Go,
            ProgrammingLanguage::Java,
            ProgrammingLanguage::C,
            ProgrammingLanguage::Cpp,
        ]
    }

    /// Returns the display name for this language.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProgrammingLanguage::Python => "Python",
            ProgrammingLanguage::Rust => "Rust",
            ProgrammingLanguage::JavaScript => "JavaScript",
            ProgrammingLanguage::TypeScript => "TypeScript",
            ProgrammingLanguage::Go => "Go",
            ProgrammingLanguage::Java => "Java",
            ProgrammingLanguage::C => "C",
            ProgrammingLanguage::Cpp => "C++",
        }
    }

    /// Returns common file extensions for this language.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            ProgrammingLanguage::Python => &[".py"],
            ProgrammingLanguage::Rust => &[".rs"],
            ProgrammingLanguage::JavaScript => &[".js", ".mjs"],
            ProgrammingLanguage::TypeScript => &[".ts", ".tsx"],
            ProgrammingLanguage::Go => &[".go"],
            ProgrammingLanguage::Java => &[".java"],
            ProgrammingLanguage::C => &[".c", ".h"],
            ProgrammingLanguage::Cpp => &[".cpp", ".hpp", ".cc", ".hh"],
        }
    }
}

impl std::fmt::Display for ProgrammingLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Type of project to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    /// REST API or web service
    Api,
    /// Command-line interface tool
    Cli,
    /// Web application with frontend
    Web,
    /// Data processing pipeline
    Pipeline,
    /// Microservice component
    Microservice,
    /// Reusable library or SDK
    Library,
}

impl ProjectType {
    /// Returns all project types.
    pub fn all() -> Vec<ProjectType> {
        vec![
            ProjectType::Api,
            ProjectType::Cli,
            ProjectType::Web,
            ProjectType::Pipeline,
            ProjectType::Microservice,
            ProjectType::Library,
        ]
    }

    /// Returns the display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectType::Api => "REST API",
            ProjectType::Cli => "CLI Tool",
            ProjectType::Web => "Web Application",
            ProjectType::Pipeline => "Data Pipeline",
            ProjectType::Microservice => "Microservice",
            ProjectType::Library => "Library/SDK",
        }
    }
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Complexity level for workspace generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceComplexity {
    /// Simple project: 3-5 files, basic functionality
    Simple,
    /// Moderate project: 6-12 files, multiple features
    Moderate,
    /// Complex project: 13-20+ files, full-featured application
    Complex,
}

impl WorkspaceComplexity {
    /// Returns the expected file count range.
    pub fn file_range(&self) -> (usize, usize) {
        match self {
            WorkspaceComplexity::Simple => (3, 5),
            WorkspaceComplexity::Moderate => (6, 12),
            WorkspaceComplexity::Complex => (13, 20),
        }
    }

    /// Returns the expected lines of code range.
    pub fn loc_range(&self) -> (usize, usize) {
        match self {
            WorkspaceComplexity::Simple => (100, 400),
            WorkspaceComplexity::Moderate => (400, 1000),
            WorkspaceComplexity::Complex => (1000, 3000),
        }
    }
}

impl std::fmt::Display for WorkspaceComplexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceComplexity::Simple => write!(f, "simple"),
            WorkspaceComplexity::Moderate => write!(f, "moderate"),
            WorkspaceComplexity::Complex => write!(f, "complex"),
        }
    }
}

/// A potential vulnerability opportunity in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityOpportunity {
    /// Type of vulnerability that could be injected.
    pub vulnerability_type: String,
    /// Location in the code where it could occur.
    pub location: String,
    /// Context explaining why this vulnerability makes sense.
    pub context: String,
}

/// Project structure specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    /// Directory names to create.
    pub directories: Vec<String>,
    /// Key files that define the project.
    pub key_files: Vec<String>,
}

/// A generated workspace idea.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdea {
    /// Unique identifier for this idea.
    pub id: String,
    /// Project name (kebab-case).
    pub project_name: String,
    /// Description of the project.
    pub description: String,
    /// Primary programming language.
    pub language: ProgrammingLanguage,
    /// Framework or "none".
    pub framework: String,
    /// Type of project.
    pub project_type: ProjectType,
    /// Complexity level.
    pub complexity: WorkspaceComplexity,
    /// Estimated number of files.
    pub estimated_files: usize,
    /// Estimated lines of code.
    pub estimated_loc: usize,
    /// Project structure specification.
    pub structure: ProjectStructure,
    /// List of dependencies.
    pub dependencies: Vec<String>,
    /// Feature descriptions.
    pub features: Vec<String>,
    /// Vulnerability opportunities.
    pub vulnerability_opportunities: Vec<VulnerabilityOpportunity>,
    /// Test scenarios.
    pub test_scenarios: Vec<String>,
    /// Timestamp when created.
    pub created_at: DateTime<Utc>,
}

impl WorkspaceIdea {
    /// Creates a new workspace idea with required fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_name: impl Into<String>,
        description: impl Into<String>,
        language: ProgrammingLanguage,
        project_type: ProjectType,
        complexity: WorkspaceComplexity,
    ) -> Self {
        let (min_files, max_files) = complexity.file_range();
        let (min_loc, max_loc) = complexity.loc_range();

        Self {
            id: Uuid::new_v4().to_string(),
            project_name: project_name.into(),
            description: description.into(),
            language,
            framework: "none".to_string(),
            project_type,
            complexity,
            estimated_files: (min_files + max_files) / 2,
            estimated_loc: (min_loc + max_loc) / 2,
            structure: ProjectStructure {
                directories: Vec::new(),
                key_files: Vec::new(),
            },
            dependencies: Vec::new(),
            features: Vec::new(),
            vulnerability_opportunities: Vec::new(),
            test_scenarios: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Sets the framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = framework.into();
        self
    }

    /// Sets the structure.
    pub fn with_structure(mut self, structure: ProjectStructure) -> Self {
        self.structure = structure;
        self
    }

    /// Sets the dependencies.
    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Sets the features.
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }

    /// Sets the vulnerability opportunities.
    pub fn with_vulnerability_opportunities(
        mut self,
        opportunities: Vec<VulnerabilityOpportunity>,
    ) -> Self {
        self.vulnerability_opportunities = opportunities;
        self
    }
}

/// Configuration for the Workspace Ideator Agent.
#[derive(Debug, Clone)]
pub struct WorkspaceIdeatorConfig {
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Nucleus sampling parameter.
    pub top_p: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for WorkspaceIdeatorConfig {
    fn default() -> Self {
        Self {
            temperature: 0.9,
            top_p: 0.95,
            max_tokens: 4000,
        }
    }
}

impl WorkspaceIdeatorConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the top_p parameter.
    pub fn with_top_p(mut self, top_p: f64) -> Self {
        self.top_p = top_p.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Workspace Ideator Agent that generates realistic project ideas.
pub struct WorkspaceIdeatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: WorkspaceIdeatorConfig,
}

impl std::fmt::Debug for WorkspaceIdeatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceIdeatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl WorkspaceIdeatorAgent {
    /// Agent name constant.
    pub const AGENT_NAME: &'static str = "workspace_ideator";

    /// Creates a new workspace ideator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: WorkspaceIdeatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, WorkspaceIdeatorConfig::default())
    }

    /// Generates a workspace idea.
    ///
    /// # Arguments
    ///
    /// * `language` - Target programming language (optional, random if None)
    /// * `project_type` - Type of project (optional, random if None)
    /// * `complexity` - Complexity level (optional, defaults to Moderate)
    pub async fn generate_workspace_idea(
        &self,
        language: Option<ProgrammingLanguage>,
        project_type: Option<ProjectType>,
        complexity: Option<WorkspaceComplexity>,
    ) -> AgentResult<WorkspaceIdea> {
        let selected_language = language.unwrap_or_else(|| {
            let languages = ProgrammingLanguage::all();
            let index = (Utc::now().timestamp_millis() as usize) % languages.len();
            languages[index]
        });

        let selected_project_type = project_type.unwrap_or_else(|| {
            let types = ProjectType::all();
            let index = (Utc::now().timestamp_millis() as usize / 7) % types.len();
            types[index]
        });

        let selected_complexity = complexity.unwrap_or(WorkspaceComplexity::Moderate);

        let mut last_error = None;
        for attempt in 0..3 {
            match self
                .attempt_generate(
                    selected_language,
                    selected_project_type,
                    selected_complexity,
                )
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        language = ?selected_language,
                        project_type = ?selected_project_type,
                        "Workspace idea generation failed, retrying..."
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.expect("should have an error after 3 failed attempts"))
    }

    /// Attempts a single generation.
    async fn attempt_generate(
        &self,
        language: ProgrammingLanguage,
        project_type: ProjectType,
        complexity: WorkspaceComplexity,
    ) -> AgentResult<WorkspaceIdea> {
        let prompt = self.build_prompt(language, project_type, complexity);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(WORKSPACE_IDEATION_SYSTEM_PROMPT),
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

        self.parse_response(content)
    }

    /// Builds the user prompt.
    fn build_prompt(
        &self,
        language: ProgrammingLanguage,
        project_type: ProjectType,
        complexity: WorkspaceComplexity,
    ) -> String {
        WORKSPACE_IDEATION_USER_TEMPLATE
            .replace("{project_type}", project_type.display_name())
            .replace("{language}", language.display_name())
            .replace("{complexity}", &complexity.to_string())
    }

    /// Parses the LLM response into a WorkspaceIdea.
    fn parse_response(&self, content: &str) -> AgentResult<WorkspaceIdea> {
        let json_content = self.extract_json(content)?;

        let parsed: IdeaResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let language = parse_language(&parsed.language)?;
        let project_type = parse_project_type(&parsed.project_type)?;
        let complexity = parse_complexity(&parsed.complexity)?;

        let vulnerability_opportunities: Vec<VulnerabilityOpportunity> = parsed
            .vulnerability_opportunities
            .into_iter()
            .map(|v| VulnerabilityOpportunity {
                vulnerability_type: v.vulnerability_type,
                location: v.location,
                context: v.context,
            })
            .collect();

        let structure = ProjectStructure {
            directories: parsed.structure.directories,
            key_files: parsed.structure.key_files,
        };

        let idea = WorkspaceIdea::new(
            parsed.project_name,
            parsed.description,
            language,
            project_type,
            complexity,
        )
        .with_framework(parsed.framework)
        .with_structure(structure)
        .with_dependencies(parsed.dependencies)
        .with_features(parsed.features)
        .with_vulnerability_opportunities(vulnerability_opportunities);

        Ok(WorkspaceIdea {
            estimated_files: parsed.estimated_files,
            estimated_loc: parsed.estimated_loc,
            test_scenarios: parsed.test_scenarios,
            ..idea
        })
    }

    /// Extracts JSON from response.
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
                Err(AgentError::ResponseParseError(format!(
                    "JSON truncated: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                    unclosed_braces, unclosed_brackets, preview
                )))
            }
            crate::utils::json_extraction::JsonExtractionResult::NotFound => {
                let trimmed = content.trim();
                let preview_len = trimmed.len().min(100);
                let preview = &trimmed[..preview_len];
                Err(AgentError::ResponseParseError(format!(
                    "No JSON found in response. Content starts with: '{}'",
                    preview
                )))
            }
        }
    }

    /// Returns the configuration.
    pub fn config(&self) -> &WorkspaceIdeatorConfig {
        &self.config
    }
}

/// Response structure from LLM.
#[derive(Debug, Deserialize)]
struct IdeaResponse {
    project_name: String,
    description: String,
    language: String,
    framework: String,
    project_type: String,
    complexity: String,
    estimated_files: usize,
    estimated_loc: usize,
    structure: StructureResponse,
    dependencies: Vec<String>,
    features: Vec<String>,
    vulnerability_opportunities: Vec<VulnerabilityResponse>,
    test_scenarios: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StructureResponse {
    directories: Vec<String>,
    key_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VulnerabilityResponse {
    #[serde(rename = "type")]
    vulnerability_type: String,
    location: String,
    context: String,
}

/// Parses a language string.
fn parse_language(s: &str) -> AgentResult<ProgrammingLanguage> {
    match s.to_lowercase().as_str() {
        "python" => Ok(ProgrammingLanguage::Python),
        "rust" => Ok(ProgrammingLanguage::Rust),
        "javascript" | "js" => Ok(ProgrammingLanguage::JavaScript),
        "typescript" | "ts" => Ok(ProgrammingLanguage::TypeScript),
        "go" | "golang" => Ok(ProgrammingLanguage::Go),
        "java" => Ok(ProgrammingLanguage::Java),
        "c" => Ok(ProgrammingLanguage::C),
        "c++" | "cpp" => Ok(ProgrammingLanguage::Cpp),
        other => Err(AgentError::ResponseParseError(format!(
            "Unknown language: {}",
            other
        ))),
    }
}

/// Parses a project type string.
fn parse_project_type(s: &str) -> AgentResult<ProjectType> {
    match s.to_lowercase().as_str() {
        "api" | "rest api" => Ok(ProjectType::Api),
        "cli" | "cli tool" => Ok(ProjectType::Cli),
        "web" | "web application" => Ok(ProjectType::Web),
        "pipeline" | "data pipeline" => Ok(ProjectType::Pipeline),
        "microservice" => Ok(ProjectType::Microservice),
        "library" | "sdk" | "library/sdk" => Ok(ProjectType::Library),
        other => Err(AgentError::ResponseParseError(format!(
            "Unknown project type: {}",
            other
        ))),
    }
}

/// Parses a complexity string.
fn parse_complexity(s: &str) -> AgentResult<WorkspaceComplexity> {
    match s.to_lowercase().as_str() {
        "simple" => Ok(WorkspaceComplexity::Simple),
        "moderate" => Ok(WorkspaceComplexity::Moderate),
        "complex" => Ok(WorkspaceComplexity::Complex),
        other => Err(AgentError::ResponseParseError(format!(
            "Unknown complexity: {}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
    use std::sync::Mutex;

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

    fn mock_response() -> String {
        r#"{
            "project_name": "user-auth-api",
            "description": "A REST API for user authentication with JWT tokens and role-based access control.",
            "language": "Python",
            "framework": "FastAPI",
            "project_type": "api",
            "complexity": "moderate",
            "estimated_files": 8,
            "estimated_loc": 600,
            "structure": {
                "directories": ["src", "tests", "config"],
                "key_files": ["main.py", "auth.py", "models.py", "config.py"]
            },
            "dependencies": ["fastapi", "pydantic", "sqlalchemy", "pyjwt"],
            "features": [
                "User registration and login",
                "JWT token generation and validation",
                "Role-based access control"
            ],
            "vulnerability_opportunities": [
                {
                    "type": "sql_injection",
                    "location": "User lookup query in auth.py",
                    "context": "Direct string interpolation in SQL query for username lookup"
                }
            ],
            "test_scenarios": [
                "Test user registration flow",
                "Test invalid login attempts"
            ]
        }"#
        .to_string()
    }

    #[test]
    fn test_programming_language_all() {
        let languages = ProgrammingLanguage::all();
        assert_eq!(languages.len(), 8);
    }

    #[test]
    fn test_project_type_all() {
        let types = ProjectType::all();
        assert_eq!(types.len(), 6);
    }

    #[test]
    fn test_workspace_complexity_ranges() {
        assert_eq!(WorkspaceComplexity::Simple.file_range(), (3, 5));
        assert_eq!(WorkspaceComplexity::Moderate.file_range(), (6, 12));
        assert_eq!(WorkspaceComplexity::Complex.file_range(), (13, 20));
    }

    #[test]
    fn test_workspace_idea_creation() {
        let idea = WorkspaceIdea::new(
            "test-project",
            "A test project",
            ProgrammingLanguage::Python,
            ProjectType::Api,
            WorkspaceComplexity::Moderate,
        );

        assert!(!idea.id.is_empty());
        assert_eq!(idea.project_name, "test-project");
        assert_eq!(idea.language, ProgrammingLanguage::Python);
    }

    #[test]
    fn test_parse_language() {
        assert_eq!(
            parse_language("Python").unwrap(),
            ProgrammingLanguage::Python
        );
        assert_eq!(parse_language("rust").unwrap(), ProgrammingLanguage::Rust);
        assert_eq!(
            parse_language("JS").unwrap(),
            ProgrammingLanguage::JavaScript
        );
        assert!(parse_language("invalid").is_err());
    }

    #[test]
    fn test_parse_project_type() {
        assert_eq!(parse_project_type("api").unwrap(), ProjectType::Api);
        assert_eq!(parse_project_type("CLI").unwrap(), ProjectType::Cli);
        assert!(parse_project_type("invalid").is_err());
    }

    #[tokio::test]
    async fn test_generate_workspace_idea() {
        let mock_llm = Arc::new(MockLlmProvider::new(&mock_response()));
        let agent = WorkspaceIdeatorAgent::with_defaults(mock_llm);

        let idea = agent
            .generate_workspace_idea(
                Some(ProgrammingLanguage::Python),
                Some(ProjectType::Api),
                Some(WorkspaceComplexity::Moderate),
            )
            .await
            .expect("should generate idea");

        assert_eq!(idea.project_name, "user-auth-api");
        assert_eq!(idea.language, ProgrammingLanguage::Python);
        assert_eq!(idea.project_type, ProjectType::Api);
        assert!(!idea.vulnerability_opportunities.is_empty());
    }

    #[test]
    fn test_config_builder() {
        let config = WorkspaceIdeatorConfig::new()
            .with_temperature(0.8)
            .with_top_p(0.9)
            .with_max_tokens(3000);

        assert!((config.temperature - 0.8).abs() < 0.01);
        assert!((config.top_p - 0.9).abs() < 0.01);
        assert_eq!(config.max_tokens, 3000);
    }
}
