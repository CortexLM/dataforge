//! Common types used across external data source collectors.
//!
//! This module defines shared types for collecting benchmark data from external sources
//! including error handling, task representation, and pagination support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Errors that can occur during data collection operations.
#[derive(Debug, Error)]
pub enum CollectorError {
    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// Failed to parse response data.
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// API rate limit exceeded.
    #[error("Rate limited: retry after {retry_after:?} seconds")]
    RateLimited {
        /// Optional retry-after duration in seconds.
        retry_after: Option<u64>,
    },

    /// Invalid or unexpected response from API.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// IO operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type alias for collector operations.
pub type CollectorResult<T> = Result<T, CollectorError>;

/// Source of a collected task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    /// Task from SWE-bench dataset (HuggingFace).
    SweBench,
    /// Task from GitHub Advisory Database.
    GitHubAdvisory,
    /// Task from GitHub Issues with linked PRs.
    GitHubIssues,
}

impl fmt::Display for TaskSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskSource::SweBench => write!(f, "SWE-bench"),
            TaskSource::GitHubAdvisory => write!(f, "GitHub Advisory"),
            TaskSource::GitHubIssues => write!(f, "GitHub Issues"),
        }
    }
}

/// A task collected from an external data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedTask {
    /// Unique identifier for the task.
    pub id: String,

    /// Source of the task data.
    pub source: TaskSource,

    /// Repository in format "owner/repo".
    pub repo: String,

    /// Base commit SHA for the task (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,

    /// Problem statement or description.
    pub problem_statement: String,

    /// Solution patch or diff (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution_patch: Option<String>,

    /// Commands to run tests for verification.
    #[serde(default)]
    pub test_commands: Vec<String>,

    /// Estimated difficulty level (0.0 - 1.0, where 1.0 is hardest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty_estimate: Option<f64>,

    /// Category of the task (e.g., "bug_fix", "security", "feature").
    pub category: String,

    /// Tags for additional classification.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Additional metadata specific to the source.
    #[serde(default)]
    pub metadata: serde_json::Value,

    /// Timestamp when the task was collected.
    pub collected_at: DateTime<Utc>,
}

impl CollectedTask {
    /// Create a new collected task with required fields.
    pub fn new(
        id: impl Into<String>,
        source: TaskSource,
        repo: impl Into<String>,
        problem_statement: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            repo: repo.into(),
            base_commit: None,
            problem_statement: problem_statement.into(),
            solution_patch: None,
            test_commands: Vec::new(),
            difficulty_estimate: None,
            category: category.into(),
            tags: Vec::new(),
            metadata: serde_json::Value::Null,
            collected_at: Utc::now(),
        }
    }

    /// Set the base commit SHA.
    pub fn with_base_commit(mut self, commit: impl Into<String>) -> Self {
        self.base_commit = Some(commit.into());
        self
    }

    /// Set the solution patch.
    pub fn with_solution_patch(mut self, patch: impl Into<String>) -> Self {
        self.solution_patch = Some(patch.into());
        self
    }

    /// Add test commands.
    pub fn with_test_commands(mut self, commands: Vec<String>) -> Self {
        self.test_commands = commands;
        self
    }

    /// Set the difficulty estimate.
    pub fn with_difficulty_estimate(mut self, difficulty: f64) -> Self {
        self.difficulty_estimate = Some(difficulty.clamp(0.0, 1.0));
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set additional metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Trait for collector configuration providing rate limiting and pagination settings.
pub trait CollectorConfig {
    /// Get the rate limit delay between requests in milliseconds.
    fn rate_limit_delay_ms(&self) -> u64;

    /// Get the maximum number of items per page/request.
    fn max_page_size(&self) -> usize;

    /// Get the maximum number of retries on failure.
    fn max_retries(&self) -> u32;
}

/// Response wrapper for paginated API results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Items in this page of results.
    pub items: Vec<T>,

    /// Cursor for fetching the next page (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,

    /// Whether there are more results available.
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response.
    pub fn new(items: Vec<T>, next_cursor: Option<String>, has_more: bool) -> Self {
        Self {
            items,
            next_cursor,
            has_more,
        }
    }

    /// Create an empty response with no more results.
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
        }
    }

    /// Create a response indicating this is the last page.
    pub fn last_page(items: Vec<T>) -> Self {
        Self {
            items,
            next_cursor: None,
            has_more: false,
        }
    }
}

impl<T> Default for PaginatedResponse<T> {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_source_display() {
        assert_eq!(TaskSource::SweBench.to_string(), "SWE-bench");
        assert_eq!(TaskSource::GitHubAdvisory.to_string(), "GitHub Advisory");
        assert_eq!(TaskSource::GitHubIssues.to_string(), "GitHub Issues");
    }

    #[test]
    fn test_task_source_serialization() {
        let source = TaskSource::SweBench;
        let json = serde_json::to_string(&source).expect("serialization should succeed");
        assert_eq!(json, "\"swe_bench\"");

        let deserialized: TaskSource =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized, TaskSource::SweBench);
    }

    #[test]
    fn test_collected_task_builder() {
        let task = CollectedTask::new(
            "test-001",
            TaskSource::SweBench,
            "owner/repo",
            "Fix the bug in function X",
            "bug_fix",
        )
        .with_base_commit("abc123")
        .with_solution_patch("diff --git a/file.py b/file.py\n...")
        .with_test_commands(vec!["pytest tests/".to_string()])
        .with_difficulty_estimate(0.7)
        .with_tags(vec!["python".to_string(), "debugging".to_string()])
        .with_metadata(serde_json::json!({"extra": "data"}));

        assert_eq!(task.id, "test-001");
        assert_eq!(task.source, TaskSource::SweBench);
        assert_eq!(task.repo, "owner/repo");
        assert_eq!(task.base_commit, Some("abc123".to_string()));
        assert_eq!(
            task.solution_patch,
            Some("diff --git a/file.py b/file.py\n...".to_string())
        );
        assert_eq!(task.test_commands, vec!["pytest tests/"]);
        assert_eq!(task.difficulty_estimate, Some(0.7));
        assert_eq!(task.category, "bug_fix");
        assert_eq!(task.tags, vec!["python", "debugging"]);
    }

    #[test]
    fn test_difficulty_estimate_clamping() {
        let task = CollectedTask::new("test", TaskSource::SweBench, "repo", "desc", "cat")
            .with_difficulty_estimate(1.5);
        assert_eq!(task.difficulty_estimate, Some(1.0));

        let task = CollectedTask::new("test", TaskSource::SweBench, "repo", "desc", "cat")
            .with_difficulty_estimate(-0.5);
        assert_eq!(task.difficulty_estimate, Some(0.0));
    }

    #[test]
    fn test_paginated_response() {
        let response = PaginatedResponse::new(
            vec!["item1".to_string(), "item2".to_string()],
            Some("cursor123".to_string()),
            true,
        );
        assert_eq!(response.items.len(), 2);
        assert_eq!(response.next_cursor, Some("cursor123".to_string()));
        assert!(response.has_more);

        let last = PaginatedResponse::last_page(vec!["final".to_string()]);
        assert!(!last.has_more);
        assert!(last.next_cursor.is_none());

        let empty: PaginatedResponse<String> = PaginatedResponse::empty();
        assert!(empty.items.is_empty());
        assert!(!empty.has_more);
    }

    #[test]
    fn test_collector_error_display() {
        let err = CollectorError::HttpError("connection timeout".to_string());
        assert_eq!(err.to_string(), "HTTP request failed: connection timeout");

        let err = CollectorError::RateLimited {
            retry_after: Some(60),
        };
        assert!(err.to_string().contains("Rate limited"));

        let err = CollectorError::ParseError("invalid JSON".to_string());
        assert_eq!(err.to_string(), "Failed to parse response: invalid JSON");
    }
}
