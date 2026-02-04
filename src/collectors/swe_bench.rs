//! SWE-bench dataset collector.
//!
//! This module provides a collector for fetching benchmark tasks from the SWE-bench
//! dataset hosted on HuggingFace. SWE-bench contains real Python bug instances
//! from popular repositories with associated patches and test cases.

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::types::{CollectedTask, CollectorConfig, CollectorError, CollectorResult, TaskSource};

/// Default dataset name for SWE-bench Lite on HuggingFace.
const DEFAULT_DATASET: &str = "princeton-nlp/SWE-bench_Lite";

/// Default split to fetch from the dataset.
const DEFAULT_SPLIT: &str = "test";

/// Base URL for HuggingFace datasets server rows API.
const HUGGINGFACE_ROWS_API: &str = "https://datasets-server.huggingface.co/rows";

/// Configuration for the SWE-bench collector.
#[derive(Debug, Clone)]
pub struct SweBenchConfig {
    /// Delay between requests in milliseconds.
    pub rate_limit_delay_ms: u64,
    /// Maximum items per request.
    pub max_page_size: usize,
    /// Maximum retry attempts on failure.
    pub max_retries: u32,
}

impl Default for SweBenchConfig {
    fn default() -> Self {
        Self {
            rate_limit_delay_ms: 100,
            max_page_size: 100,
            max_retries: 3,
        }
    }
}

impl CollectorConfig for SweBenchConfig {
    fn rate_limit_delay_ms(&self) -> u64 {
        self.rate_limit_delay_ms
    }

    fn max_page_size(&self) -> usize {
        self.max_page_size
    }

    fn max_retries(&self) -> u32 {
        self.max_retries
    }
}

/// Collector for SWE-bench dataset from HuggingFace.
///
/// Fetches real Python bug instances with patches and test cases from the
/// SWE-bench dataset. Each instance includes:
/// - Repository information
/// - Problem statement
/// - Gold patch (solution)
/// - Test commands for verification
///
/// # Example
///
/// ```ignore
/// use dataforge::collectors::SweBenchCollector;
///
/// let collector = SweBenchCollector::new();
/// let tasks = collector.collect(10, 0).await?;
///
/// for task in tasks {
///     println!("Task: {} from {}", task.id, task.repo);
/// }
/// ```
pub struct SweBenchCollector {
    /// HTTP client for API requests.
    http_client: Client,
    /// HuggingFace dataset name.
    dataset_name: String,
    /// Dataset split to fetch from.
    split: String,
    /// Collector configuration.
    config: SweBenchConfig,
}

impl SweBenchCollector {
    /// Create a new SWE-bench collector with default settings.
    ///
    /// Uses the SWE-bench_Lite dataset and "test" split by default.
    pub fn new() -> Self {
        Self::with_dataset(DEFAULT_DATASET, DEFAULT_SPLIT)
    }

    /// Create a new SWE-bench collector for a specific dataset and split.
    ///
    /// # Arguments
    ///
    /// * `dataset_name` - HuggingFace dataset identifier (e.g., "princeton-nlp/SWE-bench")
    /// * `split` - Dataset split to use (e.g., "test", "train", "validation")
    pub fn with_dataset(dataset_name: impl Into<String>, split: impl Into<String>) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
            dataset_name: dataset_name.into(),
            split: split.into(),
            config: SweBenchConfig::default(),
        }
    }

    /// Configure the collector with custom settings.
    pub fn with_config(mut self, config: SweBenchConfig) -> Self {
        self.config = config;
        self
    }

    /// Get the current dataset name.
    pub fn dataset_name(&self) -> &str {
        &self.dataset_name
    }

    /// Get the current split.
    pub fn split(&self) -> &str {
        &self.split
    }

    /// Collect tasks from the SWE-bench dataset.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of tasks to fetch
    /// * `offset` - Starting offset for pagination
    ///
    /// # Returns
    ///
    /// A vector of collected tasks, or an error if the request fails.
    ///
    /// # Errors
    ///
    /// Returns `CollectorError` if:
    /// - The HTTP request fails
    /// - The response cannot be parsed
    /// - The API returns an error status
    pub async fn collect(
        &self,
        limit: usize,
        offset: usize,
    ) -> CollectorResult<Vec<CollectedTask>> {
        let effective_limit = limit.min(self.config.max_page_size);

        let url = format!(
            "{}?dataset={}&config=default&split={}&offset={}&length={}",
            HUGGINGFACE_ROWS_API, self.dataset_name, self.split, offset, effective_limit
        );

        let response = self
            .http_client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| CollectorError::HttpError(e.to_string()))?;

        let status = response.status();
        if status.as_u16() == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());
            return Err(CollectorError::RateLimited { retry_after });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(CollectorError::HttpError(format!(
                "API returned status {}: {}",
                status, error_text
            )));
        }

        let api_response: HuggingFaceRowsResponse = response
            .json()
            .await
            .map_err(|e| CollectorError::ParseError(format!("Failed to parse response: {}", e)))?;

        let tasks = api_response
            .rows
            .into_iter()
            .filter_map(|row| self.convert_row_to_task(row))
            .collect();

        Ok(tasks)
    }

    /// Convert a HuggingFace row to a CollectedTask.
    fn convert_row_to_task(&self, row: HuggingFaceRow) -> Option<CollectedTask> {
        let data = row.row;

        // Extract required fields
        let instance_id = data.instance_id.as_ref()?.clone();
        let repo = data.repo.as_ref()?.clone();
        let problem_statement = data.problem_statement.as_ref().cloned().unwrap_or_default();

        if problem_statement.is_empty() {
            return None;
        }

        // Build test commands from FAIL_TO_PASS and PASS_TO_PASS
        let mut test_commands = Vec::new();
        if let Some(fail_to_pass) = &data.fail_to_pass {
            if !fail_to_pass.is_empty() {
                test_commands.push(format!("pytest {}", fail_to_pass));
            }
        }
        if let Some(pass_to_pass) = &data.pass_to_pass {
            if !pass_to_pass.is_empty() {
                test_commands.push(format!("pytest {}", pass_to_pass));
            }
        }

        // Estimate difficulty based on patch size and test count
        let difficulty = self.estimate_difficulty(&data);

        // Build metadata
        let metadata = serde_json::json!({
            "version": data.version,
            "environment_setup_commit": data.environment_setup_commit,
            "fail_to_pass": data.fail_to_pass,
            "pass_to_pass": data.pass_to_pass,
            "created_at": data.created_at,
        });

        // Extract tags from the repository path
        let tags = self.extract_tags(&repo, &problem_statement);

        let task = CollectedTask {
            id: instance_id,
            source: TaskSource::SweBench,
            repo,
            base_commit: data.base_commit,
            problem_statement,
            solution_patch: data.patch,
            test_commands,
            difficulty_estimate: Some(difficulty),
            category: "bug_fix".to_string(),
            tags,
            metadata,
            collected_at: Utc::now(),
        };

        Some(task)
    }

    /// Estimate difficulty based on patch characteristics.
    ///
    /// Considers:
    /// - Patch size (number of lines)
    /// - Number of test cases
    /// - Presence of complex test requirements
    fn estimate_difficulty(&self, data: &SweBenchRowData) -> f64 {
        let mut score: f64 = 0.0;

        // Factor 1: Patch size
        if let Some(patch) = &data.patch {
            let line_count = patch.lines().count();
            score += match line_count {
                0..=10 => 0.1,
                11..=50 => 0.2,
                51..=100 => 0.4,
                101..=200 => 0.6,
                _ => 0.8,
            };
        }

        // Factor 2: Test complexity (FAIL_TO_PASS count)
        if let Some(fail_to_pass) = &data.fail_to_pass {
            let test_count = fail_to_pass.split("::").count();
            score += match test_count {
                0..=1 => 0.05,
                2..=3 => 0.1,
                4..=5 => 0.15,
                _ => 0.2,
            };
        }

        // Normalize to [0, 1]
        score.min(1.0)
    }

    /// Extract tags from repository name and problem statement.
    fn extract_tags(&self, repo: &str, problem_statement: &str) -> Vec<String> {
        let mut tags = vec!["python".to_string()];

        // Add repo-based tags
        let repo_lower = repo.to_lowercase();
        if repo_lower.contains("django") {
            tags.push("django".to_string());
            tags.push("web".to_string());
        }
        if repo_lower.contains("flask") {
            tags.push("flask".to_string());
            tags.push("web".to_string());
        }
        if repo_lower.contains("numpy") || repo_lower.contains("scipy") {
            tags.push("scientific".to_string());
        }
        if repo_lower.contains("pandas") {
            tags.push("data".to_string());
        }
        if repo_lower.contains("requests") {
            tags.push("http".to_string());
        }

        // Add keyword-based tags from problem statement
        let statement_lower = problem_statement.to_lowercase();
        if statement_lower.contains("security") || statement_lower.contains("vulnerability") {
            tags.push("security".to_string());
        }
        if statement_lower.contains("performance") || statement_lower.contains("slow") {
            tags.push("performance".to_string());
        }
        if statement_lower.contains("memory") || statement_lower.contains("leak") {
            tags.push("memory".to_string());
        }
        if statement_lower.contains("crash") || statement_lower.contains("exception") {
            tags.push("crash".to_string());
        }

        tags
    }
}

impl Default for SweBenchCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Response structure from HuggingFace rows API.
#[derive(Debug, Deserialize)]
struct HuggingFaceRowsResponse {
    /// List of rows from the dataset.
    rows: Vec<HuggingFaceRow>,
    /// Total number of rows in the dataset.
    #[allow(dead_code)]
    num_rows_total: Option<usize>,
    /// Number of rows in this response.
    #[allow(dead_code)]
    num_rows_per_page: Option<usize>,
}

/// A single row from the HuggingFace dataset.
#[derive(Debug, Deserialize)]
struct HuggingFaceRow {
    /// Row index in the dataset.
    #[allow(dead_code)]
    row_idx: usize,
    /// Row data containing the actual fields.
    row: SweBenchRowData,
}

/// Data fields for a SWE-bench instance.
#[derive(Debug, Deserialize, Serialize)]
struct SweBenchRowData {
    /// Unique instance identifier.
    instance_id: Option<String>,
    /// Repository in format "owner/repo".
    repo: Option<String>,
    /// Base commit SHA.
    base_commit: Option<String>,
    /// Gold patch (solution).
    patch: Option<String>,
    /// Problem statement/description.
    problem_statement: Option<String>,
    /// Tests that should pass after applying the patch.
    #[serde(rename = "FAIL_TO_PASS")]
    fail_to_pass: Option<String>,
    /// Tests that should remain passing.
    #[serde(rename = "PASS_TO_PASS")]
    pass_to_pass: Option<String>,
    /// Version identifier.
    version: Option<String>,
    /// Environment setup commit.
    environment_setup_commit: Option<String>,
    /// Creation timestamp.
    created_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_creation() {
        let collector = SweBenchCollector::new();
        assert_eq!(collector.dataset_name(), DEFAULT_DATASET);
        assert_eq!(collector.split(), DEFAULT_SPLIT);
    }

    #[test]
    fn test_collector_with_dataset() {
        let collector = SweBenchCollector::with_dataset("custom/dataset", "train");
        assert_eq!(collector.dataset_name(), "custom/dataset");
        assert_eq!(collector.split(), "train");
    }

    #[test]
    fn test_collector_config() {
        let config = SweBenchConfig {
            rate_limit_delay_ms: 200,
            max_page_size: 50,
            max_retries: 5,
        };
        let collector = SweBenchCollector::new().with_config(config);
        assert_eq!(collector.config.rate_limit_delay_ms, 200);
        assert_eq!(collector.config.max_page_size, 50);
    }

    #[test]
    fn test_estimate_difficulty_small_patch() {
        let collector = SweBenchCollector::new();
        let data = SweBenchRowData {
            instance_id: Some("test".to_string()),
            repo: Some("test/repo".to_string()),
            base_commit: None,
            patch: Some("+ line1\n+ line2\n".to_string()),
            problem_statement: Some("Fix bug".to_string()),
            fail_to_pass: Some("test_one".to_string()),
            pass_to_pass: None,
            version: None,
            environment_setup_commit: None,
            created_at: None,
        };
        let difficulty = collector.estimate_difficulty(&data);
        assert!(difficulty < 0.5, "Small patch should have low difficulty");
    }

    #[test]
    fn test_estimate_difficulty_large_patch() {
        let collector = SweBenchCollector::new();
        let large_patch = (0..150)
            .map(|i| format!("+ line{}\n", i))
            .collect::<String>();
        let data = SweBenchRowData {
            instance_id: Some("test".to_string()),
            repo: Some("test/repo".to_string()),
            base_commit: None,
            patch: Some(large_patch),
            problem_statement: Some("Fix bug".to_string()),
            fail_to_pass: Some("test_one::test_two::test_three::test_four".to_string()),
            pass_to_pass: None,
            version: None,
            environment_setup_commit: None,
            created_at: None,
        };
        let difficulty = collector.estimate_difficulty(&data);
        assert!(
            difficulty > 0.5,
            "Large patch with many tests should have high difficulty"
        );
    }

    #[test]
    fn test_extract_tags_django() {
        let collector = SweBenchCollector::new();
        let tags = collector.extract_tags("django/django", "Fix security vulnerability in forms");
        assert!(tags.contains(&"python".to_string()));
        assert!(tags.contains(&"django".to_string()));
        assert!(tags.contains(&"web".to_string()));
        assert!(tags.contains(&"security".to_string()));
    }

    #[test]
    fn test_extract_tags_performance() {
        let collector = SweBenchCollector::new();
        let tags =
            collector.extract_tags("pandas-dev/pandas", "Slow performance in groupby operation");
        assert!(tags.contains(&"python".to_string()));
        assert!(tags.contains(&"data".to_string()));
        assert!(tags.contains(&"performance".to_string()));
    }

    #[test]
    fn test_convert_row_to_task() {
        let collector = SweBenchCollector::new();
        let row = HuggingFaceRow {
            row_idx: 0,
            row: SweBenchRowData {
                instance_id: Some("django__django-12345".to_string()),
                repo: Some("django/django".to_string()),
                base_commit: Some("abc123".to_string()),
                patch: Some("diff --git a/file.py\n+ fixed\n".to_string()),
                problem_statement: Some("Fix the bug in views.py".to_string()),
                fail_to_pass: Some("tests/test_views.py::test_fix".to_string()),
                pass_to_pass: Some("tests/test_views.py::test_other".to_string()),
                version: Some("3.2".to_string()),
                environment_setup_commit: None,
                created_at: None,
            },
        };

        let task = collector
            .convert_row_to_task(row)
            .expect("Should convert successfully");
        assert_eq!(task.id, "django__django-12345");
        assert_eq!(task.source, TaskSource::SweBench);
        assert_eq!(task.repo, "django/django");
        assert_eq!(task.base_commit, Some("abc123".to_string()));
        assert!(task.solution_patch.is_some());
        assert_eq!(task.test_commands.len(), 2);
        assert!(task.test_commands[0].contains("pytest"));
        assert_eq!(task.category, "bug_fix");
        assert!(task.tags.contains(&"django".to_string()));
    }

    #[test]
    fn test_convert_row_missing_required_fields() {
        let collector = SweBenchCollector::new();

        // Missing instance_id
        let row = HuggingFaceRow {
            row_idx: 0,
            row: SweBenchRowData {
                instance_id: None,
                repo: Some("test/repo".to_string()),
                base_commit: None,
                patch: None,
                problem_statement: Some("Problem".to_string()),
                fail_to_pass: None,
                pass_to_pass: None,
                version: None,
                environment_setup_commit: None,
                created_at: None,
            },
        };
        assert!(collector.convert_row_to_task(row).is_none());

        // Missing problem statement
        let row = HuggingFaceRow {
            row_idx: 0,
            row: SweBenchRowData {
                instance_id: Some("test-id".to_string()),
                repo: Some("test/repo".to_string()),
                base_commit: None,
                patch: None,
                problem_statement: None,
                fail_to_pass: None,
                pass_to_pass: None,
                version: None,
                environment_setup_commit: None,
                created_at: None,
            },
        };
        assert!(collector.convert_row_to_task(row).is_none());
    }

    #[test]
    fn test_default_config() {
        let config = SweBenchConfig::default();
        assert_eq!(config.rate_limit_delay_ms, 100);
        assert_eq!(config.max_page_size, 100);
        assert_eq!(config.max_retries, 3);
    }

    #[tokio::test]
    async fn test_collect_with_invalid_dataset() {
        let collector = SweBenchCollector::with_dataset(
            "nonexistent/dataset-that-does-not-exist-12345",
            "test",
        );
        let result = collector.collect(1, 0).await;
        // Should return an error (either HTTP error or parse error)
        assert!(result.is_err());
    }
}
