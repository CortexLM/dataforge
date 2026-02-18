//! SWE mining pipeline implementation.
//!
//! This module implements a real-data mining workflow inspired by SWE-Infinite:
//! - Pull PR/Issue events from GH Archive
//! - Enrich events through GitHub API
//! - Filter candidates by repo and patch quality
//! - Extract solution/test patches from PR diffs
//! - Score quality with existing DataForge agents
//! - Export tasks as `workspace.yaml` + `prompt.md`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod docker_sandbox;
pub mod enricher;
pub mod extractor;
pub mod filters;
pub mod gharchive;
pub mod github_search;
pub mod harness;
pub mod orchestrator;
pub mod pipeline;
pub mod pr_cache;
pub mod progress;
pub mod prompt_rewriter;
pub mod quality;
pub mod test_generator;
pub mod tool_server;
pub mod workspace_validator;

pub use enricher::EnrichedPullRequest;
pub use extractor::{ExtractedPatch, PatchExtractor, PatchExtractorConfig};
pub use filters::{FilterConfig, FilterResult, SweepFilter};
pub use gharchive::{GhArchiveClient, GhArchiveEvent, GhArchiveEventId};
pub use harness::{run_harness, HarnessConfig, HarnessResult, HarnessSummary};
pub use orchestrator::{SweOrchestrator, SweOrchestratorConfig, SweRunResult};
pub use pipeline::{BenchmarkMetrics, SwePipeline, SwePipelineEvent, SwePipelineRunResult};
pub use pr_cache::{OptionalCache, PrCache, PrCacheEntry};
pub use progress::{ProgressCounters, ProgressMonitor, ProgressSnapshot};
pub use prompt_rewriter::PromptRewriter;
pub use quality::{QualityAssessment, QualityConfig, QualityScorer};
pub use test_generator::{TestFile, TestGenerator};
pub use workspace_validator::{ValidationOutcome, WorkspaceValidator};

/// Default output directory for generated SWE workspaces.
pub const DEFAULT_SWE_OUTPUT_DIR: &str = "./generated-swe";

/// Validate a git ref (commit SHA, branch name) to prevent shell injection.
///
/// Accepts hex-only SHAs (short or full) and standard git ref names
/// (alphanumeric, `/`, `.`, `-`, `_`). Rejects shell metacharacters,
/// `..` sequences (path traversal), and refs starting with `-` (flag injection).
pub fn validate_git_ref(s: &str) -> Result<(), anyhow::Error> {
    if s.is_empty() {
        anyhow::bail!("git ref is empty");
    }
    if s.len() > 256 {
        anyhow::bail!("git ref too long ({} chars, max 256)", s.len());
    }
    if s.starts_with('-') {
        anyhow::bail!(
            "git ref '{}' must not start with '-' (could be interpreted as a flag)",
            s
        );
    }
    if s.contains("..") {
        anyhow::bail!("git ref '{}' must not contain '..' (path traversal)", s);
    }
    for ch in s.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '-' | '_' | '~' | '^') {
            anyhow::bail!(
                "invalid character '{}' in git ref '{}': only alphanumeric, /, ., -, _, ~, ^ allowed",
                ch,
                s
            );
        }
    }
    Ok(())
}

/// Validate a GitHub repository name (`owner/repo`) to prevent shell injection.
///
/// Accepts the standard GitHub `owner/repo` format where both parts contain
/// only alphanumeric characters, hyphens, underscores, and dots. Parts must
/// not start with `.` or `-` to prevent path traversal and flag injection.
pub fn validate_repo_name(s: &str) -> Result<(), anyhow::Error> {
    if s.is_empty() {
        anyhow::bail!("repository name is empty");
    }
    if s.len() > 256 {
        anyhow::bail!("repository name too long ({} chars, max 256)", s.len());
    }
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "invalid repository name '{}': expected 'owner/repo' format",
            s
        );
    }
    for part in &parts {
        if part.is_empty() {
            anyhow::bail!(
                "invalid repository name '{}': owner and repo must be non-empty",
                s
            );
        }
        if part.starts_with('.') || part.starts_with('-') {
            anyhow::bail!(
                "invalid repository name '{}': parts must not start with '.' or '-'",
                s
            );
        }
        for ch in part.chars() {
            if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.') {
                anyhow::bail!(
                    "invalid character '{}' in repository name '{}': only alphanumeric, -, _, . allowed",
                    ch,
                    s
                );
            }
        }
    }
    Ok(())
}

/// Validate a file path used in shell commands inside Docker containers.
///
/// Rejects paths containing shell metacharacters, single/double quotes,
/// null bytes, and `..` traversal sequences. This prevents shell injection
/// when paths are interpolated into commands like `cat > '/repo/{path}'`.
pub fn validate_file_path(path: &str) -> Result<(), anyhow::Error> {
    if path.is_empty() {
        anyhow::bail!("file path is empty");
    }
    if path.len() > 4096 {
        anyhow::bail!("file path too long ({} chars, max 4096)", path.len());
    }
    if path.contains('\0') {
        anyhow::bail!("file path contains null byte");
    }
    if path.contains("..") {
        anyhow::bail!(
            "file path '{}' contains '..' (path traversal not allowed)",
            path
        );
    }
    if path.starts_with('/') {
        anyhow::bail!("file path '{}' must be relative (no leading '/')", path);
    }
    for ch in path.chars() {
        if matches!(
            ch,
            '\'' | '"'
                | '`'
                | '$'
                | '!'
                | '&'
                | '|'
                | ';'
                | '('
                | ')'
                | '{'
                | '}'
                | '<'
                | '>'
                | '\\'
                | '\n'
                | '\r'
        ) {
            anyhow::bail!(
                "invalid character '{}' in file path '{}': shell metacharacters not allowed",
                ch,
                path
            );
        }
    }
    Ok(())
}

/// DataForge-compatible task format for SWE mined items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweTask {
    /// Stable identifier (`repo-<pr>-<issue>` or a random UUID).
    pub id: String,
    /// GitHub repository in `owner/repo` form.
    pub repo: String,
    /// Base commit SHA for the PR patch.
    pub base_commit: String,
    /// Merge commit SHA from the PR.
    pub merge_commit: String,
    /// Primary language inferred from repository metadata.
    pub language: String,
    /// Difficulty score used by DataForge consumers.
    pub difficulty_score: u8,
    /// Task creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Full solution patch (`git diff` style).
    pub patch: String,
    /// Test-only patch extracted from the PR.
    pub test_patch: String,
    /// Fail-to-pass test commands.
    pub fail_to_pass: Vec<String>,
    /// Regression/pass-to-pass test commands.
    pub pass_to_pass: Vec<String>,
    /// Install/configuration hints.
    pub install_config: BTreeMap<String, String>,
    /// Optional metadata from GitHub + extractor.
    pub meta: BTreeMap<String, String>,
    /// Human-readable prompt (LLM-rewritten, no test plan leak).
    pub prompt: String,
    /// Original PR body before rewriting.
    #[serde(default)]
    pub original_pr_body: String,
    /// Difficulty score from quality/validation phase.
    pub quality_score: Option<f64>,
    /// Whether task passed quality gate.
    pub quality_passed: bool,
    /// Docker validation result (best effort).
    pub docker_passed: bool,
    /// Optional workspace export path.
    pub workspace_path: Option<String>,
    /// Pipeline status.
    pub status: SweTaskStatus,
}

/// Status of a SWE mined task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SweTaskStatus {
    /// Candidate discovered but not yet processed.
    Candidate,
    /// Candidate filtered out.
    Rejected,
    /// Extracted and scored.
    Ready,
    /// Exported to disk.
    Exported,
    /// Validated (Docker optional).
    Validated,
}

impl SweTask {
    /// Creates a new task with sane defaults.
    pub fn new(id: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            repo: repo.into(),
            base_commit: String::new(),
            merge_commit: String::new(),
            language: String::from("unknown"),
            difficulty_score: 1,
            created_at: Utc::now(),
            patch: String::new(),
            test_patch: String::new(),
            fail_to_pass: Vec::new(),
            pass_to_pass: Vec::new(),
            install_config: BTreeMap::new(),
            meta: BTreeMap::new(),
            prompt: String::new(),
            original_pr_body: String::new(),
            quality_score: None,
            quality_passed: false,
            docker_passed: false,
            workspace_path: None,
            status: SweTaskStatus::Candidate,
        }
    }

    /// Returns true if the task contains at least one test.
    pub fn has_tests(&self) -> bool {
        !self.fail_to_pass.is_empty() || !self.pass_to_pass.is_empty()
    }

    /// Returns the standard build + test commands for a language.
    /// Used to inject real validation commands into generated tasks.
    pub fn test_commands_for_language(language: &str) -> (Vec<String>, Vec<String>) {
        let (build, test) = match language.to_lowercase().as_str() {
            "python" => (
                vec!["pip install -e .".to_string()],
                vec!["pytest".to_string()],
            ),
            "rust" => (
                vec!["cargo build".to_string()],
                vec!["cargo test".to_string()],
            ),
            "go" | "golang" => (
                vec!["go build ./...".to_string()],
                vec!["go test ./...".to_string()],
            ),
            "javascript" | "typescript" | "js" | "ts" => (
                vec!["npm install".to_string()],
                vec!["npm test".to_string()],
            ),
            "java" | "kotlin" => (
                vec!["./mvnw -q -DskipTests package".to_string()],
                vec!["./mvnw test".to_string()],
            ),
            _ => (vec![], vec![]),
        };
        // pass_to_pass = build commands, fail_to_pass = test commands
        (build, test)
    }

    /// Build a simple install command map based on language.
    pub fn install_defaults(language: &str) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        match language.to_lowercase().as_str() {
            "python" => {
                map.insert("python".to_string(), "3.11".to_string());
                map.insert("install".to_string(), "pip install -e .".to_string());
                map.insert("test_cmd".to_string(), "pytest".to_string());
            }
            "javascript" | "typescript" | "js" | "ts" => {
                map.insert("node".to_string(), "20".to_string());
                map.insert("install".to_string(), "npm install".to_string());
                map.insert("test_cmd".to_string(), "npm test".to_string());
            }
            "go" => {
                map.insert("go".to_string(), "1.22".to_string());
                map.insert("install".to_string(), "go mod download".to_string());
                map.insert("test_cmd".to_string(), "go test ./...".to_string());
            }
            "rust" => {
                map.insert("rust".to_string(), "1.75".to_string());
                map.insert("install".to_string(), "cargo fetch".to_string());
                map.insert("test_cmd".to_string(), "cargo test".to_string());
            }
            "java" => {
                map.insert("java".to_string(), "21".to_string());
                map.insert(
                    "install".to_string(),
                    "./mvnw -q -DskipTests package".to_string(),
                );
                map.insert("test_cmd".to_string(), "./mvnw test".to_string());
            }
            _ => {
                map.insert("install".to_string(), String::from("# manual install"));
                map.insert(
                    "test_cmd".to_string(),
                    String::from("# manual test command"),
                );
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_git_ref_accepts_hex_sha() {
        assert!(validate_git_ref("abc123def456").is_ok());
        assert!(validate_git_ref("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2").is_ok());
    }

    #[test]
    fn validate_git_ref_rejects_empty() {
        assert!(validate_git_ref("").is_err());
    }

    #[test]
    fn validate_git_ref_accepts_branch_names() {
        assert!(validate_git_ref("main").is_ok());
        assert!(validate_git_ref("feature/my-branch").is_ok());
        assert!(validate_git_ref("HEAD~1").is_ok());
        assert!(validate_git_ref("v1.2.3").is_ok());
    }

    #[test]
    fn validate_git_ref_rejects_shell_injection() {
        assert!(validate_git_ref("abc123; rm -rf /").is_err());
        assert!(validate_git_ref("$(whoami)").is_err());
        assert!(validate_git_ref("`id`").is_err());
        assert!(validate_git_ref("abc | cat /etc/passwd").is_err());
        assert!(validate_git_ref("abc && echo pwned").is_err());
    }

    #[test]
    fn validate_git_ref_rejects_double_dot() {
        assert!(validate_git_ref("main..HEAD").is_err());
        assert!(validate_git_ref("../../etc/passwd").is_err());
    }

    #[test]
    fn validate_git_ref_rejects_leading_dash() {
        assert!(validate_git_ref("--exec=whoami").is_err());
        assert!(validate_git_ref("-n").is_err());
    }

    #[test]
    fn validate_git_ref_rejects_too_long() {
        let long_ref = "a".repeat(257);
        assert!(validate_git_ref(&long_ref).is_err());
    }

    #[test]
    fn validate_repo_name_accepts_valid() {
        assert!(validate_repo_name("owner/repo").is_ok());
        assert!(validate_repo_name("my-org/my-repo").is_ok());
        assert!(validate_repo_name("user123/project.js").is_ok());
        assert!(validate_repo_name("Org_Name/Repo_Name").is_ok());
    }

    #[test]
    fn validate_repo_name_rejects_shell_injection() {
        assert!(validate_repo_name("owner/repo; rm -rf /").is_err());
        assert!(validate_repo_name("$(whoami)/repo").is_err());
        assert!(validate_repo_name("owner/repo && echo pwned").is_err());
    }

    #[test]
    fn validate_repo_name_rejects_invalid_format() {
        assert!(validate_repo_name("").is_err());
        assert!(validate_repo_name("noslash").is_err());
        assert!(validate_repo_name("too/many/slashes").is_err());
        assert!(validate_repo_name("/repo").is_err());
        assert!(validate_repo_name("owner/").is_err());
    }

    #[test]
    fn validate_repo_name_rejects_leading_dot_or_dash() {
        assert!(validate_repo_name(".hidden/repo").is_err());
        assert!(validate_repo_name("owner/.repo").is_err());
        assert!(validate_repo_name("-flag/repo").is_err());
        assert!(validate_repo_name("owner/-repo").is_err());
        assert!(validate_repo_name("..traversal/repo").is_err());
    }

    #[test]
    fn validate_repo_name_rejects_too_long() {
        let long_name = format!("{}/{}", "a".repeat(128), "b".repeat(128));
        assert!(validate_repo_name(&long_name).is_err());
    }

    #[test]
    fn validate_file_path_accepts_valid() {
        assert!(validate_file_path("tests/test_foo.py").is_ok());
        assert!(validate_file_path("src/main.rs").is_ok());
        assert!(validate_file_path("a/b/c/d.txt").is_ok());
        assert!(validate_file_path("file.txt").is_ok());
    }

    #[test]
    fn validate_file_path_rejects_traversal() {
        assert!(validate_file_path("../etc/passwd").is_err());
        assert!(validate_file_path("a/../../etc/passwd").is_err());
        assert!(validate_file_path("..").is_err());
    }

    #[test]
    fn validate_file_path_rejects_shell_metacharacters() {
        assert!(validate_file_path("file'; rm -rf /; echo '").is_err());
        assert!(validate_file_path("file$(whoami)").is_err());
        assert!(validate_file_path("file`id`").is_err());
        assert!(validate_file_path("file|cat").is_err());
        assert!(validate_file_path("file;ls").is_err());
        assert!(validate_file_path("file&echo").is_err());
    }

    #[test]
    fn validate_file_path_rejects_empty_and_absolute() {
        assert!(validate_file_path("").is_err());
        assert!(validate_file_path("/etc/passwd").is_err());
    }

    #[test]
    fn validate_file_path_rejects_null_byte() {
        assert!(validate_file_path("file\0.txt").is_err());
    }
}
