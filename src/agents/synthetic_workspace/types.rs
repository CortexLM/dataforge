//! Core types for synthetic workspace generation.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::config::{DifficultyLevel, LanguageTarget, ProjectCategory};

/// Specification for a project to be generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpec {
    /// Unique identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Project description.
    pub description: String,
    /// Target language.
    pub language: LanguageTarget,
    /// Project category.
    pub category: ProjectCategory,
    /// Difficulty level.
    pub difficulty: DifficultyLevel,
    /// Framework to use (if any).
    pub framework: Option<String>,
    /// Expected directory structure.
    pub structure: ProjectStructure,
    /// Features to implement.
    pub features: Vec<String>,
    /// External dependencies.
    pub dependencies: Vec<String>,
    /// When this spec was created.
    pub created_at: DateTime<Utc>,
}

impl ProjectSpec {
    /// Creates a new project spec.
    pub fn new(name: impl Into<String>, language: LanguageTarget) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            language,
            category: ProjectCategory::default(),
            difficulty: DifficultyLevel::default(),
            framework: None,
            structure: ProjectStructure::default(),
            features: Vec::new(),
            dependencies: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the category.
    pub fn with_category(mut self, category: ProjectCategory) -> Self {
        self.category = category;
        self
    }

    /// Sets the difficulty.
    pub fn with_difficulty(mut self, difficulty: DifficultyLevel) -> Self {
        self.difficulty = difficulty;
        self
    }

    /// Sets the framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self
    }

    /// Sets the structure.
    pub fn with_structure(mut self, structure: ProjectStructure) -> Self {
        self.structure = structure;
        self
    }

    /// Adds features.
    pub fn with_features<I, S>(mut self, features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.features.extend(features.into_iter().map(Into::into));
        self
    }

    /// Adds dependencies.
    pub fn with_dependencies<I, S>(mut self, deps: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.dependencies.extend(deps.into_iter().map(Into::into));
        self
    }
}

/// Directory structure for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectStructure {
    /// Top-level directories.
    pub directories: Vec<String>,
    /// Key files that define the project.
    pub key_files: Vec<String>,
    /// Test directory structure.
    pub test_structure: Vec<String>,
    /// Configuration files.
    pub config_files: Vec<String>,
}

impl ProjectStructure {
    /// Creates a new structure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds directories.
    pub fn with_directories<I, S>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.directories.extend(dirs.into_iter().map(Into::into));
        self
    }

    /// Adds key files.
    pub fn with_key_files<I, S>(mut self, files: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.key_files.extend(files.into_iter().map(Into::into));
        self
    }

    /// Adds test structure.
    pub fn with_test_structure<I, S>(mut self, tests: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.test_structure
            .extend(tests.into_iter().map(Into::into));
        self
    }
}

/// Specification for a vulnerability to inject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilitySpec {
    /// Unique identifier.
    pub id: String,
    /// Type of vulnerability (e.g., "sql_injection", "xss").
    pub vulnerability_type: String,
    /// CWE identifier if applicable.
    pub cwe_id: Option<String>,
    /// OWASP category if applicable.
    pub owasp_category: Option<String>,
    /// Target file where vulnerability should be injected.
    pub target_file: String,
    /// Target function/method name.
    pub target_function: Option<String>,
    /// Description of the vulnerability.
    pub description: String,
    /// How the vulnerability should be implemented.
    pub injection_strategy: String,
    /// How subtle the vulnerability should be (1-10).
    pub subtlety_level: u8,
    /// Whether this vulnerability depends on others.
    pub dependencies: Vec<String>,
    /// Remediation guidance (hidden from the task).
    pub remediation: String,
}

impl VulnerabilitySpec {
    /// Creates a new vulnerability spec.
    pub fn new(vulnerability_type: impl Into<String>, target_file: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            vulnerability_type: vulnerability_type.into(),
            cwe_id: None,
            owasp_category: None,
            target_file: target_file.into(),
            target_function: None,
            description: String::new(),
            injection_strategy: String::new(),
            subtlety_level: 5,
            dependencies: Vec::new(),
            remediation: String::new(),
        }
    }

    /// Sets the CWE ID.
    pub fn with_cwe(mut self, cwe: impl Into<String>) -> Self {
        self.cwe_id = Some(cwe.into());
        self
    }

    /// Sets the OWASP category.
    pub fn with_owasp(mut self, owasp: impl Into<String>) -> Self {
        self.owasp_category = Some(owasp.into());
        self
    }

    /// Sets the target function.
    pub fn with_target_function(mut self, func: impl Into<String>) -> Self {
        self.target_function = Some(func.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the injection strategy.
    pub fn with_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.injection_strategy = strategy.into();
        self
    }

    /// Sets the subtlety level.
    pub fn with_subtlety(mut self, level: u8) -> Self {
        self.subtlety_level = level.clamp(1, 10);
        self
    }

    /// Sets the remediation guidance.
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = remediation.into();
        self
    }
}

/// A generated file in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// Relative path from workspace root.
    pub path: PathBuf,
    /// File content.
    pub content: FileContent,
    /// File type classification.
    pub file_type: FileType,
    /// Description of the file's purpose.
    pub description: String,
    /// Whether this file contains injected vulnerabilities.
    pub has_vulnerabilities: bool,
    /// IDs of vulnerabilities in this file.
    pub vulnerability_ids: Vec<String>,
}

impl GeneratedFile {
    /// Creates a new generated file.
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: FileContent::Text(content.into()),
            file_type: FileType::Source,
            description: String::new(),
            has_vulnerabilities: false,
            vulnerability_ids: Vec::new(),
        }
    }

    /// Creates a source file.
    pub fn source(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: FileContent::Text(content.into()),
            file_type: FileType::Source,
            description: String::new(),
            has_vulnerabilities: false,
            vulnerability_ids: Vec::new(),
        }
    }

    /// Creates a test file.
    pub fn test(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: FileContent::Text(content.into()),
            file_type: FileType::Test,
            description: String::new(),
            has_vulnerabilities: false,
            vulnerability_ids: Vec::new(),
        }
    }

    /// Creates a config file.
    pub fn config(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: FileContent::Text(content.into()),
            file_type: FileType::Configuration,
            description: String::new(),
            has_vulnerabilities: false,
            vulnerability_ids: Vec::new(),
        }
    }

    /// Sets the file type.
    pub fn with_type(mut self, file_type: FileType) -> Self {
        self.file_type = file_type;
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Marks this file as containing vulnerabilities.
    pub fn with_vulnerabilities(mut self, vulnerability_ids: Vec<String>) -> Self {
        self.has_vulnerabilities = !vulnerability_ids.is_empty();
        self.vulnerability_ids = vulnerability_ids;
        self
    }

    /// Returns the content as text (if text).
    pub fn text_content(&self) -> Option<&str> {
        match &self.content {
            FileContent::Text(text) => Some(text),
            FileContent::Binary(_) => None,
        }
    }

    /// Returns the number of lines.
    pub fn line_count(&self) -> usize {
        match &self.content {
            FileContent::Text(text) => text.lines().count(),
            FileContent::Binary(_) => 0,
        }
    }

    /// Returns the file extension.
    pub fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(String::from)
    }
}

/// File content types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileContent {
    /// Text file content.
    Text(String),
    /// Binary file content (base64 encoded).
    Binary(Vec<u8>),
}

/// File type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    /// Source code file.
    Source,
    /// Test file.
    Test,
    /// Configuration file.
    Configuration,
    /// Documentation file.
    Documentation,
    /// Data file.
    Data,
    /// Build script.
    BuildScript,
    /// Other file type.
    Other,
}

impl Default for FileType {
    fn default() -> Self {
        Self::Source
    }
}

/// An injected vulnerability in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedVulnerability {
    /// Unique identifier.
    pub id: String,
    /// Type of vulnerability.
    pub vulnerability_type: String,
    /// CWE identifier.
    pub cwe_id: Option<String>,
    /// OWASP category.
    pub owasp_category: Option<String>,
    /// File where vulnerability exists.
    pub file_path: String,
    /// Line numbers where vulnerability exists.
    pub line_numbers: Vec<usize>,
    /// Function/method containing the vulnerability.
    pub function_name: Option<String>,
    /// Description of the vulnerability.
    pub description: String,
    /// Severity (1-10).
    pub severity: u8,
    /// Expected impact if exploited.
    pub impact: String,
    /// Remediation guidance.
    pub remediation: String,
    /// Code snippet showing the vulnerable code.
    pub vulnerable_code: String,
    /// Code snippet showing the fixed code.
    pub fixed_code: String,
}

impl InjectedVulnerability {
    /// Creates a new injected vulnerability.
    pub fn new(vulnerability_type: impl Into<String>, file_path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            vulnerability_type: vulnerability_type.into(),
            cwe_id: None,
            owasp_category: None,
            file_path: file_path.into(),
            line_numbers: Vec::new(),
            function_name: None,
            description: String::new(),
            severity: 5,
            impact: String::new(),
            remediation: String::new(),
            vulnerable_code: String::new(),
            fixed_code: String::new(),
        }
    }

    /// Sets the CWE ID.
    pub fn with_cwe(mut self, cwe: impl Into<String>) -> Self {
        self.cwe_id = Some(cwe.into());
        self
    }

    /// Sets the OWASP category.
    pub fn with_owasp(mut self, owasp: impl Into<String>) -> Self {
        self.owasp_category = Some(owasp.into());
        self
    }

    /// Sets the line numbers.
    pub fn with_lines(mut self, lines: Vec<usize>) -> Self {
        self.line_numbers = lines;
        self
    }

    /// Sets the function name.
    pub fn with_function(mut self, func: impl Into<String>) -> Self {
        self.function_name = Some(func.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the severity.
    pub fn with_severity(mut self, severity: u8) -> Self {
        self.severity = severity.clamp(1, 10);
        self
    }

    /// Sets the impact.
    pub fn with_impact(mut self, impact: impl Into<String>) -> Self {
        self.impact = impact.into();
        self
    }

    /// Sets the remediation.
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = remediation.into();
        self
    }

    /// Sets the vulnerable code snippet.
    pub fn with_vulnerable_code(mut self, code: impl Into<String>) -> Self {
        self.vulnerable_code = code.into();
        self
    }

    /// Sets the fixed code snippet.
    pub fn with_fixed_code(mut self, code: impl Into<String>) -> Self {
        self.fixed_code = code.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_spec_builder() {
        let spec = ProjectSpec::new("test-project", LanguageTarget::Python)
            .with_description("A test project")
            .with_category(ProjectCategory::WebApi)
            .with_framework("Flask");

        assert_eq!(spec.name, "test-project");
        assert_eq!(spec.language, LanguageTarget::Python);
        assert_eq!(spec.category, ProjectCategory::WebApi);
        assert_eq!(spec.framework, Some("Flask".to_string()));
    }

    #[test]
    fn test_generated_file() {
        let file = GeneratedFile::source("src/main.py", "print('hello')");
        assert_eq!(file.file_type, FileType::Source);
        assert_eq!(file.line_count(), 1);
        assert_eq!(file.extension(), Some("py".to_string()));
    }

    #[test]
    fn test_vulnerability_spec() {
        let spec = VulnerabilitySpec::new("sql_injection", "app/db.py")
            .with_cwe("CWE-89")
            .with_subtlety(8)
            .with_remediation("Use parameterized queries");

        assert_eq!(spec.vulnerability_type, "sql_injection");
        assert_eq!(spec.cwe_id, Some("CWE-89".to_string()));
        assert_eq!(spec.subtlety_level, 8);
    }

    #[test]
    fn test_injected_vulnerability() {
        let vuln = InjectedVulnerability::new("xss", "templates/view.html")
            .with_severity(7)
            .with_lines(vec![42, 43, 44])
            .with_function("render_user_input");

        assert_eq!(vuln.severity, 7);
        assert_eq!(vuln.line_numbers, vec![42, 43, 44]);
    }
}
