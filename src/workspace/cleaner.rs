//! Workspace cleaning functionality.
//!
//! This module provides functionality to remove hints and comments that
//! might reveal vulnerability injection points from generated workspaces.
//!
//! # Overview
//!
//! The cleaner ensures that generated workspaces don't contain obvious
//! hints about where vulnerabilities are located. This includes:
//!
//! - Removing TODO, FIXME, XXX comments
//! - Removing comments that mention security terms
//! - Removing debug comments that reveal injection points
//! - Sanitizing variable names that hint at vulnerabilities
//!
//! # Example
//!
//! ```ignore
//! use dataforge::workspace::{WorkspaceCleaner, GeneratedWorkspace};
//!
//! let cleaner = WorkspaceCleaner::new()
//!     .with_remove_todos(true)
//!     .with_security_terms(vec!["vulnerability", "injection"]);
//!
//! let cleaned = cleaner.clean(&workspace)?;
//! ```

use regex::Regex;
use std::collections::HashSet;
use tracing::{debug, info, instrument};

use crate::error::GeneratorError;

use super::types::{GeneratedWorkspace, WorkspaceFile};

// ============================================================================
// Default Patterns
// ============================================================================

/// Returns default patterns that should be removed from code.
pub fn default_hint_patterns() -> Vec<&'static str> {
    vec![
        // Task management comments
        r"(?i)//\s*TODO[:\s].*",
        r"(?i)#\s*TODO[:\s].*",
        r"(?i)/\*\s*TODO[:\s].*\*/",
        r"(?i)//\s*FIXME[:\s].*",
        r"(?i)#\s*FIXME[:\s].*",
        r"(?i)/\*\s*FIXME[:\s].*\*/",
        r"(?i)//\s*XXX[:\s].*",
        r"(?i)#\s*XXX[:\s].*",
        r"(?i)//\s*HACK[:\s].*",
        r"(?i)#\s*HACK[:\s].*",
        r"(?i)//\s*BUG[:\s].*",
        r"(?i)#\s*BUG[:\s].*",
        // Security-related comments
        r"(?i)//\s*SECURITY[:\s].*",
        r"(?i)#\s*SECURITY[:\s].*",
        r"(?i)//\s*VULNERABLE[:\s].*",
        r"(?i)#\s*VULNERABLE[:\s].*",
        r"(?i)//\s*INSECURE[:\s].*",
        r"(?i)#\s*INSECURE[:\s].*",
        r"(?i)//\s*WARNING[:\s].*security.*",
        r"(?i)#\s*WARNING[:\s].*security.*",
        // Injection point hints
        r"(?i)//\s*INJECTION.*",
        r"(?i)#\s*INJECTION.*",
        r"(?i)//\s*VULN.*",
        r"(?i)#\s*VULN.*",
        // Debug markers
        r"(?i)//\s*DEBUG[:\s].*",
        r"(?i)#\s*DEBUG[:\s].*",
        r"(?i)//\s*TEST[:\s].*vulnerability.*",
        r"(?i)#\s*TEST[:\s].*vulnerability.*",
    ]
}

/// Returns security-related terms that hint at vulnerabilities.
pub fn default_security_terms() -> Vec<&'static str> {
    vec![
        "sql injection",
        "sqli",
        "xss",
        "cross-site scripting",
        "csrf",
        "cross-site request forgery",
        "authentication bypass",
        "auth bypass",
        "race condition",
        "memory leak",
        "path traversal",
        "directory traversal",
        "insecure deserialization",
        "command injection",
        "cmd injection",
        "ssrf",
        "server-side request forgery",
        "idor",
        "insecure direct object reference",
        "hardcoded credential",
        "hardcoded password",
        "hardcoded secret",
        "buffer overflow",
        "use after free",
        "null pointer dereference",
        "integer overflow",
        "cryptographic weakness",
        "open redirect",
        "cve-",
        "cwe-",
        "owasp",
    ]
}

// ============================================================================
// Cleaner Configuration
// ============================================================================

/// Configuration for workspace cleaning.
#[derive(Debug, Clone)]
pub struct CleanerConfig {
    /// Whether to remove TODO comments.
    pub remove_todos: bool,
    /// Whether to remove FIXME comments.
    pub remove_fixmes: bool,
    /// Whether to remove security-hinting comments.
    pub remove_security_hints: bool,
    /// Additional patterns to remove (regex).
    pub additional_patterns: Vec<String>,
    /// Security terms to look for in comments.
    pub security_terms: Vec<String>,
    /// Whether to report cleaning actions (verbose mode).
    pub verbose: bool,
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            remove_todos: true,
            remove_fixmes: true,
            remove_security_hints: true,
            additional_patterns: Vec::new(),
            security_terms: default_security_terms()
                .into_iter()
                .map(String::from)
                .collect(),
            verbose: false,
        }
    }
}

impl CleanerConfig {
    /// Creates a new cleaner config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether to remove TODO comments.
    pub fn with_remove_todos(mut self, remove: bool) -> Self {
        self.remove_todos = remove;
        self
    }

    /// Sets whether to remove FIXME comments.
    pub fn with_remove_fixmes(mut self, remove: bool) -> Self {
        self.remove_fixmes = remove;
        self
    }

    /// Sets whether to remove security hints.
    pub fn with_remove_security_hints(mut self, remove: bool) -> Self {
        self.remove_security_hints = remove;
        self
    }

    /// Adds additional patterns to remove.
    pub fn with_additional_patterns<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.additional_patterns
            .extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Adds security terms to filter.
    pub fn with_security_terms<I, S>(mut self, terms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.security_terms
            .extend(terms.into_iter().map(Into::into));
        self
    }

    /// Sets verbose mode.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

// ============================================================================
// Cleaning Result
// ============================================================================

/// Result of a cleaning operation.
#[derive(Debug, Clone, Default)]
pub struct CleaningResult {
    /// Number of files processed.
    pub files_processed: usize,
    /// Number of files modified.
    pub files_modified: usize,
    /// Total number of patterns matched.
    pub patterns_matched: usize,
    /// Total number of lines removed/modified.
    pub lines_modified: usize,
    /// Details of modifications per file.
    pub modifications: Vec<FileModification>,
}

impl CleaningResult {
    /// Creates a new cleaning result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file modification.
    pub fn add_modification(&mut self, modification: FileModification) {
        if modification.changes > 0 {
            self.files_modified += 1;
            self.patterns_matched += modification.changes;
            self.lines_modified += modification.lines_affected;
        }
        self.files_processed += 1;
        self.modifications.push(modification);
    }
}

/// Details of modifications to a single file.
#[derive(Debug, Clone)]
pub struct FileModification {
    /// File path.
    pub file_path: String,
    /// Number of changes made.
    pub changes: usize,
    /// Number of lines affected.
    pub lines_affected: usize,
    /// Description of changes.
    pub descriptions: Vec<String>,
}

impl FileModification {
    /// Creates a new file modification record.
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            changes: 0,
            lines_affected: 0,
            descriptions: Vec::new(),
        }
    }

    /// Records a change.
    pub fn record_change(&mut self, description: impl Into<String>, lines: usize) {
        self.changes += 1;
        self.lines_affected += lines;
        self.descriptions.push(description.into());
    }
}

// ============================================================================
// Workspace Cleaner
// ============================================================================

/// Cleaner for removing hints from generated workspaces.
pub struct WorkspaceCleaner {
    /// Cleaner configuration.
    config: CleanerConfig,
    /// Compiled regex patterns.
    patterns: Vec<Regex>,
}

impl WorkspaceCleaner {
    /// Creates a new workspace cleaner with default configuration.
    pub fn new() -> Self {
        let config = CleanerConfig::default();
        let patterns = Self::compile_patterns(&config);
        Self { config, patterns }
    }

    /// Creates a new workspace cleaner with custom configuration.
    pub fn with_config(config: CleanerConfig) -> Self {
        let patterns = Self::compile_patterns(&config);
        Self { config, patterns }
    }

    /// Returns the current configuration.
    pub fn config(&self) -> &CleanerConfig {
        &self.config
    }

    /// Compiles all patterns for the given configuration.
    fn compile_patterns(config: &CleanerConfig) -> Vec<Regex> {
        let mut patterns = Vec::new();

        // Add default hint patterns
        for pattern in default_hint_patterns() {
            if let Ok(regex) = Regex::new(pattern) {
                patterns.push(regex);
            }
        }

        // Add additional patterns
        for pattern in &config.additional_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                patterns.push(regex);
            }
        }

        patterns
    }

    /// Cleans a workspace, returning a new cleaned workspace.
    #[instrument(skip(self, workspace))]
    pub fn clean(&self, workspace: &GeneratedWorkspace) -> Result<GeneratedWorkspace, GeneratorError> {
        info!("Cleaning workspace: {}", workspace.id);

        let mut cleaned = workspace.clone();
        let mut result = CleaningResult::new();

        for file in &mut cleaned.files {
            let modification = self.clean_file(file)?;
            result.add_modification(modification);
        }

        info!(
            "Cleaning complete: {} files processed, {} modified, {} patterns matched",
            result.files_processed, result.files_modified, result.patterns_matched
        );

        Ok(cleaned)
    }

    /// Cleans a single file.
    fn clean_file(&self, file: &mut WorkspaceFile) -> Result<FileModification, GeneratorError> {
        let file_path = file.path.display().to_string();
        let mut modification = FileModification::new(&file_path);

        // Skip binary files and non-code files
        if self.is_binary_file(file) {
            return Ok(modification);
        }

        let original_content = file.content.clone();
        let mut cleaned_content = original_content.clone();

        // Apply all patterns
        for pattern in &self.patterns {
            let matches_before = pattern.find_iter(&cleaned_content).count();
            if matches_before > 0 {
                cleaned_content = pattern.replace_all(&cleaned_content, "").to_string();
                modification.record_change(
                    format!("Removed {} matches for pattern: {}", matches_before, pattern),
                    matches_before,
                );
            }
        }

        // Remove comments containing security terms
        if self.config.remove_security_hints {
            cleaned_content = self.remove_security_comments(&cleaned_content, &mut modification);
        }

        // Clean up empty lines left by removed comments
        cleaned_content = self.cleanup_empty_lines(&cleaned_content);

        if cleaned_content != original_content {
            file.content = cleaned_content;
            debug!("Modified file: {} ({} changes)", file_path, modification.changes);
        }

        Ok(modification)
    }

    /// Checks if a file is binary.
    fn is_binary_file(&self, file: &WorkspaceFile) -> bool {
        // Check by extension
        if let Some(ext) = file.extension() {
            let binary_extensions = [
                "pyc", "pyo", "pyd", "so", "dll", "dylib", "exe", "bin", "o", "a", "lib", "class",
                "jar", "war", "ear", "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "png", "jpg",
                "jpeg", "gif", "bmp", "ico", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
                "woff", "woff2", "ttf", "otf", "eot",
            ];
            if binary_extensions.contains(&ext.to_lowercase().as_str()) {
                return true;
            }
        }

        // Check for null bytes (binary content indicator)
        file.content.bytes().any(|b| b == 0)
    }

    /// Removes comments containing security terms.
    fn remove_security_comments(&self, content: &str, modification: &mut FileModification) -> String {
        let mut result = Vec::new();
        let terms: HashSet<_> = self
            .config
            .security_terms
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        for line in content.lines() {
            let line_lower = line.to_lowercase();
            let is_comment = self.is_comment_line(&line_lower);

            if is_comment {
                let contains_security_term = terms.iter().any(|term| line_lower.contains(term));
                if contains_security_term {
                    modification.record_change(
                        format!("Removed security-hinting comment: {}", line.trim()),
                        1,
                    );
                    continue; // Skip this line
                }
            }

            result.push(line);
        }

        result.join("\n")
    }

    /// Checks if a line is a comment.
    fn is_comment_line(&self, line: &str) -> bool {
        let trimmed = line.trim();

        // Single-line comment markers
        if trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("'''")
            || trimmed.starts_with("\"\"\"")
            || trimmed.starts_with("--")
            || trimmed.starts_with("rem ")
            || trimmed.starts_with(';')
        {
            return true;
        }

        false
    }

    /// Cleans up excessive empty lines.
    fn cleanup_empty_lines(&self, content: &str) -> String {
        // Replace 3+ consecutive empty lines with 2
        let re = Regex::new(r"\n{4,}").expect("Invalid regex for empty lines");
        re.replace_all(content, "\n\n\n").to_string()
    }

    /// Cleans content without needing a file context.
    pub fn clean_content(&self, content: &str) -> String {
        let mut cleaned = content.to_string();

        // Apply all patterns
        for pattern in &self.patterns {
            cleaned = pattern.replace_all(&cleaned, "").to_string();
        }

        // Remove security comments
        if self.config.remove_security_hints {
            let mut modification = FileModification::new("inline");
            cleaned = self.remove_security_comments(&cleaned, &mut modification);
        }

        // Cleanup empty lines
        self.cleanup_empty_lines(&cleaned)
    }
}

impl Default for WorkspaceCleaner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating WorkspaceCleaner instances.
pub struct WorkspaceCleanerBuilder {
    config: CleanerConfig,
}

impl WorkspaceCleanerBuilder {
    /// Creates a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: CleanerConfig::default(),
        }
    }

    /// Sets whether to remove TODO comments.
    pub fn remove_todos(mut self, remove: bool) -> Self {
        self.config.remove_todos = remove;
        self
    }

    /// Sets whether to remove FIXME comments.
    pub fn remove_fixmes(mut self, remove: bool) -> Self {
        self.config.remove_fixmes = remove;
        self
    }

    /// Sets whether to remove security hints.
    pub fn remove_security_hints(mut self, remove: bool) -> Self {
        self.config.remove_security_hints = remove;
        self
    }

    /// Adds additional patterns to remove.
    pub fn additional_patterns<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config
            .additional_patterns
            .extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Adds security terms to filter.
    pub fn security_terms<I, S>(mut self, terms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config
            .security_terms
            .extend(terms.into_iter().map(Into::into));
        self
    }

    /// Sets verbose mode.
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.config.verbose = verbose;
        self
    }

    /// Builds the cleaner.
    pub fn build(self) -> WorkspaceCleaner {
        WorkspaceCleaner::with_config(self.config)
    }
}

impl Default for WorkspaceCleanerBuilder {
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
    use crate::workspace::types::{VulnerabilityType, WorkspaceSpec};

    #[test]
    fn test_default_hint_patterns() {
        let patterns = default_hint_patterns();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_default_security_terms() {
        let terms = default_security_terms();
        assert!(terms.contains(&"sql injection"));
        assert!(terms.contains(&"xss"));
        assert!(terms.contains(&"authentication bypass"));
    }

    #[test]
    fn test_cleaner_config_defaults() {
        let config = CleanerConfig::default();
        assert!(config.remove_todos);
        assert!(config.remove_fixmes);
        assert!(config.remove_security_hints);
        assert!(!config.security_terms.is_empty());
    }

    #[test]
    fn test_cleaner_config_builder() {
        let config = CleanerConfig::new()
            .with_remove_todos(false)
            .with_verbose(true);

        assert!(!config.remove_todos);
        assert!(config.verbose);
    }

    #[test]
    fn test_clean_content_removes_todos() {
        let cleaner = WorkspaceCleaner::new();

        let content = r#"
def main():
    # TODO: Fix this later
    print("hello")
    // FIXME: This is broken
    return 0
"#;

        let cleaned = cleaner.clean_content(content);
        assert!(!cleaned.contains("TODO"));
        assert!(!cleaned.contains("FIXME"));
        assert!(cleaned.contains("print"));
    }

    #[test]
    fn test_clean_content_removes_security_hints() {
        let cleaner = WorkspaceCleaner::new();

        let content = r#"
def login(username, password):
    # SECURITY: This has SQL injection vulnerability
    query = f"SELECT * FROM users WHERE username='{username}'"
    # WARNING: Authentication bypass possible here
    return execute(query)
"#;

        let cleaned = cleaner.clean_content(content);
        assert!(!cleaned.to_lowercase().contains("sql injection"));
        assert!(!cleaned.to_lowercase().contains("authentication bypass"));
        assert!(cleaned.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_clean_content_removes_injection_markers() {
        let cleaner = WorkspaceCleaner::new();

        let content = r#"
// VULNERABLE: SQL injection here
const query = "SELECT * FROM users WHERE id=" + id;
# INJECTION POINT: XSS vulnerability
output = "<div>" + user_input + "</div>"
"#;

        let cleaned = cleaner.clean_content(content);
        assert!(!cleaned.to_lowercase().contains("vulnerable"));
        assert!(!cleaned.to_lowercase().contains("injection point"));
    }

    #[test]
    fn test_is_comment_line() {
        let cleaner = WorkspaceCleaner::new();

        assert!(cleaner.is_comment_line("// this is a comment"));
        assert!(cleaner.is_comment_line("# python comment"));
        assert!(cleaner.is_comment_line("/* c comment */"));
        assert!(cleaner.is_comment_line("-- sql comment"));
        assert!(!cleaner.is_comment_line("let x = 5;"));
        assert!(!cleaner.is_comment_line("print('hello')"));
    }

    #[test]
    fn test_cleaning_result() {
        let mut result = CleaningResult::new();

        let mut mod1 = FileModification::new("file1.py");
        mod1.record_change("Removed TODO", 1);
        result.add_modification(mod1);

        let mut mod2 = FileModification::new("file2.py");
        mod2.record_change("Removed FIXME", 2);
        mod2.record_change("Removed security comment", 1);
        result.add_modification(mod2);

        assert_eq!(result.files_processed, 2);
        assert_eq!(result.files_modified, 2);
        assert_eq!(result.patterns_matched, 3);
    }

    #[test]
    fn test_cleaner_builder() {
        let cleaner = WorkspaceCleanerBuilder::new()
            .remove_todos(false)
            .additional_patterns(vec![r"CUSTOM_MARKER"])
            .verbose(true)
            .build();

        assert!(!cleaner.config.remove_todos);
        assert!(cleaner.config.verbose);
    }

    #[test]
    fn test_cleanup_empty_lines() {
        let cleaner = WorkspaceCleaner::new();

        let content = "line1\n\n\n\n\n\nline2";
        let cleaned = cleaner.cleanup_empty_lines(content);

        // Should reduce to max 3 newlines (2 empty lines between)
        assert_eq!(cleaned.matches('\n').count(), 3);
    }

    #[test]
    fn test_is_binary_file() {
        let cleaner = WorkspaceCleaner::new();

        let text_file = WorkspaceFile::source("main.py", "print('hello')");
        assert!(!cleaner.is_binary_file(&text_file));

        let binary_ext = WorkspaceFile::new("file.pyc", "content");
        assert!(cleaner.is_binary_file(&binary_ext));

        let binary_content = WorkspaceFile::new("file.txt", "has\x00null");
        assert!(cleaner.is_binary_file(&binary_content));
    }

    #[test]
    fn test_clean_workspace() {
        let spec = WorkspaceSpec::new("test")
            .with_language(crate::workspace::types::WorkspaceLanguage::Python)
            .with_vulnerability(VulnerabilityType::SqlInjection);

        let mut workspace = GeneratedWorkspace::new(spec);
        workspace.add_file(WorkspaceFile::source(
            "main.py",
            r#"
# TODO: This needs fixing
# SECURITY: SQL injection vulnerability here
def query(id):
    return f"SELECT * FROM users WHERE id={id}"
"#,
        ));

        let cleaner = WorkspaceCleaner::new();
        let cleaned = cleaner.clean(&workspace).expect("should clean");

        let file = cleaned.get_file("main.py").expect("file should exist");
        assert!(!file.content.contains("TODO"));
        assert!(!file.content.to_lowercase().contains("sql injection"));
        assert!(file.content.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_clean_preserves_legitimate_code() {
        let cleaner = WorkspaceCleaner::new();

        let content = r#"
def handle_request(request):
    # Get user input (this is fine)
    user_id = request.get("id")
    
    # Execute query
    result = db.query(user_id)
    
    return result

class UserService:
    """Service for handling user operations."""
    
    def get_user(self, id):
        return self.repo.find(id)
"#;

        let cleaned = cleaner.clean_content(content);

        // Should preserve all the actual code
        assert!(cleaned.contains("def handle_request"));
        assert!(cleaned.contains("class UserService"));
        assert!(cleaned.contains("def get_user"));
        assert!(cleaned.contains("Get user input"));
    }

    #[test]
    fn test_multiple_comment_styles() {
        let cleaner = WorkspaceCleaner::new();

        let content = r#"
// TODO: C-style todo
# TODO: Python-style todo
/* TODO: Block comment todo */
-- TODO: SQL-style todo
; TODO: Assembly-style todo
"""
TODO: Docstring todo
"""
print("keep this")
"#;

        let cleaned = cleaner.clean_content(content);

        // All TODO comments should be removed
        let todo_count = cleaned.matches("TODO").count();
        // Only the docstring TODO might remain since it's inside a string
        assert!(todo_count <= 1);
        assert!(cleaned.contains("print"));
    }
}
