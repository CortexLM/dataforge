//! Code Cleaner Agent for removing vulnerability hints and markers.
//!
//! This agent reviews generated code and removes any comments, markers,
//! or patterns that could reveal injected vulnerabilities. The goal is
//! to make the code look natural, as if written by a real developer.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::{AgentError, AgentResult};
use super::vulnerability_injector::{InjectionResult, VulnerableFile};
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for code cleaning.
const CODE_CLEANING_SYSTEM_PROMPT: &str = r#"You are a code reviewer ensuring generated code looks completely natural.

Your task is to clean code by removing ANY hints that could reveal:
1. That vulnerabilities were intentionally injected
2. Where security issues might exist
3. That this is benchmark/test code

THINGS TO REMOVE OR MODIFY:

COMMENTS:
- Remove TODO, FIXME, HACK, XXX comments
- Remove comments mentioning vulnerabilities, security, or injection
- Remove comments that say "this is intentional" or similar
- Remove comments explaining why code is insecure
- Keep ONLY legitimate documentation comments

MARKERS:
- Remove any "VULNERABLE" or "INSECURE" labels
- Remove any test markers or benchmark identifiers
- Remove numbered comments like "// VULN-001"
- Remove any CWE or CVE references in comments

SUSPICIOUS PATTERNS:
- Rename variables like "vulnerable_query" to natural names
- Rename functions like "unsafe_execute" to normal names
- Remove any debug comments about security testing
- Remove print/log statements that reveal security issues

PRESERVE:
- Legitimate code documentation
- Function/class docstrings that explain functionality
- License headers and attribution comments
- Normal developer comments about logic

CODE QUALITY:
- Ensure consistent code style throughout
- Make sure the code looks professional
- Variable and function names should be typical developer choices
- The code should pass a code review without raising suspicion

OUTPUT REQUIREMENTS:
- Return the cleaned file content
- Track all changes made for audit purposes
- Preserve all functionality - only clean metadata"#;

/// User prompt template for code cleaning.
const CODE_CLEANING_USER_TEMPLATE: &str = r#"Clean the following code files to remove any hints of vulnerability injection.

Files to Clean:
{files_content}

Known Vulnerabilities (for reference - ensure hints are removed):
{vulnerability_summary}

Requirements:
1. Remove all TODO, FIXME, HACK, XXX comments
2. Remove any comments hinting at vulnerabilities
3. Rename suspicious variable/function names to natural ones
4. Ensure the code looks like production code
5. Preserve all functionality

You MUST respond with ONLY valid JSON:
{{
  "cleaned_files": [
    {{
      "path": "path/to/file",
      "content": "complete cleaned file content",
      "changes_made": ["list of changes made to this file"]
    }}
  ],
  "total_changes": 0,
  "cleaning_notes": "any notes about the cleaning process"
}}"#;

/// A cleaned file with change tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanedFile {
    /// File path.
    pub path: String,
    /// Cleaned content.
    pub content: String,
    /// List of changes made.
    pub changes_made: Vec<String>,
}

impl CleanedFile {
    /// Creates a new cleaned file.
    pub fn new(
        path: impl Into<String>,
        content: impl Into<String>,
        changes_made: Vec<String>,
    ) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            changes_made,
        }
    }

    /// Returns the number of changes made.
    pub fn change_count(&self) -> usize {
        self.changes_made.len()
    }

    /// Returns whether any changes were made.
    pub fn was_modified(&self) -> bool {
        !self.changes_made.is_empty()
    }
}

/// Result of code cleaning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleaningResult {
    /// Unique identifier.
    pub id: String,
    /// Source injection result ID.
    pub source_injection_id: String,
    /// Cleaned files.
    pub cleaned_files: Vec<CleanedFile>,
    /// Total number of changes made.
    pub total_changes: usize,
    /// Notes about the cleaning process.
    pub cleaning_notes: String,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
}

impl CleaningResult {
    /// Creates a new cleaning result.
    pub fn new(
        source_injection_id: impl Into<String>,
        cleaned_files: Vec<CleanedFile>,
        cleaning_notes: impl Into<String>,
    ) -> Self {
        let total_changes: usize = cleaned_files.iter().map(|f| f.change_count()).sum();

        Self {
            id: Uuid::new_v4().to_string(),
            source_injection_id: source_injection_id.into(),
            cleaned_files,
            total_changes,
            cleaning_notes: cleaning_notes.into(),
            created_at: Utc::now(),
        }
    }

    /// Returns files that were modified.
    pub fn modified_files(&self) -> Vec<&CleanedFile> {
        self.cleaned_files
            .iter()
            .filter(|f| f.was_modified())
            .collect()
    }

    /// Gets a file by path.
    pub fn get_file(&self, path: &str) -> Option<&CleanedFile> {
        self.cleaned_files.iter().find(|f| f.path == path)
    }
}

/// Configuration for the Code Cleaner Agent.
#[derive(Debug, Clone)]
pub struct CodeCleanerConfig {
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for response.
    pub max_tokens: u32,
    /// Patterns to always remove from comments.
    pub forbidden_patterns: Vec<String>,
}

impl Default for CodeCleanerConfig {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            max_tokens: 16000,
            forbidden_patterns: vec![
                "TODO".to_string(),
                "FIXME".to_string(),
                "HACK".to_string(),
                "XXX".to_string(),
                "VULNERABLE".to_string(),
                "INSECURE".to_string(),
                "VULN".to_string(),
                "CVE-".to_string(),
                "CWE-".to_string(),
            ],
        }
    }
}

impl CodeCleanerConfig {
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

    /// Adds a forbidden pattern.
    pub fn with_forbidden_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.forbidden_patterns.push(pattern.into());
        self
    }

    /// Sets all forbidden patterns.
    pub fn with_forbidden_patterns(mut self, patterns: Vec<String>) -> Self {
        self.forbidden_patterns = patterns;
        self
    }
}

/// Code Cleaner Agent.
pub struct CodeCleanerAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: CodeCleanerConfig,
}

impl std::fmt::Debug for CodeCleanerAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeCleanerAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CodeCleanerAgent {
    /// Agent name constant.
    pub const AGENT_NAME: &'static str = "code_cleaner";

    /// Creates a new code cleaner agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: CodeCleanerConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, CodeCleanerConfig::default())
    }

    /// Cleans code from an injection result.
    pub async fn clean_code(
        &self,
        injection_result: &InjectionResult,
    ) -> AgentResult<CleaningResult> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.attempt_clean(injection_result).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "Code cleaning failed, retrying..."
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.expect("should have an error after 3 failed attempts"))
    }

    /// Cleans individual files.
    pub async fn clean_files(
        &self,
        files: &[VulnerableFile],
        vulnerability_summary: &str,
    ) -> AgentResult<CleaningResult> {
        let prompt = self.build_prompt_from_files(files, vulnerability_summary);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(CODE_CLEANING_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_response(content, "manual")
    }

    /// Attempts a single cleaning.
    async fn attempt_clean(
        &self,
        injection_result: &InjectionResult,
    ) -> AgentResult<CleaningResult> {
        let prompt = self.build_prompt(injection_result);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(CODE_CLEANING_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_response(content, &injection_result.id)
    }

    /// Builds the user prompt.
    fn build_prompt(&self, injection_result: &InjectionResult) -> String {
        let files_content = injection_result
            .modified_files
            .iter()
            .map(|f| format!("--- {} ---\n{}\n", f.path, f.content))
            .collect::<Vec<_>>()
            .join("\n");

        let vulnerability_summary = injection_result
            .vulnerabilities
            .iter()
            .map(|v| {
                format!(
                    "- {} in {} (lines {}-{})",
                    v.vulnerability_type, v.file_path, v.line_range.0, v.line_range.1
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        CODE_CLEANING_USER_TEMPLATE
            .replace("{files_content}", &files_content)
            .replace("{vulnerability_summary}", &vulnerability_summary)
    }

    /// Builds prompt from individual files.
    fn build_prompt_from_files(
        &self,
        files: &[VulnerableFile],
        vulnerability_summary: &str,
    ) -> String {
        let files_content = files
            .iter()
            .map(|f| format!("--- {} ---\n{}\n", f.path, f.content))
            .collect::<Vec<_>>()
            .join("\n");

        CODE_CLEANING_USER_TEMPLATE
            .replace("{files_content}", &files_content)
            .replace("{vulnerability_summary}", vulnerability_summary)
    }

    /// Parses the LLM response.
    fn parse_response(&self, content: &str, source_id: &str) -> AgentResult<CleaningResult> {
        let json_content = self.extract_json(content)?;

        let parsed: CleaningResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let cleaned_files: Vec<CleanedFile> = parsed
            .cleaned_files
            .into_iter()
            .map(|f| CleanedFile::new(f.path, f.content, f.changes_made))
            .collect();

        Ok(CleaningResult::new(
            source_id,
            cleaned_files,
            parsed.cleaning_notes,
        ))
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

    /// Performs basic pattern-based cleaning without LLM.
    pub fn quick_clean(&self, content: &str) -> String {
        let mut result = content.to_string();

        // Remove lines containing forbidden patterns (in comments only)
        for pattern in &self.config.forbidden_patterns {
            result = self.remove_pattern_comments(&result, pattern);
        }

        result
    }

    /// Removes comments containing a specific pattern.
    fn remove_pattern_comments(&self, content: &str, pattern: &str) -> String {
        content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                // Keep the line if it's not a comment containing the pattern
                if trimmed.starts_with("//")
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    !trimmed.to_uppercase().contains(&pattern.to_uppercase())
                } else {
                    true
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns the configuration.
    pub fn config(&self) -> &CodeCleanerConfig {
        &self.config
    }
}

/// Response structure from LLM.
#[derive(Debug, Deserialize)]
struct CleaningResponse {
    cleaned_files: Vec<CleanedFileResponse>,
    #[serde(default)]
    #[allow(dead_code)]
    total_changes: usize,
    #[serde(default)]
    cleaning_notes: String,
}

#[derive(Debug, Deserialize)]
struct CleanedFileResponse {
    path: String,
    content: String,
    #[serde(default)]
    changes_made: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::vulnerability_injector::{InjectedVulnerability, VulnerabilityType};
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
        r#"{
            "cleaned_files": [
                {
                    "path": "src/auth.py",
                    "content": "def get_user(user_id):\n    query = f\"SELECT * FROM users WHERE id = {user_id}\"\n    return db.execute(query)\n",
                    "changes_made": ["Removed TODO comment on line 1"]
                }
            ],
            "total_changes": 1,
            "cleaning_notes": "Removed vulnerability hints and cleaned code"
        }"#
        .to_string()
    }

    #[test]
    fn test_cleaned_file() {
        let file = CleanedFile::new(
            "src/main.py",
            "print('hello')",
            vec!["Removed TODO".to_string()],
        );

        assert_eq!(file.path, "src/main.py");
        assert_eq!(file.change_count(), 1);
        assert!(file.was_modified());
    }

    #[test]
    fn test_cleaning_result() {
        let files = vec![
            CleanedFile::new("file1.py", "content1", vec!["change1".to_string()]),
            CleanedFile::new("file2.py", "content2", vec![]),
        ];

        let result = CleaningResult::new("injection-1", files, "Cleaned successfully");

        assert_eq!(result.total_changes, 1);
        assert_eq!(result.modified_files().len(), 1);
        assert!(result.get_file("file1.py").is_some());
    }

    #[test]
    fn test_config_builder() {
        let config = CodeCleanerConfig::new()
            .with_temperature(0.3)
            .with_max_tokens(8000)
            .with_forbidden_pattern("DANGER");

        assert!((config.temperature - 0.3).abs() < 0.01);
        assert_eq!(config.max_tokens, 8000);
        assert!(config.forbidden_patterns.contains(&"DANGER".to_string()));
    }

    #[test]
    fn test_quick_clean() {
        let agent = CodeCleanerAgent::new(
            Arc::new(MockLlmProvider::new("")),
            CodeCleanerConfig::default(),
        );

        let content = r#"
# TODO: Fix this later
def foo():
    pass
# This is a normal comment
# FIXME: Security issue
"#;

        let cleaned = agent.quick_clean(content);
        assert!(!cleaned.contains("TODO"));
        assert!(!cleaned.contains("FIXME"));
        assert!(cleaned.contains("normal comment"));
    }

    #[test]
    fn test_remove_pattern_comments() {
        let agent = CodeCleanerAgent::new(
            Arc::new(MockLlmProvider::new("")),
            CodeCleanerConfig::default(),
        );

        let content = "// TODO: implement\ncode here\n// Normal comment";
        let result = agent.remove_pattern_comments(content, "TODO");

        assert!(!result.contains("TODO"));
        assert!(result.contains("code here"));
        assert!(result.contains("Normal comment"));
    }

    #[tokio::test]
    async fn test_clean_code() {
        let mock_llm = Arc::new(MockLlmProvider::new(&mock_response()));
        let agent = CodeCleanerAgent::with_defaults(mock_llm);

        let injection_result = InjectionResult::new(
            "workspace-1",
            vec![VulnerableFile {
                path: "src/auth.py".to_string(),
                content: "# TODO: vulnerable\ndef get_user(user_id):\n    query = f\"SELECT * FROM users WHERE id = {user_id}\"\n    return db.execute(query)\n".to_string(),
                original_hash: "abc123".to_string(),
            }],
            vec![InjectedVulnerability::new(
                VulnerabilityType::SqlInjection,
                "src/auth.py",
                (2, 3),
                "SQL injection",
            )],
        );

        let result = agent
            .clean_code(&injection_result)
            .await
            .expect("should clean code");

        assert_eq!(result.cleaned_files.len(), 1);
        assert_eq!(result.total_changes, 1);
    }
}
