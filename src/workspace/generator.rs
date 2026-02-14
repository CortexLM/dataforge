//! Workspace generator for creating code workspaces with injected vulnerabilities.
//!
//! This module provides the main workspace generation functionality that:
//! - Takes a WorkspaceSpec and generates a complete codebase
//! - Coordinates the multi-agent pipeline for code generation
//! - Handles file system operations for workspace creation
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::workspace::{WorkspaceGenerator, WorkspaceSpec, WorkspaceLanguage, VulnerabilityType};
//! use swe_forge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! let llm = Arc::new(LiteLlmClient::from_env()?);
//! let generator = WorkspaceGenerator::new(llm);
//!
//! let spec = WorkspaceSpec::new("sql-injection-fix")
//!     .with_language(WorkspaceLanguage::Python)
//!     .with_vulnerability(VulnerabilityType::SqlInjection);
//!
//! let workspace = generator.generate(&spec).await?;
//! ```

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info, instrument};

use crate::error::GeneratorError;
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::cleaner::WorkspaceCleaner;
use super::types::{
    GeneratedWorkspace, InjectedVulnerability, VerificationScript, VulnerabilityType,
    WorkspaceFile, WorkspaceFileType, WorkspaceSpec,
};

// ============================================================================
// Generator Configuration
// ============================================================================

/// Configuration for workspace generation.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Model to use for code generation.
    pub model: String,
    /// Temperature for code generation (lower = more deterministic).
    pub temperature: f64,
    /// Maximum tokens for generation responses.
    pub max_tokens: u32,
    /// Whether to auto-clean generated workspaces.
    pub auto_clean: bool,
    /// Output directory for generated workspaces.
    pub output_dir: PathBuf,
    /// Whether to write files to disk during generation.
    pub write_to_disk: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4-20250514".to_string(),
            temperature: 0.3,
            max_tokens: 8192,
            auto_clean: true,
            output_dir: PathBuf::from("./generated-workspaces"),
            write_to_disk: true,
        }
    }
}

impl GeneratorConfig {
    /// Creates a new generator config with the specified model.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Sets the temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets whether to auto-clean workspaces.
    pub fn with_auto_clean(mut self, auto_clean: bool) -> Self {
        self.auto_clean = auto_clean;
        self
    }

    /// Sets the output directory.
    pub fn with_output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = dir.into();
        self
    }

    /// Sets whether to write files to disk.
    pub fn with_write_to_disk(mut self, write: bool) -> Self {
        self.write_to_disk = write;
        self
    }
}

// ============================================================================
// Generator Trait
// ============================================================================

/// Trait for workspace generators.
#[async_trait]
pub trait WorkspaceGen: Send + Sync {
    /// Generates a complete workspace from a specification.
    async fn generate(&self, spec: &WorkspaceSpec) -> Result<GeneratedWorkspace, GeneratorError>;

    /// Generates a workspace and writes it to disk.
    async fn generate_to_disk(
        &self,
        spec: &WorkspaceSpec,
        output_dir: &Path,
    ) -> Result<GeneratedWorkspace, GeneratorError>;
}

// ============================================================================
// Main Generator Implementation
// ============================================================================

/// Main workspace generator that uses LLM to create codebases with vulnerabilities.
pub struct WorkspaceGenerator<L: LlmProvider> {
    /// LLM provider for code generation.
    llm: Arc<L>,
    /// Generator configuration.
    config: GeneratorConfig,
    /// Workspace cleaner for removing hints.
    cleaner: WorkspaceCleaner,
}

impl<L: LlmProvider> WorkspaceGenerator<L> {
    /// Creates a new workspace generator with default configuration.
    pub fn new(llm: Arc<L>) -> Self {
        Self {
            llm,
            config: GeneratorConfig::default(),
            cleaner: WorkspaceCleaner::new(),
        }
    }

    /// Creates a new workspace generator with custom configuration.
    pub fn with_config(llm: Arc<L>, config: GeneratorConfig) -> Self {
        Self {
            llm,
            config,
            cleaner: WorkspaceCleaner::new(),
        }
    }

    /// Returns the generator configuration.
    pub fn config(&self) -> &GeneratorConfig {
        &self.config
    }

    /// Generates a project structure prompt for the LLM.
    fn build_structure_prompt(&self, spec: &WorkspaceSpec) -> String {
        let vuln_descriptions: Vec<String> = spec
            .vulnerability_types
            .iter()
            .map(|v| format!("- {}: {}", v.display_name(), v.description()))
            .collect();

        format!(
            r#"Generate a complete, realistic {language} project structure for a {project_type} application.

The project should:
1. Be a realistic, production-quality codebase
2. Include proper project structure and organization
3. Have realistic file names and directory structure
4. Include configuration files ({package_file})
5. Have test files with realistic test cases

Project details:
- Language: {language}
- Project type: {project_type}
- Difficulty: {difficulty}/10
- Description: {description}

The codebase should contain the following security vulnerabilities that need to be discovered and fixed:
{vulnerabilities}

IMPORTANT: 
- Generate realistic code that looks like actual production code
- The vulnerabilities should be subtle and realistic, not obvious
- Do NOT include any comments or hints about where the vulnerabilities are
- Do NOT include TODO, FIXME, or any comments mentioning vulnerabilities
- The code should compile/run without errors

Output the project structure as a JSON array of file objects with this format:
[
  {{"path": "relative/path/to/file.ext", "content": "file content here", "type": "source|test|config"}}
]

Only output valid JSON, no markdown code fences or explanations."#,
            language = spec.language.display_name(),
            project_type = spec.project_type,
            package_file = spec.language.package_file(),
            difficulty = spec.difficulty,
            description = if spec.description.is_empty() {
                "A typical application"
            } else {
                &spec.description
            },
            vulnerabilities = vuln_descriptions.join("\n")
        )
    }

    /// Generates a prompt for creating verification scripts.
    fn build_verification_prompt(&self, spec: &WorkspaceSpec, files: &[WorkspaceFile]) -> String {
        let file_list: Vec<String> = files
            .iter()
            .filter(|f| f.file_type == WorkspaceFileType::Source)
            .map(|f| f.path.display().to_string())
            .collect();

        let vuln_descriptions: Vec<String> = spec
            .vulnerability_types
            .iter()
            .map(|v| format!("- {}", v.display_name()))
            .collect();

        format!(
            r##"Create verification test scripts that will:
1. FAIL when the vulnerabilities are present (initial state)
2. PASS when the vulnerabilities are fixed correctly

Project language: {language}
Test command: {test_cmd}

Files in the project:
{files}

Vulnerabilities to test for:
{vulnerabilities}

Generate bash test scripts that:
- Test for each vulnerability type
- Return exit code 0 when the code is secure (vulnerability fixed)
- Return exit code 1 when the code is vulnerable

Output as JSON array:
[
  {{"name": "test_sql_injection.sh", "content": "bash script content here...", "description": "Tests for SQL injection", "tests_vulns": ["sql_injection"]}}
]

Only output valid JSON."##,
            language = spec.language.display_name(),
            test_cmd = spec.language.test_command(),
            files = file_list.join("\n"),
            vulnerabilities = vuln_descriptions.join("\n")
        )
    }

    /// Generates a task prompt for the agent.
    fn build_task_prompt(&self, spec: &WorkspaceSpec) -> String {
        let vuln_hints: Vec<String> = spec
            .vulnerability_types
            .iter()
            .map(|v| format!("- {}", v.display_name()))
            .collect();

        format!(
            r#"# Security Code Review Task

You are reviewing a {language} {project_type} codebase for security vulnerabilities.

## Objective
Find and fix all security vulnerabilities in this codebase.

## Potential Vulnerability Types
The codebase may contain vulnerabilities such as:
{hints}

## Instructions
1. Analyze all source files for security issues
2. Identify the specific vulnerabilities
3. Apply fixes that follow security best practices
4. Ensure all tests pass after your fixes
5. Do not break existing functionality

## Verification
Run the test suite to verify your fixes:
```bash
{test_cmd}
```

Good luck!"#,
            language = spec.language.display_name(),
            project_type = spec.project_type,
            hints = vuln_hints.join("\n"),
            test_cmd = spec.language.test_command()
        )
    }

    /// Parses the LLM response for file generation.
    fn parse_files_response(&self, response: &str) -> Result<Vec<WorkspaceFile>, GeneratorError> {
        // Try to extract JSON from the response
        let json_str = self.extract_json_array(response)?;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).map_err(|e| {
            GeneratorError::InvalidParameter(format!("Failed to parse JSON: {}", e))
        })?;

        let mut files = Vec::new();
        for item in parsed {
            let path = item
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GeneratorError::MissingParameter("path".to_string()))?;

            let content = item
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GeneratorError::MissingParameter("content".to_string()))?;

            let file_type_str = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("source");

            let file_type = match file_type_str {
                "test" => WorkspaceFileType::Test,
                "config" | "configuration" => WorkspaceFileType::Configuration,
                "build" => WorkspaceFileType::BuildScript,
                "doc" | "documentation" => WorkspaceFileType::Documentation,
                "data" => WorkspaceFileType::Data,
                _ => WorkspaceFileType::Source,
            };

            files.push(WorkspaceFile::new(path, content).with_type(file_type));
        }

        if files.is_empty() {
            return Err(GeneratorError::InvalidParameter(
                "No files generated".to_string(),
            ));
        }

        Ok(files)
    }

    /// Parses the LLM response for verification scripts.
    fn parse_verification_response(
        &self,
        response: &str,
    ) -> Result<Vec<VerificationScript>, GeneratorError> {
        let json_str = self.extract_json_array(response)?;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).map_err(|e| {
            GeneratorError::InvalidParameter(format!("Failed to parse JSON: {}", e))
        })?;

        let mut scripts = Vec::new();
        for item in parsed {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GeneratorError::MissingParameter("name".to_string()))?;

            let content = item
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| GeneratorError::MissingParameter("content".to_string()))?;

            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let tests_vulns: Vec<String> = item
                .get("tests_vulns")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            scripts.push(
                VerificationScript::bash(name, content)
                    .with_description(description)
                    .with_tests_vulnerabilities(tests_vulns),
            );
        }

        Ok(scripts)
    }

    /// Extracts a JSON array from potentially wrapped response.
    fn extract_json_array(&self, response: &str) -> Result<String, GeneratorError> {
        let trimmed = response.trim();

        // Check if it's already valid JSON
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return Ok(trimmed.to_string());
        }

        // Try to find JSON array in markdown code blocks
        if let Some(start) = trimmed.find("```json") {
            if let Some(end) = trimmed[start..]
                .find("```\n")
                .or(trimmed[start..].rfind("```"))
            {
                let json_start = start + 7; // Skip "```json"
                let actual_end = start + end;
                let json_content = trimmed[json_start..actual_end].trim();
                if json_content.starts_with('[') {
                    return Ok(json_content.to_string());
                }
            }
        }

        // Try to find bare JSON array
        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                return Ok(trimmed[start..=end].to_string());
            }
        }

        Err(GeneratorError::InvalidParameter(
            "Could not find JSON array in response".to_string(),
        ))
    }

    /// Creates placeholder injected vulnerabilities based on spec.
    fn create_vulnerability_records(
        &self,
        spec: &WorkspaceSpec,
        files: &[WorkspaceFile],
    ) -> Vec<InjectedVulnerability> {
        let mut vulnerabilities = Vec::new();

        for vuln_type in &spec.vulnerability_types {
            // Find a relevant source file
            let relevant_file = files
                .iter()
                .filter(|f| f.file_type == WorkspaceFileType::Source)
                .find(|f| {
                    let path_str = f.path.display().to_string().to_lowercase();
                    match vuln_type {
                        VulnerabilityType::SqlInjection => {
                            path_str.contains("db")
                                || path_str.contains("database")
                                || path_str.contains("query")
                                || path_str.contains("model")
                        }
                        VulnerabilityType::Xss => {
                            path_str.contains("view")
                                || path_str.contains("template")
                                || path_str.contains("render")
                                || path_str.contains("html")
                        }
                        VulnerabilityType::AuthenticationBypass => {
                            path_str.contains("auth")
                                || path_str.contains("login")
                                || path_str.contains("user")
                                || path_str.contains("session")
                        }
                        VulnerabilityType::PathTraversal => {
                            path_str.contains("file")
                                || path_str.contains("upload")
                                || path_str.contains("download")
                                || path_str.contains("storage")
                        }
                        VulnerabilityType::CommandInjection => {
                            path_str.contains("exec")
                                || path_str.contains("shell")
                                || path_str.contains("command")
                                || path_str.contains("process")
                        }
                        _ => true, // Default to first source file
                    }
                })
                .or_else(|| {
                    files
                        .iter()
                        .find(|f| f.file_type == WorkspaceFileType::Source)
                });

            if let Some(file) = relevant_file {
                vulnerabilities.push(
                    InjectedVulnerability::new(*vuln_type, file.path.clone(), (1, 10))
                        .with_description(format!(
                            "{} vulnerability in {}",
                            vuln_type.display_name(),
                            file.path.display()
                        )),
                );
            }
        }

        vulnerabilities
    }

    /// Writes workspace files to disk.
    #[instrument(skip(self, workspace))]
    async fn write_workspace_to_disk(
        &self,
        workspace: &GeneratedWorkspace,
        output_dir: &Path,
    ) -> Result<(), GeneratorError> {
        let workspace_dir = output_dir.join(&workspace.id);
        fs::create_dir_all(&workspace_dir).await?;

        info!("Writing workspace to {}", workspace_dir.display());

        // Write all files
        for file in &workspace.files {
            let file_path = workspace_dir.join(&file.path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::write(&file_path, &file.content).await?;
            debug!("Wrote file: {}", file_path.display());

            // Set executable permission if needed
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&file_path).await?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&file_path, perms).await?;
            }
        }

        // Write verification scripts
        let scripts_dir = workspace_dir.join(".verification");
        fs::create_dir_all(&scripts_dir).await?;

        for script in &workspace.verification_scripts {
            let script_path = scripts_dir.join(&script.name);
            let content = format!("{}\n{}", script.shebang(), script.content);
            fs::write(&script_path, content).await?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&script_path).await?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&script_path, perms).await?;
            }
        }

        // Write task prompt
        fs::write(workspace_dir.join("prompt.md"), &workspace.task_prompt).await?;

        // Write task.yaml
        let task_yaml = serde_yaml::to_string(&workspace.spec).map_err(GeneratorError::Yaml)?;
        fs::write(workspace_dir.join("task.yaml"), task_yaml).await?;

        // Write canary
        fs::write(workspace_dir.join(".canary"), &workspace.canary_token).await?;

        // Write solution (hidden)
        let solution_dir = workspace_dir.join(".solution");
        fs::create_dir_all(&solution_dir).await?;
        fs::write(
            solution_dir.join("description.md"),
            &workspace.solution_description,
        )
        .await?;

        info!("Workspace written successfully");
        Ok(())
    }
}

#[async_trait]
impl<L: LlmProvider + 'static> WorkspaceGen for WorkspaceGenerator<L> {
    #[instrument(skip(self))]
    async fn generate(&self, spec: &WorkspaceSpec) -> Result<GeneratedWorkspace, GeneratorError> {
        info!("Generating workspace: {}", spec.id);

        // Validate the spec
        spec.validate().map_err(GeneratorError::InvalidParameter)?;

        // Generate project structure
        let structure_prompt = self.build_structure_prompt(spec);
        debug!("Structure prompt length: {} chars", structure_prompt.len());

        let structure_request = GenerationRequest::new(
            &self.config.model,
            vec![
                Message::system("You are an expert software developer. Generate realistic, production-quality code. Output only valid JSON arrays as requested."),
                Message::user(structure_prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let structure_response = self
            .llm
            .generate(structure_request)
            .await
            .map_err(|e| GeneratorError::Template(format!("LLM generation failed: {}", e)))?;

        let response_content = structure_response
            .first_content()
            .ok_or_else(|| GeneratorError::Template("Empty LLM response".to_string()))?;

        debug!("Received response of {} chars", response_content.len());

        // Parse files from response
        let mut files = self.parse_files_response(response_content)?;
        info!("Generated {} files", files.len());

        // Generate verification scripts
        let verification_prompt = self.build_verification_prompt(spec, &files);
        let verification_request = GenerationRequest::new(
            &self.config.model,
            vec![
                Message::system("You are a security testing expert. Generate verification test scripts. Output only valid JSON arrays."),
                Message::user(verification_prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens / 2);

        let verification_response = self.llm.generate(verification_request).await.map_err(|e| {
            GeneratorError::Template(format!("Verification generation failed: {}", e))
        })?;

        let verification_scripts = verification_response
            .first_content()
            .map(|content| self.parse_verification_response(content))
            .transpose()?
            .unwrap_or_default();

        info!(
            "Generated {} verification scripts",
            verification_scripts.len()
        );

        // Create vulnerability records
        let vulnerabilities = self.create_vulnerability_records(spec, &files);

        // Build the workspace
        let mut workspace = GeneratedWorkspace::new(spec.clone());
        workspace.add_files(files.drain(..));
        for vuln in vulnerabilities {
            workspace.add_vulnerability(vuln);
        }
        for script in verification_scripts {
            workspace.add_verification_script(script);
        }
        workspace.task_prompt = self.build_task_prompt(spec);
        workspace.solution_description = format!(
            "Fix the following vulnerabilities:\n{}",
            spec.vulnerability_types
                .iter()
                .map(|v| format!("- {}: {}", v.display_name(), v.description()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // Clean the workspace if configured
        if self.config.auto_clean {
            debug!("Auto-cleaning workspace");
            let cleaned = self.cleaner.clean(&workspace)?;
            return Ok(cleaned);
        }

        Ok(workspace)
    }

    async fn generate_to_disk(
        &self,
        spec: &WorkspaceSpec,
        output_dir: &Path,
    ) -> Result<GeneratedWorkspace, GeneratorError> {
        let workspace = self.generate(spec).await?;
        self.write_workspace_to_disk(&workspace, output_dir).await?;
        Ok(workspace)
    }
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating WorkspaceGenerator instances.
pub struct WorkspaceGeneratorBuilder<L: LlmProvider> {
    llm: Arc<L>,
    config: GeneratorConfig,
}

impl<L: LlmProvider> WorkspaceGeneratorBuilder<L> {
    /// Creates a new builder with the given LLM provider.
    pub fn new(llm: Arc<L>) -> Self {
        Self {
            llm,
            config: GeneratorConfig::default(),
        }
    }

    /// Sets the model to use.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Sets the temperature.
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.config.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets max tokens.
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.config.max_tokens = max_tokens;
        self
    }

    /// Sets auto-clean option.
    pub fn auto_clean(mut self, auto_clean: bool) -> Self {
        self.config.auto_clean = auto_clean;
        self
    }

    /// Sets output directory.
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.output_dir = dir.into();
        self
    }

    /// Sets write to disk option.
    pub fn write_to_disk(mut self, write: bool) -> Self {
        self.config.write_to_disk = write;
        self
    }

    /// Builds the generator.
    pub fn build(self) -> WorkspaceGenerator<L> {
        WorkspaceGenerator::with_config(self.llm, self.config)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceLanguage;

    #[test]
    fn test_generator_config_defaults() {
        let config = GeneratorConfig::default();
        assert!(!config.model.is_empty());
        assert!(config.temperature > 0.0 && config.temperature < 1.0);
        assert!(config.max_tokens > 0);
        assert!(config.auto_clean);
    }

    #[test]
    fn test_generator_config_builder() {
        let config = GeneratorConfig::new("test-model")
            .with_temperature(0.5)
            .with_max_tokens(4096)
            .with_auto_clean(false);

        assert_eq!(config.model, "test-model");
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.max_tokens, 4096);
        assert!(!config.auto_clean);
    }

    #[test]
    fn test_temperature_clamping() {
        let config1 = GeneratorConfig::default().with_temperature(3.0);
        assert_eq!(config1.temperature, 2.0);

        let config2 = GeneratorConfig::default().with_temperature(-1.0);
        assert_eq!(config2.temperature, 0.0);
    }

    #[test]
    fn test_extract_json_array() {
        struct MockProvider;

        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn generate(
                &self,
                _request: GenerationRequest,
            ) -> Result<crate::llm::GenerationResponse, crate::error::LlmError> {
                unreachable!("Mock should not be called in this test")
            }
        }

        let generator = WorkspaceGenerator::new(Arc::new(MockProvider));

        // Test direct JSON
        let json = generator.extract_json_array(r#"[{"path": "test.py"}]"#);
        assert!(json.is_ok());

        // Test with whitespace
        let json = generator.extract_json_array(r#"  [{"path": "test.py"}]  "#);
        assert!(json.is_ok());

        // Test with markdown code block
        let json = generator.extract_json_array(
            r#"Here's the code:
```json
[{"path": "test.py"}]
```"#,
        );
        assert!(json.is_ok());

        // Test finding array in text
        let json =
            generator.extract_json_array(r#"Here is the output: [{"path": "test.py"}] done"#);
        assert!(json.is_ok());

        // Test invalid input
        let json = generator.extract_json_array("no json here");
        assert!(json.is_err());
    }

    #[test]
    fn test_build_structure_prompt() {
        struct MockProvider;

        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn generate(
                &self,
                _request: GenerationRequest,
            ) -> Result<crate::llm::GenerationResponse, crate::error::LlmError> {
                unreachable!("Mock should not be called in this test")
            }
        }

        let generator = WorkspaceGenerator::new(Arc::new(MockProvider));
        let spec = WorkspaceSpec::new("test")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection);

        let prompt = generator.build_structure_prompt(&spec);

        assert!(prompt.contains("Python"));
        assert!(prompt.contains("SQL Injection"));
        assert!(prompt.contains("requirements.txt"));
    }

    #[test]
    fn test_parse_files_response() {
        struct MockProvider;

        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn generate(
                &self,
                _request: GenerationRequest,
            ) -> Result<crate::llm::GenerationResponse, crate::error::LlmError> {
                unreachable!("Mock should not be called in this test")
            }
        }

        let generator = WorkspaceGenerator::new(Arc::new(MockProvider));

        let response = r#"[
            {"path": "src/main.py", "content": "print('hello')", "type": "source"},
            {"path": "tests/test_main.py", "content": "def test(): pass", "type": "test"}
        ]"#;

        let files = generator.parse_files_response(response).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_type, WorkspaceFileType::Source);
        assert_eq!(files[1].file_type, WorkspaceFileType::Test);
    }

    #[test]
    fn test_create_vulnerability_records() {
        struct MockProvider;

        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn generate(
                &self,
                _request: GenerationRequest,
            ) -> Result<crate::llm::GenerationResponse, crate::error::LlmError> {
                unreachable!("Mock should not be called in this test")
            }
        }

        let generator = WorkspaceGenerator::new(Arc::new(MockProvider));
        let spec = WorkspaceSpec::new("test")
            .with_vulnerability(VulnerabilityType::SqlInjection)
            .with_vulnerability(VulnerabilityType::Xss);

        let files = vec![
            WorkspaceFile::source("src/db.py", "# db code"),
            WorkspaceFile::source("src/views.py", "# view code"),
        ];

        let vulns = generator.create_vulnerability_records(&spec, &files);
        assert_eq!(vulns.len(), 2);
    }
}
