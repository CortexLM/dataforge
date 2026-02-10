//! Project templates for synthetic workspace generation.
//!
//! This module provides templates for different project types that serve as
//! starting points for code generation.

use serde::{Deserialize, Serialize};

use super::config::{LanguageTarget, ProjectCategory};

/// A template for generating a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTemplate {
    /// Template identifier.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Target language.
    pub language: LanguageTarget,
    /// Project category.
    pub category: ProjectCategory,
    /// Template description.
    pub description: String,
    /// Framework to use.
    pub framework: Option<String>,
    /// Directory structure.
    pub directories: Vec<String>,
    /// Files to generate with their purposes.
    pub files: Vec<FileTemplate>,
    /// Dependencies to include.
    pub dependencies: Vec<DependencyTemplate>,
    /// Common vulnerability patterns for this template.
    pub vulnerability_patterns: Vec<VulnerabilityPattern>,
}

impl WorkspaceTemplate {
    /// Creates a new workspace template.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        language: LanguageTarget,
        category: ProjectCategory,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            language,
            category,
            description: String::new(),
            framework: None,
            directories: Vec::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
            vulnerability_patterns: Vec::new(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self
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

    /// Adds file templates.
    pub fn with_files(mut self, files: Vec<FileTemplate>) -> Self {
        self.files = files;
        self
    }

    /// Adds dependencies.
    pub fn with_dependencies(mut self, deps: Vec<DependencyTemplate>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Adds vulnerability patterns.
    pub fn with_vulnerability_patterns(mut self, patterns: Vec<VulnerabilityPattern>) -> Self {
        self.vulnerability_patterns = patterns;
        self
    }

    // =========================================================================
    // Built-in Templates
    // =========================================================================

    /// Python Flask REST API template.
    pub fn python_flask_api() -> Self {
        Self::new("python-flask-api", "Flask REST API", LanguageTarget::Python, ProjectCategory::WebApi)
            .with_description("A RESTful API built with Flask, featuring user authentication, database operations, and file handling")
            .with_framework("Flask")
            .with_directories(vec![
                "app",
                "app/routes",
                "app/models",
                "app/utils",
                "tests",
                "tests/unit",
                "tests/integration",
                "config",
            ])
            .with_files(vec![
                FileTemplate::source("app/__init__.py", "Flask application factory", true),
                FileTemplate::source("app/routes/__init__.py", "Route blueprints", false),
                FileTemplate::source("app/routes/auth.py", "Authentication endpoints", true),
                FileTemplate::source("app/routes/users.py", "User CRUD endpoints", true),
                FileTemplate::source("app/routes/files.py", "File upload/download", true),
                FileTemplate::source("app/models/user.py", "User data model", true),
                FileTemplate::source("app/models/session.py", "Session model", true),
                FileTemplate::source("app/utils/crypto.py", "Cryptographic utilities", true),
                FileTemplate::source("app/utils/validation.py", "Input validation", true),
                FileTemplate::source("app/database.py", "Database connection", true),
                FileTemplate::config("config.py", "Application configuration", true),
                FileTemplate::config("requirements.txt", "Python dependencies", false),
                FileTemplate::test("tests/conftest.py", "Test fixtures", false),
                FileTemplate::test("tests/test_auth.py", "Auth tests", false),
            ])
            .with_dependencies(vec![
                DependencyTemplate::new("flask", ">=3.0.0"),
                DependencyTemplate::new("flask-cors", ">=4.0.0"),
                DependencyTemplate::new("pyjwt", ">=2.8.0"),
                DependencyTemplate::new("psycopg2-binary", ">=2.9.0"),
                DependencyTemplate::new("python-dotenv", ">=1.0.0"),
                DependencyTemplate::dev("pytest", ">=7.0.0"),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("sql_injection")
                    .in_file("app/database.py")
                    .in_file("app/routes/users.py")
                    .with_description("Direct string interpolation in SQL queries"),
                VulnerabilityPattern::new("authentication_bypass")
                    .in_file("app/routes/auth.py")
                    .with_description("Missing or weak authentication checks"),
                VulnerabilityPattern::new("insecure_deserialization")
                    .in_file("app/routes/auth.py")
                    .in_file("app/utils/crypto.py")
                    .with_description("Using pickle or yaml.load without safe_load"),
                VulnerabilityPattern::new("weak_cryptography")
                    .in_file("app/utils/crypto.py")
                    .with_description("Using MD5/SHA1 for passwords, weak random"),
                VulnerabilityPattern::new("path_traversal")
                    .in_file("app/routes/files.py")
                    .with_description("Unchecked file path from user input"),
                VulnerabilityPattern::new("hardcoded_secrets")
                    .in_file("config.py")
                    .with_description("API keys or passwords in code"),
            ])
    }

    /// Python FastAPI microservice template.
    pub fn python_fastapi_service() -> Self {
        Self::new("python-fastapi-service", "FastAPI Microservice", LanguageTarget::Python, ProjectCategory::Microservice)
            .with_description("A microservice built with FastAPI, featuring async operations, data validation, and external API calls")
            .with_framework("FastAPI")
            .with_directories(vec![
                "app",
                "app/api",
                "app/api/v1",
                "app/core",
                "app/db",
                "app/schemas",
                "app/services",
                "tests",
            ])
            .with_files(vec![
                FileTemplate::source("app/main.py", "FastAPI application entry", true),
                FileTemplate::source("app/api/v1/endpoints.py", "API endpoints", true),
                FileTemplate::source("app/core/config.py", "Configuration", true),
                FileTemplate::source("app/core/security.py", "Security utilities", true),
                FileTemplate::source("app/db/database.py", "Database setup", true),
                FileTemplate::source("app/db/crud.py", "CRUD operations", true),
                FileTemplate::source("app/schemas/user.py", "Pydantic schemas", false),
                FileTemplate::source("app/services/external.py", "External API calls", true),
                FileTemplate::config("requirements.txt", "Dependencies", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("ssrf")
                    .in_file("app/services/external.py")
                    .with_description("Unvalidated URL in external requests"),
                VulnerabilityPattern::new("sql_injection")
                    .in_file("app/db/crud.py")
                    .with_description("Raw SQL with user input"),
                VulnerabilityPattern::new("mass_assignment")
                    .in_file("app/api/v1/endpoints.py")
                    .with_description("Accepting all fields without filtering"),
            ])
    }

    /// Node.js Express API template.
    pub fn nodejs_express_api() -> Self {
        Self::new(
            "nodejs-express-api",
            "Express.js REST API",
            LanguageTarget::JavaScript,
            ProjectCategory::WebApi,
        )
        .with_description(
            "A RESTful API built with Express.js, featuring JWT auth, MongoDB, and file handling",
        )
        .with_framework("Express")
        .with_directories(vec![
            "src",
            "src/routes",
            "src/controllers",
            "src/models",
            "src/middleware",
            "src/utils",
            "src/config",
            "tests",
        ])
        .with_files(vec![
            FileTemplate::source("src/index.js", "Application entry", true),
            FileTemplate::source("src/app.js", "Express setup", true),
            FileTemplate::source("src/routes/auth.js", "Auth routes", true),
            FileTemplate::source("src/routes/users.js", "User routes", true),
            FileTemplate::source("src/controllers/authController.js", "Auth logic", true),
            FileTemplate::source("src/controllers/userController.js", "User logic", true),
            FileTemplate::source("src/models/User.js", "User model", false),
            FileTemplate::source("src/middleware/auth.js", "Auth middleware", true),
            FileTemplate::source("src/middleware/validate.js", "Validation", true),
            FileTemplate::source("src/utils/crypto.js", "Crypto utils", true),
            FileTemplate::source("src/utils/db.js", "Database utils", true),
            FileTemplate::source("src/config/index.js", "Configuration", true),
            FileTemplate::config("package.json", "NPM config", false),
            FileTemplate::test("tests/auth.test.js", "Auth tests", false),
        ])
        .with_vulnerability_patterns(vec![
            VulnerabilityPattern::new("nosql_injection")
                .in_file("src/controllers/userController.js")
                .with_description("Unsanitized input in MongoDB queries"),
            VulnerabilityPattern::new("xss")
                .in_file("src/routes/users.js")
                .with_description("Reflected user input without encoding"),
            VulnerabilityPattern::new("prototype_pollution")
                .in_file("src/utils/db.js")
                .with_description("Object merge without prototype check"),
            VulnerabilityPattern::new("jwt_weakness")
                .in_file("src/middleware/auth.js")
                .with_description("Algorithm confusion or weak secret"),
        ])
    }

    /// Rust CLI tool template.
    pub fn rust_cli_tool() -> Self {
        Self::new("rust-cli-tool", "Rust CLI Tool", LanguageTarget::Rust, ProjectCategory::CliTool)
            .with_description("A command-line tool built with Rust, featuring file processing, serialization, and shell execution")
            .with_framework("clap")
            .with_directories(vec![
                "src",
                "src/commands",
                "src/utils",
                "tests",
            ])
            .with_files(vec![
                FileTemplate::source("src/main.rs", "Entry point", true),
                FileTemplate::source("src/lib.rs", "Library root", false),
                FileTemplate::source("src/commands/mod.rs", "Command modules", false),
                FileTemplate::source("src/commands/process.rs", "File processing", true),
                FileTemplate::source("src/commands/exec.rs", "Shell execution", true),
                FileTemplate::source("src/utils/mod.rs", "Utility modules", false),
                FileTemplate::source("src/utils/file.rs", "File utilities", true),
                FileTemplate::source("src/utils/crypto.rs", "Crypto utilities", true),
                FileTemplate::config("Cargo.toml", "Cargo manifest", false),
                FileTemplate::test("tests/integration.rs", "Integration tests", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("command_injection")
                    .in_file("src/commands/exec.rs")
                    .with_description("Unsanitized input in shell commands"),
                VulnerabilityPattern::new("path_traversal")
                    .in_file("src/utils/file.rs")
                    .with_description("Unchecked path concatenation"),
                VulnerabilityPattern::new("unsafe_deserialization")
                    .in_file("src/commands/process.rs")
                    .with_description("Deserializing untrusted data"),
            ])
    }

    /// Go microservice template.
    pub fn go_microservice() -> Self {
        Self::new("go-microservice", "Go Microservice", LanguageTarget::Go, ProjectCategory::Microservice)
            .with_description("A microservice built with Go, featuring HTTP handlers, database operations, and authentication")
            .with_framework("Gin")
            .with_directories(vec![
                "cmd",
                "internal",
                "internal/api",
                "internal/auth",
                "internal/db",
                "internal/models",
                "pkg",
                "tests",
            ])
            .with_files(vec![
                FileTemplate::source("cmd/server/main.go", "Entry point", true),
                FileTemplate::source("internal/api/handlers.go", "HTTP handlers", true),
                FileTemplate::source("internal/api/middleware.go", "Middleware", true),
                FileTemplate::source("internal/auth/jwt.go", "JWT handling", true),
                FileTemplate::source("internal/db/queries.go", "Database queries", true),
                FileTemplate::source("internal/models/user.go", "User model", false),
                FileTemplate::source("pkg/utils/crypto.go", "Crypto utils", true),
                FileTemplate::config("go.mod", "Go modules", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("sql_injection")
                    .in_file("internal/db/queries.go")
                    .with_description("String concatenation in SQL"),
                VulnerabilityPattern::new("weak_random")
                    .in_file("internal/auth/jwt.go")
                    .in_file("pkg/utils/crypto.go")
                    .with_description("Using math/rand instead of crypto/rand"),
                VulnerabilityPattern::new("race_condition")
                    .in_file("internal/api/handlers.go")
                    .with_description("Unprotected shared state"),
            ])
    }

    /// Returns all built-in templates.
    pub fn all_templates() -> Vec<Self> {
        vec![
            Self::python_flask_api(),
            Self::python_fastapi_service(),
            Self::nodejs_express_api(),
            Self::rust_cli_tool(),
            Self::go_microservice(),
        ]
    }

    /// Gets a template by language and category.
    pub fn get_template(language: LanguageTarget, category: ProjectCategory) -> Option<Self> {
        Self::all_templates()
            .into_iter()
            .find(|t| t.language == language && t.category == category)
    }

    /// Gets templates for a language.
    pub fn templates_for_language(language: LanguageTarget) -> Vec<Self> {
        Self::all_templates()
            .into_iter()
            .filter(|t| t.language == language)
            .collect()
    }
}

/// A file template specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTemplate {
    /// File path.
    pub path: String,
    /// Description of the file's purpose.
    pub description: String,
    /// Whether this file can contain vulnerabilities.
    pub can_have_vulnerabilities: bool,
    /// File type.
    pub file_type: FileTemplateType,
    /// Required imports/dependencies for this file.
    pub requires: Vec<String>,
}

impl FileTemplate {
    /// Creates a source file template.
    pub fn source(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Source,
            requires: Vec::new(),
        }
    }

    /// Creates a test file template.
    pub fn test(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Test,
            requires: Vec::new(),
        }
    }

    /// Creates a config file template.
    pub fn config(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Config,
            requires: Vec::new(),
        }
    }

    /// Adds required dependencies.
    pub fn with_requires<I, S>(mut self, requires: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.requires = requires.into_iter().map(Into::into).collect();
        self
    }
}

/// File template types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileTemplateType {
    Source,
    Test,
    Config,
    Documentation,
}

/// A dependency template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyTemplate {
    /// Package name.
    pub name: String,
    /// Version constraint.
    pub version: String,
    /// Whether this is a dev dependency.
    pub dev_only: bool,
}

impl DependencyTemplate {
    /// Creates a production dependency.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            dev_only: false,
        }
    }

    /// Creates a dev dependency.
    pub fn dev(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            dev_only: true,
        }
    }
}

/// A vulnerability pattern that can be applied to a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityPattern {
    /// Vulnerability type.
    pub vulnerability_type: String,
    /// Files where this vulnerability can be placed.
    pub applicable_files: Vec<String>,
    /// Description of how to implement the vulnerability.
    pub description: String,
    /// CWE identifier.
    pub cwe_id: Option<String>,
    /// OWASP category.
    pub owasp_category: Option<String>,
}

impl VulnerabilityPattern {
    /// Creates a new vulnerability pattern.
    pub fn new(vulnerability_type: impl Into<String>) -> Self {
        Self {
            vulnerability_type: vulnerability_type.into(),
            applicable_files: Vec::new(),
            description: String::new(),
            cwe_id: None,
            owasp_category: None,
        }
    }

    /// Adds a file where this vulnerability can be placed.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.applicable_files.push(file.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_template_creation() {
        let template = WorkspaceTemplate::new(
            "test-template",
            "Test Template",
            LanguageTarget::Python,
            ProjectCategory::WebApi,
        );

        assert_eq!(template.id, "test-template");
        assert_eq!(template.language, LanguageTarget::Python);
    }

    #[test]
    fn test_python_flask_template() {
        let template = WorkspaceTemplate::python_flask_api();

        assert_eq!(template.language, LanguageTarget::Python);
        assert_eq!(template.category, ProjectCategory::WebApi);
        assert!(!template.files.is_empty());
        assert!(!template.vulnerability_patterns.is_empty());

        // Check that files have vulnerability flags
        let auth_file = template.files.iter().find(|f| f.path.contains("auth.py"));
        assert!(auth_file.is_some());
        assert!(auth_file.unwrap().can_have_vulnerabilities);
    }

    #[test]
    fn test_all_templates() {
        let templates = WorkspaceTemplate::all_templates();
        assert!(!templates.is_empty());

        // Each template should have files and vulnerability patterns
        for template in &templates {
            assert!(
                !template.files.is_empty(),
                "Template {} has no files",
                template.id
            );
            assert!(
                !template.vulnerability_patterns.is_empty(),
                "Template {} has no vulnerability patterns",
                template.id
            );
        }
    }

    #[test]
    fn test_get_template_by_language() {
        let template =
            WorkspaceTemplate::get_template(LanguageTarget::Python, ProjectCategory::WebApi);
        assert!(template.is_some());
        assert_eq!(template.unwrap().language, LanguageTarget::Python);
    }

    #[test]
    fn test_vulnerability_pattern() {
        let pattern = VulnerabilityPattern::new("sql_injection")
            .in_file("db.py")
            .in_file("queries.py")
            .with_description("Direct SQL interpolation")
            .with_cwe("CWE-89");

        assert_eq!(pattern.vulnerability_type, "sql_injection");
        assert_eq!(pattern.applicable_files.len(), 2);
        assert_eq!(pattern.cwe_id, Some("CWE-89".to_string()));
    }
}
