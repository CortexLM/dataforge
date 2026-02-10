//! Core types for workspace generation.
//!
//! This module defines the data structures used for generating complete
//! code workspaces with deliberately injected vulnerabilities and bugs.
//!
//! # Overview
//!
//! The workspace generation system creates realistic codebases that contain
//! security vulnerabilities or bugs that agents must identify and fix.
//! Each workspace includes:
//!
//! - Source code files with injected issues
//! - Build/configuration files
//! - Test files
//! - Verification scripts to check if fixes were applied correctly
//!
//! # Example
//!
//! ```ignore
//! use dataforge::workspace::{WorkspaceSpec, WorkspaceLanguage, VulnerabilityType};
//!
//! let spec = WorkspaceSpec::new("sql-injection-fix")
//!     .with_language(WorkspaceLanguage::Python)
//!     .with_vulnerability(VulnerabilityType::SqlInjection)
//!     .with_description("Fix the SQL injection vulnerability in the user login endpoint");
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// Language Types
// ============================================================================

/// Supported programming languages for workspace generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceLanguage {
    /// Python projects (Flask, Django, FastAPI, etc.)
    #[default]
    Python,
    /// JavaScript projects (Node.js, Express, etc.)
    JavaScript,
    /// TypeScript projects
    TypeScript,
    /// Rust projects
    Rust,
    /// Go projects
    Go,
    /// Java projects
    Java,
    /// C/C++ projects
    Cpp,
    /// Ruby projects
    Ruby,
    /// PHP projects
    Php,
}

impl WorkspaceLanguage {
    /// Returns the file extension for source files in this language.
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Python => ".py",
            Self::JavaScript => ".js",
            Self::TypeScript => ".ts",
            Self::Rust => ".rs",
            Self::Go => ".go",
            Self::Java => ".java",
            Self::Cpp => ".cpp",
            Self::Ruby => ".rb",
            Self::Php => ".php",
        }
    }

    /// Returns common build artifacts to exclude for this language.
    pub fn build_artifacts(&self) -> &'static [&'static str] {
        match self {
            Self::Python => &[
                "__pycache__",
                "*.pyc",
                "*.pyo",
                ".pytest_cache",
                "*.egg-info",
                "dist",
                "build",
                ".tox",
                ".venv",
                "venv",
                ".mypy_cache",
            ],
            Self::JavaScript | Self::TypeScript => &[
                "node_modules",
                "dist",
                "build",
                ".next",
                ".nuxt",
                "coverage",
                ".cache",
            ],
            Self::Rust => &["target", "Cargo.lock"],
            Self::Go => &["vendor", "bin"],
            Self::Java => &["target", "build", ".gradle", "*.class", "*.jar"],
            Self::Cpp => &["build", "cmake-build-*", "*.o", "*.a", "*.so", "*.dylib"],
            Self::Ruby => &["vendor", ".bundle", "coverage", "tmp"],
            Self::Php => &["vendor", ".phpunit.cache", "composer.lock"],
        }
    }

    /// Returns the package manager file for this language.
    pub fn package_file(&self) -> &'static str {
        match self {
            Self::Python => "requirements.txt",
            Self::JavaScript | Self::TypeScript => "package.json",
            Self::Rust => "Cargo.toml",
            Self::Go => "go.mod",
            Self::Java => "pom.xml",
            Self::Cpp => "CMakeLists.txt",
            Self::Ruby => "Gemfile",
            Self::Php => "composer.json",
        }
    }

    /// Returns the typical test command for this language.
    pub fn test_command(&self) -> &'static str {
        match self {
            Self::Python => "pytest",
            Self::JavaScript | Self::TypeScript => "npm test",
            Self::Rust => "cargo test",
            Self::Go => "go test ./...",
            Self::Java => "mvn test",
            Self::Cpp => "ctest",
            Self::Ruby => "bundle exec rspec",
            Self::Php => "vendor/bin/phpunit",
        }
    }

    /// Returns a human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Cpp => "C++",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
        }
    }
}

impl std::fmt::Display for WorkspaceLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Vulnerability Types
// ============================================================================

/// Types of security vulnerabilities that can be injected into workspaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulnerabilityType {
    /// SQL injection vulnerability (e.g., string concatenation in queries)
    SqlInjection,
    /// Cross-site scripting (XSS) vulnerability
    Xss,
    /// Authentication bypass vulnerability
    AuthenticationBypass,
    /// Race condition vulnerability
    RaceCondition,
    /// Memory leak vulnerability
    MemoryLeak,
    /// Path traversal vulnerability (e.g., ../../etc/passwd)
    PathTraversal,
    /// Insecure deserialization vulnerability
    InsecureDeserialization,
    /// Command injection vulnerability
    CommandInjection,
    /// Server-side request forgery (SSRF)
    Ssrf,
    /// Insecure direct object reference (IDOR)
    Idor,
    /// Hardcoded credentials
    HardcodedCredentials,
    /// Missing input validation
    MissingInputValidation,
    /// Improper error handling that leaks information
    InformationLeakage,
    /// Buffer overflow vulnerability (primarily for C/C++/Rust)
    BufferOverflow,
    /// Use after free vulnerability (primarily for C/C++/Rust)
    UseAfterFree,
    /// Null pointer dereference
    NullPointerDereference,
    /// Integer overflow vulnerability
    IntegerOverflow,
    /// Cryptographic weakness (weak algorithms, improper key handling)
    CryptographicWeakness,
    /// Cross-site request forgery (CSRF)
    Csrf,
    /// Open redirect vulnerability
    OpenRedirect,
}

impl VulnerabilityType {
    /// Returns a human-readable description of the vulnerability.
    pub fn description(&self) -> &'static str {
        match self {
            Self::SqlInjection => "SQL injection allows attackers to manipulate database queries",
            Self::Xss => "Cross-site scripting allows attackers to inject malicious scripts",
            Self::AuthenticationBypass => {
                "Authentication bypass allows unauthorized access to protected resources"
            }
            Self::RaceCondition => "Race condition allows exploitation of timing vulnerabilities",
            Self::MemoryLeak => "Memory leak causes resource exhaustion over time",
            Self::PathTraversal => {
                "Path traversal allows access to files outside intended directory"
            }
            Self::InsecureDeserialization => {
                "Insecure deserialization can lead to remote code execution"
            }
            Self::CommandInjection => {
                "Command injection allows execution of arbitrary system commands"
            }
            Self::Ssrf => "Server-side request forgery allows making requests from the server",
            Self::Idor => {
                "Insecure direct object reference allows access to unauthorized resources"
            }
            Self::HardcodedCredentials => {
                "Hardcoded credentials expose sensitive authentication data"
            }
            Self::MissingInputValidation => {
                "Missing input validation allows malformed or malicious data"
            }
            Self::InformationLeakage => "Information leakage exposes sensitive system details",
            Self::BufferOverflow => "Buffer overflow allows memory corruption and code execution",
            Self::UseAfterFree => "Use after free allows exploitation of freed memory",
            Self::NullPointerDereference => "Null pointer dereference causes crashes",
            Self::IntegerOverflow => "Integer overflow can lead to unexpected behavior",
            Self::CryptographicWeakness => {
                "Cryptographic weakness compromises data confidentiality"
            }
            Self::Csrf => {
                "Cross-site request forgery allows attackers to perform actions on behalf of users"
            }
            Self::OpenRedirect => {
                "Open redirect allows attackers to redirect users to malicious sites"
            }
        }
    }

    /// Returns the CWE (Common Weakness Enumeration) identifier, if applicable.
    pub fn cwe_id(&self) -> Option<u32> {
        match self {
            Self::SqlInjection => Some(89),
            Self::Xss => Some(79),
            Self::AuthenticationBypass => Some(287),
            Self::RaceCondition => Some(362),
            Self::MemoryLeak => Some(401),
            Self::PathTraversal => Some(22),
            Self::InsecureDeserialization => Some(502),
            Self::CommandInjection => Some(78),
            Self::Ssrf => Some(918),
            Self::Idor => Some(639),
            Self::HardcodedCredentials => Some(798),
            Self::MissingInputValidation => Some(20),
            Self::InformationLeakage => Some(200),
            Self::BufferOverflow => Some(120),
            Self::UseAfterFree => Some(416),
            Self::NullPointerDereference => Some(476),
            Self::IntegerOverflow => Some(190),
            Self::CryptographicWeakness => Some(327),
            Self::Csrf => Some(352),
            Self::OpenRedirect => Some(601),
        }
    }

    /// Returns the severity level (1-10 scale, 10 being most severe).
    pub fn severity(&self) -> u8 {
        match self {
            Self::SqlInjection => 9,
            Self::Xss => 7,
            Self::AuthenticationBypass => 10,
            Self::RaceCondition => 6,
            Self::MemoryLeak => 4,
            Self::PathTraversal => 8,
            Self::InsecureDeserialization => 9,
            Self::CommandInjection => 10,
            Self::Ssrf => 8,
            Self::Idor => 7,
            Self::HardcodedCredentials => 8,
            Self::MissingInputValidation => 5,
            Self::InformationLeakage => 5,
            Self::BufferOverflow => 9,
            Self::UseAfterFree => 9,
            Self::NullPointerDereference => 6,
            Self::IntegerOverflow => 7,
            Self::CryptographicWeakness => 8,
            Self::Csrf => 6,
            Self::OpenRedirect => 5,
        }
    }

    /// Returns languages where this vulnerability is commonly found.
    pub fn applicable_languages(&self) -> &'static [WorkspaceLanguage] {
        use WorkspaceLanguage::*;
        match self {
            Self::SqlInjection => &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust],
            Self::Xss => &[JavaScript, TypeScript, Python, Java, Php, Ruby],
            Self::AuthenticationBypass => {
                &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust]
            }
            Self::RaceCondition => &[Python, JavaScript, Go, Rust, Java, Cpp],
            Self::MemoryLeak => &[Cpp, Rust, Go, Python, Java],
            Self::PathTraversal => &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go],
            Self::InsecureDeserialization => &[Python, JavaScript, Java, Php, Ruby],
            Self::CommandInjection => &[Python, JavaScript, TypeScript, Php, Ruby, Go],
            Self::Ssrf => &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go],
            Self::Idor => &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go],
            Self::HardcodedCredentials => &[
                Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust, Cpp,
            ],
            Self::MissingInputValidation => {
                &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust]
            }
            Self::InformationLeakage => {
                &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust]
            }
            Self::BufferOverflow => &[Cpp, Rust],
            Self::UseAfterFree => &[Cpp, Rust],
            Self::NullPointerDereference => &[Cpp, Rust, Go, Java],
            Self::IntegerOverflow => &[Cpp, Rust, Go, Java, Python],
            Self::CryptographicWeakness => {
                &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go, Rust]
            }
            Self::Csrf => &[Python, JavaScript, TypeScript, Java, Php, Ruby],
            Self::OpenRedirect => &[Python, JavaScript, TypeScript, Java, Php, Ruby, Go],
        }
    }

    /// Returns a human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SqlInjection => "SQL Injection",
            Self::Xss => "Cross-Site Scripting (XSS)",
            Self::AuthenticationBypass => "Authentication Bypass",
            Self::RaceCondition => "Race Condition",
            Self::MemoryLeak => "Memory Leak",
            Self::PathTraversal => "Path Traversal",
            Self::InsecureDeserialization => "Insecure Deserialization",
            Self::CommandInjection => "Command Injection",
            Self::Ssrf => "Server-Side Request Forgery (SSRF)",
            Self::Idor => "Insecure Direct Object Reference (IDOR)",
            Self::HardcodedCredentials => "Hardcoded Credentials",
            Self::MissingInputValidation => "Missing Input Validation",
            Self::InformationLeakage => "Information Leakage",
            Self::BufferOverflow => "Buffer Overflow",
            Self::UseAfterFree => "Use After Free",
            Self::NullPointerDereference => "Null Pointer Dereference",
            Self::IntegerOverflow => "Integer Overflow",
            Self::CryptographicWeakness => "Cryptographic Weakness",
            Self::Csrf => "Cross-Site Request Forgery (CSRF)",
            Self::OpenRedirect => "Open Redirect",
        }
    }
}

impl std::fmt::Display for VulnerabilityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Injected Vulnerability
// ============================================================================

/// Represents a vulnerability that has been injected into a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedVulnerability {
    /// Unique identifier for this injection.
    pub id: String,
    /// Type of vulnerability injected.
    pub vulnerability_type: VulnerabilityType,
    /// File path where the vulnerability was injected.
    pub file_path: PathBuf,
    /// Line number range where the vulnerability exists (start, end).
    pub line_range: (usize, usize),
    /// Description of the specific injection.
    pub description: String,
    /// The vulnerable code snippet.
    pub vulnerable_code: String,
    /// The correct/fixed code snippet.
    pub fixed_code: String,
    /// Hints that should be removed from the code (for cleaning).
    pub hints_to_remove: Vec<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl InjectedVulnerability {
    /// Creates a new injected vulnerability record.
    pub fn new(
        vulnerability_type: VulnerabilityType,
        file_path: impl Into<PathBuf>,
        line_range: (usize, usize),
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            vulnerability_type,
            file_path: file_path.into(),
            line_range,
            description: String::new(),
            vulnerable_code: String::new(),
            fixed_code: String::new(),
            hints_to_remove: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
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

    /// Adds hints that should be removed during cleaning.
    pub fn with_hints_to_remove<I, S>(mut self, hints: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.hints_to_remove
            .extend(hints.into_iter().map(Into::into));
        self
    }

    /// Adds metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), json_value);
        }
        self
    }

    /// Returns the CWE identifier for this vulnerability.
    pub fn cwe_id(&self) -> Option<u32> {
        self.vulnerability_type.cwe_id()
    }

    /// Returns the severity level.
    pub fn severity(&self) -> u8 {
        self.vulnerability_type.severity()
    }
}

// ============================================================================
// Workspace File
// ============================================================================

/// Represents a file in the generated workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// Relative path within the workspace.
    pub path: PathBuf,
    /// File content.
    pub content: String,
    /// Whether this file is executable.
    pub executable: bool,
    /// File type/purpose.
    pub file_type: WorkspaceFileType,
    /// Injected vulnerabilities in this file.
    pub vulnerabilities: Vec<String>,
}

impl WorkspaceFile {
    /// Creates a new workspace file.
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            executable: false,
            file_type: WorkspaceFileType::Source,
            vulnerabilities: Vec::new(),
        }
    }

    /// Creates a source file.
    pub fn source(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self::new(path, content).with_type(WorkspaceFileType::Source)
    }

    /// Creates a test file.
    pub fn test(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self::new(path, content).with_type(WorkspaceFileType::Test)
    }

    /// Creates a configuration file.
    pub fn config(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self::new(path, content).with_type(WorkspaceFileType::Configuration)
    }

    /// Sets the file type.
    pub fn with_type(mut self, file_type: WorkspaceFileType) -> Self {
        self.file_type = file_type;
        self
    }

    /// Marks the file as executable.
    pub fn with_executable(mut self, executable: bool) -> Self {
        self.executable = executable;
        self
    }

    /// Associates vulnerability IDs with this file.
    pub fn with_vulnerabilities<I, S>(mut self, vuln_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.vulnerabilities
            .extend(vuln_ids.into_iter().map(Into::into));
        self
    }

    /// Returns the file extension.
    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|ext| ext.to_str())
    }

    /// Returns the file name.
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|name| name.to_str())
    }
}

/// Types of files in a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceFileType {
    /// Source code file.
    Source,
    /// Test file.
    Test,
    /// Configuration file (e.g., package.json, Cargo.toml).
    Configuration,
    /// Build script.
    BuildScript,
    /// Documentation file.
    Documentation,
    /// Data/fixture file.
    Data,
    /// Other/miscellaneous file.
    Other,
}

// ============================================================================
// Verification Script
// ============================================================================

/// Script for verifying that fixes were correctly applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationScript {
    /// Script identifier.
    pub id: String,
    /// Script name (e.g., "test_sql_injection_fix.sh").
    pub name: String,
    /// Script content.
    pub content: String,
    /// Script type/interpreter.
    pub script_type: ScriptType,
    /// Expected exit code for success.
    pub expected_exit_code: i32,
    /// Description of what the script verifies.
    pub description: String,
    /// Vulnerabilities this script tests.
    pub tests_vulnerabilities: Vec<String>,
}

impl VerificationScript {
    /// Creates a new verification script.
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            content: content.into(),
            script_type: ScriptType::Bash,
            expected_exit_code: 0,
            description: String::new(),
            tests_vulnerabilities: Vec::new(),
        }
    }

    /// Creates a bash verification script.
    pub fn bash(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(name, content).with_type(ScriptType::Bash)
    }

    /// Creates a Python verification script.
    pub fn python(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(name, content).with_type(ScriptType::Python)
    }

    /// Sets the script type.
    pub fn with_type(mut self, script_type: ScriptType) -> Self {
        self.script_type = script_type;
        self
    }

    /// Sets the expected exit code.
    pub fn with_expected_exit_code(mut self, code: i32) -> Self {
        self.expected_exit_code = code;
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the vulnerabilities this script tests.
    pub fn with_tests_vulnerabilities<I, S>(mut self, vuln_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tests_vulnerabilities
            .extend(vuln_ids.into_iter().map(Into::into));
        self
    }

    /// Returns the shebang line for this script type.
    pub fn shebang(&self) -> &'static str {
        self.script_type.shebang()
    }
}

/// Types of verification scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptType {
    /// Bash shell script.
    Bash,
    /// Python script.
    Python,
    /// Node.js script.
    Node,
    /// PowerShell script.
    PowerShell,
}

impl ScriptType {
    /// Returns the shebang line for this script type.
    pub fn shebang(&self) -> &'static str {
        match self {
            Self::Bash => "#!/bin/bash",
            Self::Python => "#!/usr/bin/env python3",
            Self::Node => "#!/usr/bin/env node",
            Self::PowerShell => "#!/usr/bin/env pwsh",
        }
    }

    /// Returns the file extension for this script type.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Bash => ".sh",
            Self::Python => ".py",
            Self::Node => ".js",
            Self::PowerShell => ".ps1",
        }
    }
}

// ============================================================================
// Workspace Specification
// ============================================================================

/// Specification for a workspace to be generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSpec {
    /// Unique identifier for this workspace.
    pub id: String,
    /// Human-readable name for the workspace.
    pub name: String,
    /// Description of the task/challenge.
    pub description: String,
    /// Primary programming language.
    pub language: WorkspaceLanguage,
    /// Types of vulnerabilities to inject.
    pub vulnerability_types: Vec<VulnerabilityType>,
    /// Difficulty level (1-10 scale).
    pub difficulty: u8,
    /// Project type/template (e.g., "web-api", "cli-tool", "library").
    pub project_type: String,
    /// Additional configuration.
    pub config: HashMap<String, serde_json::Value>,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Seed for reproducible generation (optional).
    pub seed: Option<u64>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl WorkspaceSpec {
    /// Creates a new workspace specification.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            description: String::new(),
            language: WorkspaceLanguage::default(),
            vulnerability_types: Vec::new(),
            difficulty: 5,
            project_type: "web-api".to_string(),
            config: HashMap::new(),
            tags: Vec::new(),
            seed: None,
            created_at: Utc::now(),
        }
    }

    /// Sets the workspace name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the workspace description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the programming language.
    pub fn with_language(mut self, language: WorkspaceLanguage) -> Self {
        self.language = language;
        self
    }

    /// Adds a vulnerability type to inject.
    pub fn with_vulnerability(mut self, vuln_type: VulnerabilityType) -> Self {
        self.vulnerability_types.push(vuln_type);
        self
    }

    /// Adds multiple vulnerability types.
    pub fn with_vulnerabilities<I>(mut self, vuln_types: I) -> Self
    where
        I: IntoIterator<Item = VulnerabilityType>,
    {
        self.vulnerability_types.extend(vuln_types);
        self
    }

    /// Sets the difficulty level (1-10).
    pub fn with_difficulty(mut self, difficulty: u8) -> Self {
        self.difficulty = difficulty.clamp(1, 10);
        self
    }

    /// Sets the project type.
    pub fn with_project_type(mut self, project_type: impl Into<String>) -> Self {
        self.project_type = project_type.into();
        self
    }

    /// Adds configuration value.
    pub fn with_config(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.config.insert(key.into(), json_value);
        }
        self
    }

    /// Sets the tags.
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the random seed for reproducible generation.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Validates the specification.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Workspace ID cannot be empty".to_string());
        }

        if self.vulnerability_types.is_empty() {
            return Err("At least one vulnerability type must be specified".to_string());
        }

        // Check that all vulnerability types are applicable to the language
        for vuln_type in &self.vulnerability_types {
            if !vuln_type.applicable_languages().contains(&self.language) {
                return Err(format!(
                    "Vulnerability type {} is not applicable to language {}",
                    vuln_type, self.language
                ));
            }
        }

        Ok(())
    }
}

// ============================================================================
// Generated Workspace
// ============================================================================

/// A completely generated workspace with all files and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkspace {
    /// Unique identifier for this workspace.
    pub id: String,
    /// The specification used to generate this workspace.
    pub spec: WorkspaceSpec,
    /// All files in the workspace.
    pub files: Vec<WorkspaceFile>,
    /// All injected vulnerabilities.
    pub vulnerabilities: Vec<InjectedVulnerability>,
    /// Verification scripts.
    pub verification_scripts: Vec<VerificationScript>,
    /// Task prompt/instruction for the agent.
    pub task_prompt: String,
    /// Hidden solution description (for grading).
    pub solution_description: String,
    /// Anti-memorization canary token.
    pub canary_token: String,
    /// Generation timestamp.
    pub generated_at: DateTime<Utc>,
    /// Generation metadata (model used, tokens, etc.).
    pub generation_metadata: HashMap<String, serde_json::Value>,
}

impl GeneratedWorkspace {
    /// Creates a new generated workspace.
    pub fn new(spec: WorkspaceSpec) -> Self {
        let id = spec.id.clone();
        Self {
            id: id.clone(),
            spec,
            files: Vec::new(),
            vulnerabilities: Vec::new(),
            verification_scripts: Vec::new(),
            task_prompt: String::new(),
            solution_description: String::new(),
            canary_token: format!(
                "CANARY_{}_{:x}",
                id,
                uuid::Uuid::new_v4().as_u128() & 0xFFFFFFFF
            ),
            generated_at: Utc::now(),
            generation_metadata: HashMap::new(),
        }
    }

    /// Adds a file to the workspace.
    pub fn add_file(&mut self, file: WorkspaceFile) {
        self.files.push(file);
    }

    /// Adds multiple files to the workspace.
    pub fn add_files<I>(&mut self, files: I)
    where
        I: IntoIterator<Item = WorkspaceFile>,
    {
        self.files.extend(files);
    }

    /// Adds an injected vulnerability.
    pub fn add_vulnerability(&mut self, vuln: InjectedVulnerability) {
        self.vulnerabilities.push(vuln);
    }

    /// Adds a verification script.
    pub fn add_verification_script(&mut self, script: VerificationScript) {
        self.verification_scripts.push(script);
    }

    /// Sets the task prompt.
    pub fn with_task_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.task_prompt = prompt.into();
        self
    }

    /// Sets the solution description.
    pub fn with_solution_description(mut self, description: impl Into<String>) -> Self {
        self.solution_description = description.into();
        self
    }

    /// Adds generation metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.generation_metadata.insert(key.into(), json_value);
        }
        self
    }

    /// Returns the total number of lines of code.
    pub fn total_lines_of_code(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.file_type == WorkspaceFileType::Source)
            .map(|f| f.content.lines().count())
            .sum()
    }

    /// Returns all source files.
    pub fn source_files(&self) -> Vec<&WorkspaceFile> {
        self.files
            .iter()
            .filter(|f| f.file_type == WorkspaceFileType::Source)
            .collect()
    }

    /// Returns all test files.
    pub fn test_files(&self) -> Vec<&WorkspaceFile> {
        self.files
            .iter()
            .filter(|f| f.file_type == WorkspaceFileType::Test)
            .collect()
    }

    /// Returns the file at the given path.
    pub fn get_file(&self, path: impl AsRef<std::path::Path>) -> Option<&WorkspaceFile> {
        let path = path.as_ref();
        self.files.iter().find(|f| f.path == path)
    }

    /// Returns a mutable reference to the file at the given path.
    pub fn get_file_mut(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Option<&mut WorkspaceFile> {
        let path = path.as_ref();
        self.files.iter_mut().find(|f| f.path == path)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_language_extensions() {
        assert_eq!(WorkspaceLanguage::Python.file_extension(), ".py");
        assert_eq!(WorkspaceLanguage::JavaScript.file_extension(), ".js");
        assert_eq!(WorkspaceLanguage::Rust.file_extension(), ".rs");
    }

    #[test]
    fn test_workspace_language_display() {
        assert_eq!(WorkspaceLanguage::Python.display_name(), "Python");
        assert_eq!(WorkspaceLanguage::TypeScript.display_name(), "TypeScript");
        assert_eq!(WorkspaceLanguage::Cpp.display_name(), "C++");
    }

    #[test]
    fn test_vulnerability_type_cwe_ids() {
        assert_eq!(VulnerabilityType::SqlInjection.cwe_id(), Some(89));
        assert_eq!(VulnerabilityType::Xss.cwe_id(), Some(79));
        assert_eq!(VulnerabilityType::PathTraversal.cwe_id(), Some(22));
    }

    #[test]
    fn test_vulnerability_type_severity() {
        assert!(VulnerabilityType::AuthenticationBypass.severity() >= 9);
        assert!(VulnerabilityType::MemoryLeak.severity() <= 5);
    }

    #[test]
    fn test_vulnerability_type_applicable_languages() {
        let langs = VulnerabilityType::BufferOverflow.applicable_languages();
        assert!(langs.contains(&WorkspaceLanguage::Cpp));
        assert!(langs.contains(&WorkspaceLanguage::Rust));
        assert!(!langs.contains(&WorkspaceLanguage::Python));
    }

    #[test]
    fn test_workspace_spec_builder() {
        let spec = WorkspaceSpec::new("test-workspace")
            .with_name("Test Workspace")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection)
            .with_difficulty(7)
            .with_tags(["security", "sql"]);

        assert_eq!(spec.id, "test-workspace");
        assert_eq!(spec.name, "Test Workspace");
        assert_eq!(spec.language, WorkspaceLanguage::Python);
        assert_eq!(spec.vulnerability_types.len(), 1);
        assert_eq!(spec.difficulty, 7);
        assert_eq!(spec.tags.len(), 2);
    }

    #[test]
    fn test_workspace_spec_validation() {
        let valid_spec = WorkspaceSpec::new("test")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection);
        assert!(valid_spec.validate().is_ok());

        let empty_id = WorkspaceSpec::new("");
        assert!(empty_id.validate().is_err());

        let no_vulns = WorkspaceSpec::new("test");
        assert!(no_vulns.validate().is_err());

        let wrong_lang = WorkspaceSpec::new("test")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::BufferOverflow);
        assert!(wrong_lang.validate().is_err());
    }

    #[test]
    fn test_workspace_file_creation() {
        let file = WorkspaceFile::source("src/main.py", "print('hello')");
        assert_eq!(file.file_type, WorkspaceFileType::Source);
        assert_eq!(file.extension(), Some("py"));
        assert!(!file.executable);
    }

    #[test]
    fn test_injected_vulnerability_builder() {
        let vuln =
            InjectedVulnerability::new(VulnerabilityType::SqlInjection, "src/db.py", (10, 15))
                .with_description("SQL injection in user query")
                .with_vulnerable_code("cursor.execute(f\"SELECT * FROM users WHERE id={id}\")")
                .with_fixed_code("cursor.execute(\"SELECT * FROM users WHERE id=?\", (id,))");

        assert_eq!(vuln.vulnerability_type, VulnerabilityType::SqlInjection);
        assert_eq!(vuln.line_range, (10, 15));
        assert!(!vuln.description.is_empty());
    }

    #[test]
    fn test_verification_script_creation() {
        let script = VerificationScript::bash("test_fix.sh", "#!/bin/bash\npytest tests/")
            .with_description("Run security tests")
            .with_expected_exit_code(0);

        assert_eq!(script.script_type, ScriptType::Bash);
        assert_eq!(script.expected_exit_code, 0);
        assert_eq!(script.shebang(), "#!/bin/bash");
    }

    #[test]
    fn test_generated_workspace_creation() {
        let spec = WorkspaceSpec::new("test-ws")
            .with_language(WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection);

        let mut workspace = GeneratedWorkspace::new(spec);
        workspace.add_file(WorkspaceFile::source("src/main.py", "# code"));
        workspace.add_file(WorkspaceFile::test("tests/test_main.py", "# tests"));

        assert_eq!(workspace.files.len(), 2);
        assert_eq!(workspace.source_files().len(), 1);
        assert_eq!(workspace.test_files().len(), 1);
        assert!(workspace.canary_token.starts_with("CANARY_test-ws_"));
    }

    #[test]
    fn test_script_type_extensions() {
        assert_eq!(ScriptType::Bash.extension(), ".sh");
        assert_eq!(ScriptType::Python.extension(), ".py");
        assert_eq!(ScriptType::Node.extension(), ".js");
    }

    #[test]
    fn test_difficulty_clamping() {
        let spec = WorkspaceSpec::new("test").with_difficulty(15);
        assert_eq!(spec.difficulty, 10);

        let spec2 = WorkspaceSpec::new("test").with_difficulty(0);
        assert_eq!(spec2.difficulty, 1);
    }

    #[test]
    fn test_workspace_file_serialization() {
        let file = WorkspaceFile::source("src/main.py", "print('hello')");
        let json = serde_json::to_string(&file).expect("should serialize");
        let parsed: WorkspaceFile = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(file.path, parsed.path);
        assert_eq!(file.content, parsed.content);
    }

    #[test]
    fn test_vulnerability_type_serialization() {
        let vuln = VulnerabilityType::SqlInjection;
        let json = serde_json::to_string(&vuln).expect("should serialize");
        assert_eq!(json, "\"sql_injection\"");
        let parsed: VulnerabilityType = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(vuln, parsed);
    }
}
