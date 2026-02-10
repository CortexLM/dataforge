//! Workspace export functionality.
//!
//! This module provides functionality to export generated workspaces:
//! - Export as .zip archives
//! - Exclude common build artifacts
//! - Generate task.yaml, prompt.md, and verification scripts
//!
//! # Example
//!
//! ```ignore
//! use dataforge::workspace::{WorkspaceExporter, GeneratedWorkspace};
//! use std::path::Path;
//!
//! let exporter = WorkspaceExporter::new()
//!     .with_exclude_patterns(vec!["*.pyc", "__pycache__"]);
//!
//! let workspace: GeneratedWorkspace = // ... generate workspace
//! exporter.export_to_zip(&workspace, Path::new("output/workspace.zip")).await?;
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, instrument};

use crate::error::ExportError;

use super::types::{GeneratedWorkspace, WorkspaceLanguage};

// ============================================================================
// Default Exclusion Patterns
// ============================================================================

/// Returns a list of common build artifacts to exclude.
pub fn default_exclude_patterns() -> Vec<&'static str> {
    vec![
        // Version control
        ".git",
        ".gitignore",
        ".gitattributes",
        ".svn",
        ".hg",
        // Python
        "__pycache__",
        "*.pyc",
        "*.pyo",
        "*.pyd",
        ".pytest_cache",
        ".mypy_cache",
        ".tox",
        ".venv",
        "venv",
        "env",
        "*.egg-info",
        ".eggs",
        "dist",
        "build",
        // JavaScript/TypeScript
        "node_modules",
        ".npm",
        ".yarn",
        "bower_components",
        ".next",
        ".nuxt",
        ".cache",
        "coverage",
        // Rust
        "target",
        "Cargo.lock",
        // Go
        "vendor",
        "bin",
        // Java
        "*.class",
        "*.jar",
        "*.war",
        ".gradle",
        // C/C++
        "*.o",
        "*.a",
        "*.so",
        "*.dylib",
        "*.dll",
        "cmake-build-*",
        // Ruby
        ".bundle",
        // PHP
        "vendor",
        "composer.lock",
        // IDE/Editor
        ".idea",
        ".vscode",
        "*.swp",
        "*.swo",
        "*~",
        ".DS_Store",
        "Thumbs.db",
        // General
        "*.log",
        "*.tmp",
        "*.temp",
        ".env",
        ".env.local",
    ]
}

// ============================================================================
// Export Configuration
// ============================================================================

/// Configuration for workspace export.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Patterns to exclude from export.
    pub exclude_patterns: Vec<String>,
    /// Whether to include the solution in exports.
    pub include_solution: bool,
    /// Whether to include verification scripts.
    pub include_verification: bool,
    /// Whether to include task prompt.
    pub include_prompt: bool,
    /// Whether to include task.yaml.
    pub include_task_yaml: bool,
    /// Whether to compress the archive (affects zip level).
    pub compress: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            exclude_patterns: default_exclude_patterns()
                .into_iter()
                .map(String::from)
                .collect(),
            include_solution: false, // Don't include solution by default
            include_verification: true,
            include_prompt: true,
            include_task_yaml: true,
            compress: true,
        }
    }
}

impl ExportConfig {
    /// Creates a new export config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds additional exclude patterns.
    pub fn with_exclude_patterns<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.exclude_patterns
            .extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Sets whether to include the solution.
    pub fn with_include_solution(mut self, include: bool) -> Self {
        self.include_solution = include;
        self
    }

    /// Sets whether to include verification scripts.
    pub fn with_include_verification(mut self, include: bool) -> Self {
        self.include_verification = include;
        self
    }

    /// Sets whether to include the task prompt.
    pub fn with_include_prompt(mut self, include: bool) -> Self {
        self.include_prompt = include;
        self
    }

    /// Sets whether to include task.yaml.
    pub fn with_include_task_yaml(mut self, include: bool) -> Self {
        self.include_task_yaml = include;
        self
    }

    /// Sets compression option.
    pub fn with_compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Adds language-specific exclude patterns.
    pub fn with_language_excludes(mut self, language: WorkspaceLanguage) -> Self {
        for pattern in language.build_artifacts() {
            self.exclude_patterns.push(pattern.to_string());
        }
        self
    }
}

// ============================================================================
// Export Result
// ============================================================================

/// Result of a workspace export operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceExportResult {
    /// ID of the exported workspace.
    pub workspace_id: String,
    /// Output path of the export.
    pub output_path: PathBuf,
    /// Number of files exported.
    pub file_count: usize,
    /// Total size in bytes.
    pub total_size: u64,
    /// Number of files excluded.
    pub excluded_count: usize,
    /// Export timestamp.
    pub exported_at: DateTime<Utc>,
    /// Files that were included.
    pub included_files: Vec<String>,
}

impl WorkspaceExportResult {
    /// Creates a new export result.
    pub fn new(workspace_id: impl Into<String>, output_path: impl Into<PathBuf>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            output_path: output_path.into(),
            file_count: 0,
            total_size: 0,
            excluded_count: 0,
            exported_at: Utc::now(),
            included_files: Vec::new(),
        }
    }
}

// ============================================================================
// Workspace Exporter
// ============================================================================

/// Exports generated workspaces to various formats.
pub struct WorkspaceExporter {
    /// Export configuration.
    config: ExportConfig,
}

impl WorkspaceExporter {
    /// Creates a new workspace exporter with default configuration.
    pub fn new() -> Self {
        Self {
            config: ExportConfig::default(),
        }
    }

    /// Creates a new workspace exporter with custom configuration.
    pub fn with_config(config: ExportConfig) -> Self {
        Self { config }
    }

    /// Returns the current configuration.
    pub fn config(&self) -> &ExportConfig {
        &self.config
    }

    /// Checks if a path should be excluded based on the configured patterns.
    pub fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.config.exclude_patterns {
            // Check for exact directory/file name match
            if let Some(file_name) = path.file_name() {
                let file_name_str = file_name.to_string_lossy();

                // Exact match
                if file_name_str == *pattern {
                    return true;
                }

                // Glob pattern match (simple)
                if pattern.starts_with('*') {
                    let suffix = &pattern[1..];
                    if file_name_str.ends_with(suffix) {
                        return true;
                    }
                }

                if pattern.ends_with('*') {
                    let prefix = &pattern[..pattern.len() - 1];
                    if file_name_str.starts_with(prefix) {
                        return true;
                    }
                }
            }

            // Check if any path component matches
            for component in path.components() {
                if let std::path::Component::Normal(name) = component {
                    if name.to_string_lossy() == *pattern {
                        return true;
                    }
                }
            }

            // Check full path match
            if path_str == *pattern {
                return true;
            }
        }

        false
    }

    /// Exports a workspace to a directory.
    #[instrument(skip(self, workspace))]
    pub async fn export_to_directory(
        &self,
        workspace: &GeneratedWorkspace,
        output_dir: &Path,
    ) -> Result<WorkspaceExportResult, ExportError> {
        info!("Exporting workspace {} to directory", workspace.id);

        let workspace_dir = output_dir.join(&workspace.id);
        fs::create_dir_all(&workspace_dir).await?;

        let mut result = WorkspaceExportResult::new(&workspace.id, &workspace_dir);

        // Export workspace files
        for file in &workspace.files {
            if self.should_exclude(&file.path) {
                debug!("Excluding file: {}", file.path.display());
                result.excluded_count += 1;
                continue;
            }

            let file_path = workspace_dir.join(&file.path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            fs::write(&file_path, &file.content).await?;

            result.file_count += 1;
            result.total_size += file.content.len() as u64;
            result.included_files.push(file.path.display().to_string());

            // Set executable permission if needed
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&file_path).await?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&file_path, perms).await?;
            }
        }

        // Export task.yaml if configured
        if self.config.include_task_yaml {
            let task_yaml = self.generate_task_yaml(workspace)?;
            let yaml_path = workspace_dir.join("task.yaml");
            fs::write(&yaml_path, &task_yaml).await?;
            result.file_count += 1;
            result.total_size += task_yaml.len() as u64;
            result.included_files.push("task.yaml".to_string());
        }

        // Export prompt.md if configured
        if self.config.include_prompt {
            let prompt_path = workspace_dir.join("prompt.md");
            fs::write(&prompt_path, &workspace.task_prompt).await?;
            result.file_count += 1;
            result.total_size += workspace.task_prompt.len() as u64;
            result.included_files.push("prompt.md".to_string());
        }

        // Export verification scripts if configured
        if self.config.include_verification && !workspace.verification_scripts.is_empty() {
            let scripts_dir = workspace_dir.join(".verification");
            fs::create_dir_all(&scripts_dir).await?;

            for script in &workspace.verification_scripts {
                let script_content = format!("{}\n{}", script.shebang(), script.content);
                let script_path = scripts_dir.join(&script.name);
                fs::write(&script_path, &script_content).await?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&script_path).await?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&script_path, perms).await?;
                }

                result.file_count += 1;
                result.total_size += script_content.len() as u64;
                result
                    .included_files
                    .push(format!(".verification/{}", script.name));
            }
        }

        // Export canary file
        let canary_path = workspace_dir.join(".canary");
        fs::write(&canary_path, &workspace.canary_token).await?;
        result.file_count += 1;
        result.total_size += workspace.canary_token.len() as u64;
        result.included_files.push(".canary".to_string());

        // Export solution if configured
        if self.config.include_solution && !workspace.solution_description.is_empty() {
            let solution_dir = workspace_dir.join(".solution");
            fs::create_dir_all(&solution_dir).await?;
            let solution_path = solution_dir.join("description.md");
            fs::write(&solution_path, &workspace.solution_description).await?;
            result.file_count += 1;
            result.total_size += workspace.solution_description.len() as u64;
            result
                .included_files
                .push(".solution/description.md".to_string());
        }

        info!(
            "Exported {} files ({} excluded), {} bytes total",
            result.file_count, result.excluded_count, result.total_size
        );

        Ok(result)
    }

    /// Exports a workspace to a zip archive.
    ///
    /// This implementation first exports to a temporary directory, then creates
    /// a tar.gz archive (since the zip crate is not available).
    #[instrument(skip(self, workspace))]
    pub async fn export_to_zip(
        &self,
        workspace: &GeneratedWorkspace,
        output_path: &Path,
    ) -> Result<WorkspaceExportResult, ExportError> {
        info!("Exporting workspace {} to archive", workspace.id);

        // Create parent directory if needed
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Create a temporary directory for export
        let temp_dir = tempfile::TempDir::new()
            .map_err(|e| ExportError::FilesystemError(format!("Failed to create temp dir: {}", e)))?;

        // Export to the temp directory
        let export_result = self.export_to_directory(workspace, temp_dir.path()).await?;

        // Create the archive using tar
        let _workspace_dir = temp_dir.path().join(&workspace.id);
        let output_path_str = output_path.display().to_string();
        let workspace_id = workspace.id.clone();

        // Use tar to create the archive
        let status = tokio::process::Command::new("tar")
            .current_dir(temp_dir.path())
            .args([
                "-czf",
                &output_path_str,
                &workspace_id,
            ])
            .status()
            .await
            .map_err(|e| ExportError::FilesystemError(format!("Failed to run tar: {}", e)))?;

        if !status.success() {
            return Err(ExportError::FilesystemError(
                "tar command failed".to_string(),
            ));
        }

        // Get the actual file size
        let metadata = std::fs::metadata(output_path)?;
        let mut result = export_result;
        result.output_path = output_path.to_path_buf();
        result.total_size = metadata.len();

        info!(
            "Exported {} files to archive ({} bytes)",
            result.file_count, result.total_size
        );

        Ok(result)
    }

    /// Generates the task.yaml content.
    fn generate_task_yaml(&self, workspace: &GeneratedWorkspace) -> Result<String, ExportError> {
        let task_info = TaskYaml {
            id: workspace.id.clone(),
            name: workspace.spec.name.clone(),
            language: workspace.spec.language.display_name().to_string(),
            difficulty: workspace.spec.difficulty,
            project_type: workspace.spec.project_type.clone(),
            vulnerability_types: workspace
                .spec
                .vulnerability_types
                .iter()
                .map(|v| v.display_name().to_string())
                .collect(),
            tags: workspace.spec.tags.clone(),
            canary_token: workspace.canary_token.clone(),
            generated_at: workspace.generated_at,
            verification_script_count: workspace.verification_scripts.len(),
            file_count: workspace.files.len(),
        };

        serde_yaml::to_string(&task_info).map_err(|e| ExportError::Serialization(e.to_string()))
    }

    /// Exports multiple workspaces to a batch directory.
    #[instrument(skip(self, workspaces))]
    pub async fn export_batch(
        &self,
        workspaces: &[GeneratedWorkspace],
        output_dir: &Path,
    ) -> Result<Vec<WorkspaceExportResult>, ExportError> {
        if workspaces.is_empty() {
            return Err(ExportError::NoTasks);
        }

        info!("Exporting batch of {} workspaces", workspaces.len());

        fs::create_dir_all(output_dir).await?;

        let mut results = Vec::with_capacity(workspaces.len());
        for workspace in workspaces {
            let result = self.export_to_directory(workspace, output_dir).await?;
            results.push(result);
        }

        // Write batch manifest
        let manifest = BatchManifest {
            workspace_count: workspaces.len(),
            workspace_ids: workspaces.iter().map(|w| w.id.clone()).collect(),
            exported_at: Utc::now(),
            total_files: results.iter().map(|r| r.file_count).sum(),
            total_size: results.iter().map(|r| r.total_size).sum(),
        };

        let manifest_yaml =
            serde_yaml::to_string(&manifest).map_err(|e| ExportError::Serialization(e.to_string()))?;
        fs::write(output_dir.join("manifest.yaml"), manifest_yaml).await?;

        info!(
            "Batch export complete: {} workspaces, {} files",
            results.len(),
            results.iter().map(|r| r.file_count).sum::<usize>()
        );

        Ok(results)
    }
}

impl Default for WorkspaceExporter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper Structures
// ============================================================================

/// Structure for task.yaml content.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskYaml {
    id: String,
    name: String,
    language: String,
    difficulty: u8,
    project_type: String,
    vulnerability_types: Vec<String>,
    tags: Vec<String>,
    canary_token: String,
    generated_at: DateTime<Utc>,
    verification_script_count: usize,
    file_count: usize,
}

/// Manifest for batch exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatchManifest {
    workspace_count: usize,
    workspace_ids: Vec<String>,
    exported_at: DateTime<Utc>,
    total_files: usize,
    total_size: u64,
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating WorkspaceExporter instances.
pub struct WorkspaceExporterBuilder {
    config: ExportConfig,
}

impl WorkspaceExporterBuilder {
    /// Creates a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: ExportConfig::default(),
        }
    }

    /// Adds exclude patterns.
    pub fn exclude_patterns<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config.exclude_patterns
            .extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Sets whether to include the solution.
    pub fn include_solution(mut self, include: bool) -> Self {
        self.config.include_solution = include;
        self
    }

    /// Sets whether to include verification scripts.
    pub fn include_verification(mut self, include: bool) -> Self {
        self.config.include_verification = include;
        self
    }

    /// Sets whether to include the task prompt.
    pub fn include_prompt(mut self, include: bool) -> Self {
        self.config.include_prompt = include;
        self
    }

    /// Sets whether to include task.yaml.
    pub fn include_task_yaml(mut self, include: bool) -> Self {
        self.config.include_task_yaml = include;
        self
    }

    /// Sets compression option.
    pub fn compress(mut self, compress: bool) -> Self {
        self.config.compress = compress;
        self
    }

    /// Adds language-specific exclude patterns.
    pub fn language_excludes(mut self, language: WorkspaceLanguage) -> Self {
        for pattern in language.build_artifacts() {
            self.config.exclude_patterns.push(pattern.to_string());
        }
        self
    }

    /// Builds the exporter.
    pub fn build(self) -> WorkspaceExporter {
        WorkspaceExporter::with_config(self.config)
    }
}

impl Default for WorkspaceExporterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::types::{VulnerabilityType, WorkspaceFile, WorkspaceSpec};

    fn create_test_workspace() -> GeneratedWorkspace {
        let spec = WorkspaceSpec::new("test-workspace")
            .with_name("Test Workspace")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection);

        let mut workspace = GeneratedWorkspace::new(spec);
        workspace.add_file(WorkspaceFile::source("src/main.py", "print('hello')"));
        workspace.add_file(WorkspaceFile::source(
            "src/__pycache__/main.cpython-311.pyc",
            "binary data",
        ));
        workspace.add_file(WorkspaceFile::test("tests/test_main.py", "def test(): pass"));
        workspace.task_prompt = "Fix the SQL injection vulnerability.".to_string();
        workspace.solution_description = "Use parameterized queries.".to_string();
        workspace
    }

    #[test]
    fn test_default_exclude_patterns() {
        let patterns = default_exclude_patterns();
        assert!(patterns.contains(&"node_modules"));
        assert!(patterns.contains(&"__pycache__"));
        assert!(patterns.contains(&"target"));
        assert!(patterns.contains(&".git"));
    }

    #[test]
    fn test_export_config_defaults() {
        let config = ExportConfig::default();
        assert!(!config.include_solution);
        assert!(config.include_verification);
        assert!(config.include_prompt);
        assert!(config.include_task_yaml);
        assert!(config.compress);
    }

    #[test]
    fn test_export_config_builder() {
        let config = ExportConfig::new()
            .with_include_solution(true)
            .with_compress(false);

        assert!(config.include_solution);
        assert!(!config.compress);
    }

    #[test]
    fn test_should_exclude() {
        let exporter = WorkspaceExporter::new();

        // Test exact matches
        assert!(exporter.should_exclude(Path::new("node_modules")));
        assert!(exporter.should_exclude(Path::new("__pycache__")));
        assert!(exporter.should_exclude(Path::new(".git")));

        // Test nested paths
        assert!(exporter.should_exclude(Path::new("src/__pycache__/main.pyc")));
        assert!(exporter.should_exclude(Path::new("project/node_modules/pkg")));

        // Test wildcard patterns
        assert!(exporter.should_exclude(Path::new("test.pyc")));
        assert!(exporter.should_exclude(Path::new("module.pyo")));

        // Test non-excluded paths
        assert!(!exporter.should_exclude(Path::new("src/main.py")));
        assert!(!exporter.should_exclude(Path::new("tests/test_main.py")));
    }

    #[test]
    fn test_language_specific_excludes() {
        let config = ExportConfig::new().with_language_excludes(WorkspaceLanguage::Python);

        assert!(config.exclude_patterns.contains(&"__pycache__".to_string()));
        assert!(config.exclude_patterns.contains(&"*.pyc".to_string()));
    }

    #[test]
    fn test_workspace_export_result() {
        let result = WorkspaceExportResult::new("test-ws", "/output/test-ws");
        assert_eq!(result.workspace_id, "test-ws");
        assert_eq!(result.file_count, 0);
        assert_eq!(result.total_size, 0);
    }

    #[tokio::test]
    async fn test_export_to_directory() {
        let temp_dir = tempfile::TempDir::new().expect("should create temp dir");
        let workspace = create_test_workspace();
        let exporter = WorkspaceExporter::new();

        let result = exporter
            .export_to_directory(&workspace, temp_dir.path())
            .await
            .expect("should export");

        assert!(result.file_count > 0);
        assert!(result.excluded_count > 0); // __pycache__ should be excluded

        // Check that task.yaml was created
        let task_yaml_path = temp_dir.path().join("test-workspace/task.yaml");
        assert!(task_yaml_path.exists());

        // Check that prompt.md was created
        let prompt_path = temp_dir.path().join("test-workspace/prompt.md");
        assert!(prompt_path.exists());

        // Check that canary was created
        let canary_path = temp_dir.path().join("test-workspace/.canary");
        assert!(canary_path.exists());
    }

    #[tokio::test]
    async fn test_export_to_zip() {
        let temp_dir = tempfile::TempDir::new().expect("should create temp dir");
        let workspace = create_test_workspace();
        let exporter = WorkspaceExporter::new();

        let zip_path = temp_dir.path().join("workspace.zip");
        let result = exporter
            .export_to_zip(&workspace, &zip_path)
            .await
            .expect("should export");

        assert!(zip_path.exists());
        assert!(result.file_count > 0);
        assert!(result.total_size > 0);
    }

    #[test]
    fn test_exporter_builder() {
        let exporter = WorkspaceExporterBuilder::new()
            .include_solution(true)
            .compress(false)
            .exclude_patterns(vec!["*.tmp"])
            .build();

        assert!(exporter.config.include_solution);
        assert!(!exporter.config.compress);
        assert!(exporter.config.exclude_patterns.contains(&"*.tmp".to_string()));
    }

    #[test]
    fn test_generate_task_yaml() {
        let workspace = create_test_workspace();
        let exporter = WorkspaceExporter::new();

        let yaml = exporter.generate_task_yaml(&workspace).expect("should generate yaml");

        assert!(yaml.contains("test-workspace"));
        assert!(yaml.contains("Python"));
        assert!(yaml.contains("SQL Injection"));
    }
}
