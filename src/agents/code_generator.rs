//! Code Generator Agent for creating complete, working code workspaces.
//!
//! This agent takes a workspace specification and generates clean, functional
//! code that forms the base for vulnerability injection. The generated code
//! should be realistic, follow best practices, and compile/run successfully.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::{AgentError, AgentResult};
use super::workspace_ideator::{ProgrammingLanguage, WorkspaceIdea};
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for code generation.
const CODE_GENERATION_SYSTEM_PROMPT: &str = r#"You are an expert software developer generating COMPLETE, PRODUCTION-QUALITY code.

Your task is to generate a complete, working codebase based on the provided specification.

CRITICAL REQUIREMENTS:
1. Generate COMPLETE code - no placeholders, no "..." ellipsis, no TODO comments
2. Code must be FUNCTIONAL - it should compile/run successfully
3. Follow language-specific best practices and conventions
4. Include proper error handling throughout
5. Use realistic variable and function names
6. Include necessary imports and dependencies
7. Structure code logically with proper separation of concerns

CODE QUALITY STANDARDS:
- Python: Follow PEP 8, use type hints, proper docstrings
- Rust: Follow Rust idioms, proper error handling with Result, documentation
- JavaScript/TypeScript: ESLint compliant, JSDoc comments, proper async/await
- Go: Follow Go conventions, proper error handling, gofmt compatible
- Java: Follow Java conventions, proper exception handling, Javadoc
- C/C++: Proper memory management, include guards, documentation

FILE ORGANIZATION:
- Main entry point should be clearly identifiable
- Separate concerns into appropriate modules/files
- Configuration should be separate from business logic
- Tests should be in a dedicated test directory/file

DO NOT:
- Use placeholder comments like "// implement this" or Python TODO comments
- Leave functions empty or with just "pass" or "return"
- Skip error handling
- Use meaningless variable names like 'x', 'temp', 'data'
- Include debugging print statements
- Hardcode sensitive values (use environment variables or config)

The code should look like it was written by a professional developer for a real project."#;

/// User prompt template for code generation.
const CODE_GENERATION_USER_TEMPLATE: &str = r#"Generate a complete, working codebase for the following project:

Project Name: {project_name}
Description: {description}
Language: {language}
Framework: {framework}
Project Type: {project_type}
Complexity: {complexity}

Structure:
- Directories: {directories}
- Key Files: {key_files}

Dependencies: {dependencies}

Features to Implement:
{features}

Generate the complete codebase. You MUST respond with ONLY valid JSON in this exact format:
{{
  "files": [
    {{
      "path": "relative/path/to/file.ext",
      "content": "complete file content here",
      "description": "brief description of this file's purpose"
    }}
  ],
  "build_instructions": "how to build/run this project",
  "test_instructions": "how to run tests"
}}"#;

/// A generated code file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// Relative path within the workspace.
    pub path: String,
    /// Complete file content.
    pub content: String,
    /// Description of the file's purpose.
    pub description: String,
}

impl GeneratedFile {
    /// Creates a new generated file.
    pub fn new(
        path: impl Into<String>,
        content: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            description: description.into(),
        }
    }

    /// Returns the file extension.
    pub fn extension(&self) -> Option<&str> {
        self.path.rsplit('.').next()
    }

    /// Returns the filename without path.
    pub fn filename(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Returns the line count.
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }
}

/// A complete generated workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkspace {
    /// Unique identifier.
    pub id: String,
    /// Source workspace idea ID.
    pub source_idea_id: String,
    /// Project name.
    pub project_name: String,
    /// Programming language.
    pub language: ProgrammingLanguage,
    /// Generated files.
    pub files: Vec<GeneratedFile>,
    /// Build instructions.
    pub build_instructions: String,
    /// Test instructions.
    pub test_instructions: String,
    /// Total lines of code.
    pub total_loc: usize,
    /// Metadata about generation.
    pub metadata: WorkspaceMetadata,
    /// Timestamp when created.
    pub created_at: DateTime<Utc>,
}

impl GeneratedWorkspace {
    /// Creates a new generated workspace.
    pub fn new(
        source_idea_id: impl Into<String>,
        project_name: impl Into<String>,
        language: ProgrammingLanguage,
        files: Vec<GeneratedFile>,
    ) -> Self {
        let total_loc: usize = files.iter().map(|f| f.line_count()).sum();

        Self {
            id: Uuid::new_v4().to_string(),
            source_idea_id: source_idea_id.into(),
            project_name: project_name.into(),
            language,
            files,
            build_instructions: String::new(),
            test_instructions: String::new(),
            total_loc,
            metadata: WorkspaceMetadata::default(),
            created_at: Utc::now(),
        }
    }

    /// Sets build instructions.
    pub fn with_build_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.build_instructions = instructions.into();
        self
    }

    /// Sets test instructions.
    pub fn with_test_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.test_instructions = instructions.into();
        self
    }

    /// Sets metadata.
    pub fn with_metadata(mut self, metadata: WorkspaceMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns the number of files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Gets a file by path.
    pub fn get_file(&self, path: &str) -> Option<&GeneratedFile> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Gets a mutable file by path.
    pub fn get_file_mut(&mut self, path: &str) -> Option<&mut GeneratedFile> {
        self.files.iter_mut().find(|f| f.path == path)
    }

    /// Returns file paths.
    pub fn file_paths(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.path.as_str()).collect()
    }
}

/// Metadata about workspace generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    /// Generation model used.
    pub model: String,
    /// Generation timestamp.
    pub generated_at: Option<DateTime<Utc>>,
    /// Generation duration in milliseconds.
    pub generation_duration_ms: Option<u64>,
    /// Any warnings during generation.
    pub warnings: Vec<String>,
    /// Custom metadata fields.
    pub custom: HashMap<String, String>,
}

impl WorkspaceMetadata {
    /// Creates new metadata.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            generated_at: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Adds a warning.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Adds custom metadata.
    pub fn add_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.custom.insert(key.into(), value.into());
    }
}

/// Configuration for the Code Generator Agent.
#[derive(Debug, Clone)]
pub struct CodeGeneratorConfig {
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for response.
    pub max_tokens: u32,
    /// Whether to validate generated code syntax.
    pub validate_syntax: bool,
}

impl Default for CodeGeneratorConfig {
    fn default() -> Self {
        Self {
            temperature: 0.3,
            max_tokens: 16000,
            validate_syntax: true,
        }
    }
}

impl CodeGeneratorConfig {
    /// Creates new configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets syntax validation.
    pub fn with_validate_syntax(mut self, validate: bool) -> Self {
        self.validate_syntax = validate;
        self
    }
}

/// Code Generator Agent that creates complete workspaces.
pub struct CodeGeneratorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: CodeGeneratorConfig,
}

impl std::fmt::Debug for CodeGeneratorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeGeneratorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CodeGeneratorAgent {
    /// Agent name constant.
    pub const AGENT_NAME: &'static str = "code_generator";

    /// Creates a new code generator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: CodeGeneratorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, CodeGeneratorConfig::default())
    }

    /// Generates a complete workspace from an idea.
    pub async fn generate_workspace(
        &self,
        idea: &WorkspaceIdea,
    ) -> AgentResult<GeneratedWorkspace> {
        let start_time = std::time::Instant::now();

        let mut last_error = None;
        for attempt in 0..3 {
            match self.attempt_generate(idea).await {
                Ok(mut workspace) => {
                    workspace.metadata.generation_duration_ms =
                        Some(start_time.elapsed().as_millis() as u64);
                    return Ok(workspace);
                }
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        project = %idea.project_name,
                        "Workspace generation failed, retrying..."
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.expect("should have an error after 3 failed attempts"))
    }

    /// Attempts a single generation.
    async fn attempt_generate(&self, idea: &WorkspaceIdea) -> AgentResult<GeneratedWorkspace> {
        let prompt = self.build_prompt(idea);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(CODE_GENERATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_response(content, idea)
    }

    /// Builds the user prompt.
    fn build_prompt(&self, idea: &WorkspaceIdea) -> String {
        let features_str = idea
            .features
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n");

        WORKSPACE_GENERATION_USER_TEMPLATE
            .replace("{project_name}", &idea.project_name)
            .replace("{description}", &idea.description)
            .replace("{language}", idea.language.display_name())
            .replace("{framework}", &idea.framework)
            .replace("{project_type}", idea.project_type.display_name())
            .replace("{complexity}", &idea.complexity.to_string())
            .replace("{directories}", &idea.structure.directories.join(", "))
            .replace("{key_files}", &idea.structure.key_files.join(", "))
            .replace("{dependencies}", &idea.dependencies.join(", "))
            .replace("{features}", &features_str)
    }

    /// Parses the LLM response.
    fn parse_response(
        &self,
        content: &str,
        idea: &WorkspaceIdea,
    ) -> AgentResult<GeneratedWorkspace> {
        let json_content = self.extract_json(content)?;

        let parsed: GenerationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let files: Vec<GeneratedFile> = parsed
            .files
            .into_iter()
            .map(|f| GeneratedFile::new(f.path, f.content, f.description))
            .collect();

        if files.is_empty() {
            return Err(AgentError::GenerationFailed(
                "No files generated".to_string(),
            ));
        }

        let metadata = WorkspaceMetadata::new("unknown");

        let workspace = GeneratedWorkspace::new(&idea.id, &idea.project_name, idea.language, files)
            .with_build_instructions(parsed.build_instructions)
            .with_test_instructions(parsed.test_instructions)
            .with_metadata(metadata);

        Ok(workspace)
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
    pub fn config(&self) -> &CodeGeneratorConfig {
        &self.config
    }
}

/// Alternative user template constant for use in build_prompt.
const WORKSPACE_GENERATION_USER_TEMPLATE: &str = CODE_GENERATION_USER_TEMPLATE;

/// Response structure from LLM.
#[derive(Debug, Deserialize)]
struct GenerationResponse {
    files: Vec<FileResponse>,
    build_instructions: String,
    test_instructions: String,
}

#[derive(Debug, Deserialize)]
struct FileResponse {
    path: String,
    content: String,
    description: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::llm::{Choice, GenerationResponse as LlmGenResponse, Usage};
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
        async fn generate(&self, _request: GenerationRequest) -> Result<LlmGenResponse, LlmError> {
            let content = self.response.lock().expect("lock poisoned").clone();
            Ok(LlmGenResponse {
                id: "test-id".to_string(),
                model: "test-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 500,
                    total_tokens: 600,
                },
            })
        }
    }

    fn mock_response() -> String {
        serde_json::json!({
            "files": [
                {
                    "path": "src/main.py",
                    "content": "#!/usr/bin/env python3\n\nfrom fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/\")\ndef root():\n    return {\"message\": \"Hello World\"}\n",
                    "description": "Main application entry point"
                },
                {
                    "path": "requirements.txt",
                    "content": "fastapi>=0.100.0\nuvicorn>=0.22.0\n",
                    "description": "Python dependencies"
                }
            ],
            "build_instructions": "pip install -r requirements.txt",
            "test_instructions": "pytest tests/"
        })
        .to_string()
    }

    #[test]
    fn test_generated_file() {
        let file = GeneratedFile::new("src/main.py", "print('hello')", "Main file");

        assert_eq!(file.path, "src/main.py");
        assert_eq!(file.extension(), Some("py"));
        assert_eq!(file.filename(), "main.py");
        assert_eq!(file.line_count(), 1);
    }

    #[test]
    fn test_generated_workspace() {
        let files = vec![
            GeneratedFile::new("main.py", "line1\nline2\nline3", "Main"),
            GeneratedFile::new("utils.py", "helper", "Utils"),
        ];

        let workspace = GeneratedWorkspace::new(
            "idea-123",
            "test-project",
            ProgrammingLanguage::Python,
            files,
        );

        assert_eq!(workspace.file_count(), 2);
        assert_eq!(workspace.total_loc, 4);
        assert!(workspace.get_file("main.py").is_some());
        assert!(workspace.get_file("nonexistent.py").is_none());
    }

    #[test]
    fn test_workspace_metadata() {
        let mut metadata = WorkspaceMetadata::new("gpt-4");
        metadata.add_warning("Some warning");
        metadata.add_custom("key", "value");

        assert_eq!(metadata.model, "gpt-4");
        assert_eq!(metadata.warnings.len(), 1);
        assert_eq!(metadata.custom.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_config_builder() {
        let config = CodeGeneratorConfig::new()
            .with_temperature(0.5)
            .with_max_tokens(8000)
            .with_validate_syntax(false);

        assert!((config.temperature - 0.5).abs() < 0.01);
        assert_eq!(config.max_tokens, 8000);
        assert!(!config.validate_syntax);
    }

    #[tokio::test]
    async fn test_generate_workspace() {
        use super::super::workspace_ideator::{
            ProjectStructure, ProjectType, WorkspaceComplexity, WorkspaceIdea,
        };

        let mock_llm = Arc::new(MockLlmProvider::new(&mock_response()));
        let agent = CodeGeneratorAgent::with_defaults(mock_llm);

        let idea = WorkspaceIdea::new(
            "test-api",
            "A test API",
            ProgrammingLanguage::Python,
            ProjectType::Api,
            WorkspaceComplexity::Simple,
        )
        .with_framework("FastAPI")
        .with_structure(ProjectStructure {
            directories: vec!["src".to_string()],
            key_files: vec!["main.py".to_string()],
        })
        .with_dependencies(vec!["fastapi".to_string()])
        .with_features(vec!["Basic endpoint".to_string()]);

        let workspace = agent
            .generate_workspace(&idea)
            .await
            .expect("should generate workspace");

        assert_eq!(workspace.project_name, "test-api");
        assert_eq!(workspace.file_count(), 2);
        assert!(workspace.get_file("src/main.py").is_some());
    }
}
