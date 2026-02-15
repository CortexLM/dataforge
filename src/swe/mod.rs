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

pub mod enricher;
pub mod extractor;
pub mod filters;
pub mod gharchive;
pub mod harness;
pub mod orchestrator;
pub mod pipeline;
pub mod prompt_rewriter;
pub mod quality;
pub mod test_generator;

pub use enricher::EnrichedPullRequest;
pub use extractor::{ExtractedPatch, PatchExtractor, PatchExtractorConfig};
pub use filters::{FilterConfig, FilterResult, SweepFilter};
pub use gharchive::{GhArchiveClient, GhArchiveEvent, GhArchiveEventId};
pub use harness::{run_harness, HarnessConfig, HarnessResult, HarnessSummary};
pub use orchestrator::{SweOrchestrator, SweOrchestratorConfig, SweRunResult};
pub use pipeline::{SwePipeline, SwePipelineEvent, SwePipelineRunResult};
pub use prompt_rewriter::PromptRewriter;
pub use quality::{QualityAssessment, QualityConfig, QualityScorer};
pub use test_generator::{TestFile, TestGenerator};

/// Default output directory for generated SWE workspaces.
pub const DEFAULT_SWE_OUTPUT_DIR: &str = "./generated-swe";

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
    /// Must-not-pass test commands (must FAIL even after PR, anti-cheat).
    #[serde(default)]
    pub must_not_pass: Vec<String>,
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
            must_not_pass: Vec::new(),
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
