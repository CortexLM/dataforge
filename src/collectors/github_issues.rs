//! GitHub Issues collector with linked PRs.
//!
//! This module provides a collector for fetching closed issues from GitHub
//! repositories that have associated pull requests. This allows collecting
//! real-world bug fixes with both problem descriptions and solutions.

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::types::{CollectedTask, CollectorConfig, CollectorError, CollectorResult, TaskSource};

/// GitHub REST API base URL.
const GITHUB_API_BASE: &str = "https://api.github.com";

/// Configuration for a repository to collect issues from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    /// Repository owner (user or organization).
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Labels to filter issues by (e.g., ["bug", "good-first-issue"]).
    #[serde(default)]
    pub labels: Vec<String>,
    /// Minimum number of comments on the issue.
    #[serde(default)]
    pub min_comments: u32,
    /// Whether to require a linked PR for the issue.
    #[serde(default = "default_true")]
    pub require_linked_pr: bool,
}

fn default_true() -> bool {
    true
}

impl RepoConfig {
    /// Create a new repository configuration.
    pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            labels: Vec::new(),
            min_comments: 0,
            require_linked_pr: true,
        }
    }

    /// Add a label filter.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Add multiple label filters.
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Set minimum comment count.
    pub fn with_min_comments(mut self, min: u32) -> Self {
        self.min_comments = min;
        self
    }

    /// Set whether to require linked PRs.
    pub fn with_require_linked_pr(mut self, require: bool) -> Self {
        self.require_linked_pr = require;
        self
    }

    /// Get the full repository path.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// Default repositories for DevOps/infrastructure projects.
fn default_repos() -> Vec<RepoConfig> {
    vec![
        RepoConfig::new("kubernetes", "kubernetes")
            .with_labels(vec!["kind/bug".to_string(), "good-first-issue".to_string()]),
        RepoConfig::new("docker", "compose").with_labels(vec!["kind/bug".to_string()]),
        RepoConfig::new("hashicorp", "terraform").with_labels(vec!["bug".to_string()]),
        RepoConfig::new("ansible", "ansible")
            .with_labels(vec!["bug".to_string(), "good_first_issue".to_string()]),
        RepoConfig::new("nginx", "nginx").with_labels(vec!["bug".to_string()]),
    ]
}

/// Configuration for the GitHub Issues collector.
#[derive(Debug, Clone)]
pub struct GitHubIssuesConfig {
    /// Delay between requests in milliseconds.
    pub rate_limit_delay_ms: u64,
    /// Maximum items per request.
    pub max_page_size: usize,
    /// Maximum retry attempts on failure.
    pub max_retries: u32,
}

impl Default for GitHubIssuesConfig {
    fn default() -> Self {
        Self {
            rate_limit_delay_ms: 100,
            max_page_size: 100,
            max_retries: 3,
        }
    }
}

impl CollectorConfig for GitHubIssuesConfig {
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

/// Collector for GitHub Issues with linked pull requests.
///
/// Fetches closed issues from configured repositories that have been resolved
/// by pull requests. This provides real-world bug reports with associated fixes.
///
/// # Example
///
/// ```ignore
/// use dataforge::collectors::{GitHubIssuesCollector, RepoConfig};
///
/// let collector = GitHubIssuesCollector::new(Some("ghp_xxxxx".to_string()))
///     .add_repo(RepoConfig::new("owner", "repo").with_label("bug"));
///
/// let tasks = collector.collect(10, 1).await?;
/// for task in tasks {
///     println!("Issue: {} - {}", task.id, task.problem_statement);
/// }
/// ```
pub struct GitHubIssuesCollector {
    /// HTTP client for API requests.
    http_client: Client,
    /// Optional GitHub API token for higher rate limits.
    api_token: Option<String>,
    /// Repositories to collect from.
    repos: Vec<RepoConfig>,
    /// Collector configuration.
    config: GitHubIssuesConfig,
}

impl GitHubIssuesCollector {
    /// Create a new GitHub Issues collector with default repositories.
    ///
    /// # Arguments
    ///
    /// * `api_token` - Optional GitHub personal access token for API authentication.
    ///   Without a token, requests are heavily rate-limited.
    pub fn new(api_token: Option<String>) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
            api_token,
            repos: default_repos(),
            config: GitHubIssuesConfig::default(),
        }
    }

    /// Create a collector with no default repositories.
    pub fn empty(api_token: Option<String>) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
            api_token,
            repos: Vec::new(),
            config: GitHubIssuesConfig::default(),
        }
    }

    /// Add a repository to collect from.
    pub fn add_repo(mut self, config: RepoConfig) -> Self {
        self.repos.push(config);
        self
    }

    /// Replace all repositories with a new list.
    pub fn with_repos(mut self, repos: Vec<RepoConfig>) -> Self {
        self.repos = repos;
        self
    }

    /// Configure the collector with custom settings.
    pub fn with_config(mut self, config: GitHubIssuesConfig) -> Self {
        self.config = config;
        self
    }

    /// Check if an API token is configured.
    pub fn has_token(&self) -> bool {
        self.api_token.is_some()
    }

    /// Get the configured repositories.
    pub fn repos(&self) -> &[RepoConfig] {
        &self.repos
    }

    /// Collect issues from all configured repositories.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of issues to fetch in total
    /// * `page` - Page number for pagination (1-indexed)
    ///
    /// # Returns
    ///
    /// A vector of collected tasks from all repositories.
    ///
    /// # Errors
    ///
    /// Returns `CollectorError` if:
    /// - The HTTP request fails
    /// - The response cannot be parsed
    /// - All repositories fail to fetch
    pub async fn collect(&self, limit: usize, page: u32) -> CollectorResult<Vec<CollectedTask>> {
        if self.repos.is_empty() {
            return Ok(Vec::new());
        }

        let effective_limit = limit.min(self.config.max_page_size);
        let per_repo_limit = (effective_limit / self.repos.len()).max(1);

        let mut all_tasks = Vec::new();
        let mut last_error: Option<CollectorError> = None;

        for repo_config in &self.repos {
            match self
                .collect_from_repo(repo_config, per_repo_limit, page)
                .await
            {
                Ok(tasks) => all_tasks.extend(tasks),
                Err(e) => {
                    // Continue with other repos, but track the error
                    last_error = Some(e);
                }
            }

            if all_tasks.len() >= limit {
                break;
            }
        }

        // If we got no tasks and had errors, return the last error
        if all_tasks.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
        }

        // Truncate to limit
        all_tasks.truncate(limit);
        Ok(all_tasks)
    }

    /// Collect issues from a single repository.
    async fn collect_from_repo(
        &self,
        repo_config: &RepoConfig,
        limit: usize,
        page: u32,
    ) -> CollectorResult<Vec<CollectedTask>> {
        let issues = self.fetch_issues(repo_config, limit, page).await?;

        let mut tasks = Vec::new();

        for issue in issues {
            // Skip if doesn't meet comment requirement
            if issue.comments < repo_config.min_comments {
                continue;
            }

            // Check for linked PR if required
            let linked_pr = if repo_config.require_linked_pr {
                match self.find_linked_pr(repo_config, issue.number).await {
                    Ok(Some(pr)) => Some(pr),
                    Ok(None) => continue, // Skip issues without linked PRs
                    Err(_) => continue,   // Skip on error
                }
            } else {
                None
            };

            if let Some(task) = self
                .convert_issue_to_task(repo_config, issue, linked_pr)
                .await
            {
                tasks.push(task);
            }
        }

        Ok(tasks)
    }

    /// Fetch issues from the GitHub API.
    async fn fetch_issues(
        &self,
        repo_config: &RepoConfig,
        limit: usize,
        page: u32,
    ) -> CollectorResult<Vec<GitHubIssue>> {
        let mut url = format!(
            "{}/repos/{}/{}/issues?state=closed&per_page={}&page={}",
            GITHUB_API_BASE, repo_config.owner, repo_config.repo, limit, page
        );

        // Add label filters
        if !repo_config.labels.is_empty() {
            let labels = repo_config.labels.join(",");
            url.push_str(&format!("&labels={}", urlencoding::encode(&labels)));
        }

        let mut request = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "dataforge/1.0")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(ref token) = self.api_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| CollectorError::HttpError(e.to_string()))?;

        let status = response.status();
        if status.as_u16() == 429 || status.as_u16() == 403 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .or_else(|| response.headers().get("x-ratelimit-reset"))
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

        let issues: Vec<GitHubIssue> = response
            .json()
            .await
            .map_err(|e| CollectorError::ParseError(format!("Failed to parse issues: {}", e)))?;

        // Filter out pull requests (they also appear in the issues endpoint)
        let issues = issues
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .collect();

        Ok(issues)
    }

    /// Find a linked pull request for an issue using timeline events.
    async fn find_linked_pr(
        &self,
        repo_config: &RepoConfig,
        issue_number: u64,
    ) -> CollectorResult<Option<LinkedPR>> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}/timeline",
            GITHUB_API_BASE, repo_config.owner, repo_config.repo, issue_number
        );

        let mut request = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "dataforge/1.0")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(ref token) = self.api_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| CollectorError::HttpError(e.to_string()))?;

        let status = response.status();
        if status.as_u16() == 429 || status.as_u16() == 403 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());
            return Err(CollectorError::RateLimited { retry_after });
        }

        if !status.is_success() {
            // Timeline API might not be available for all repos
            return Ok(None);
        }

        let events: Vec<TimelineEvent> = response.json().await.unwrap_or_default();

        // Find cross-reference events that link to merged PRs
        for event in events {
            if event.event == "cross-referenced" {
                if let Some(source) = event.source {
                    if let Some(issue) = source.issue {
                        if issue.pull_request.is_some() {
                            // This is a PR that references this issue
                            // Check if it's merged
                            if issue.state == "closed" {
                                // Fetch the PR diff
                                let diff = self
                                    .fetch_pr_diff(repo_config, issue.number)
                                    .await
                                    .ok()
                                    .flatten();

                                return Ok(Some(LinkedPR {
                                    number: issue.number,
                                    title: issue.title,
                                    body: issue.body,
                                    diff,
                                    labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
                                    merge_commit_sha: None,
                                }));
                            }
                        }
                    }
                }
            }

            // Also check for "connected" events
            if event.event == "connected" || event.event == "closed" {
                if let Some(commit_id) = event.commit_id {
                    return Ok(Some(LinkedPR {
                        number: 0,
                        title: String::new(),
                        body: None,
                        diff: None,
                        labels: Vec::new(),
                        merge_commit_sha: Some(commit_id),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Fetch the diff for a pull request.
    async fn fetch_pr_diff(
        &self,
        repo_config: &RepoConfig,
        pr_number: u64,
    ) -> CollectorResult<Option<String>> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            GITHUB_API_BASE, repo_config.owner, repo_config.repo, pr_number
        );

        let mut request = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github.v3.diff")
            .header("User-Agent", "dataforge/1.0")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(ref token) = self.api_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| CollectorError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let diff = response.text().await.ok();
        Ok(diff)
    }

    /// Convert a GitHub issue to a CollectedTask.
    async fn convert_issue_to_task(
        &self,
        repo_config: &RepoConfig,
        issue: GitHubIssue,
        linked_pr: Option<LinkedPR>,
    ) -> Option<CollectedTask> {
        let problem_statement = format!(
            "# {}\n\n{}",
            issue.title,
            issue.body.as_ref().cloned().unwrap_or_default()
        )
        .trim()
        .to_string();

        if problem_statement.len() < 20 {
            return None; // Skip issues with minimal content
        }

        // Estimate difficulty (before moving values from issue)
        let difficulty = self.estimate_difficulty(&issue, linked_pr.as_ref());

        // Extract tags from issue labels
        let mut tags: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

        // Add language/framework tags based on repo
        let repo_name = repo_config.repo.to_lowercase();
        if repo_name.contains("kubernetes") || repo_name.contains("k8s") {
            tags.push("kubernetes".to_string());
            tags.push("go".to_string());
        }
        if repo_name.contains("docker") {
            tags.push("docker".to_string());
            tags.push("containers".to_string());
        }
        if repo_name.contains("terraform") {
            tags.push("terraform".to_string());
            tags.push("iac".to_string());
        }
        if repo_name.contains("ansible") {
            tags.push("ansible".to_string());
            tags.push("python".to_string());
        }
        if repo_name.contains("nginx") {
            tags.push("nginx".to_string());
            tags.push("webserver".to_string());
        }

        // Determine category from labels
        let category = if tags.iter().any(|t| t.contains("bug")) {
            "bug_fix"
        } else if tags
            .iter()
            .any(|t| t.contains("feature") || t.contains("enhancement"))
        {
            "feature"
        } else {
            "general"
        };

        // Build metadata
        let metadata = serde_json::json!({
            "issue_url": issue.html_url,
            "issue_number": issue.number,
            "comments": issue.comments,
            "created_at": issue.created_at,
            "closed_at": issue.closed_at,
            "user": issue.user.login,
            "linked_pr": linked_pr.as_ref().map(|pr| serde_json::json!({
                "number": pr.number,
                "title": pr.title,
                "has_diff": pr.diff.is_some(),
                "merge_commit": pr.merge_commit_sha,
            })),
        });

        let task = CollectedTask {
            id: format!("{}#{}", repo_config.full_name(), issue.number),
            source: TaskSource::GitHubIssues,
            repo: repo_config.full_name(),
            base_commit: linked_pr
                .as_ref()
                .and_then(|pr| pr.merge_commit_sha.clone()),
            problem_statement,
            solution_patch: linked_pr.and_then(|pr| pr.diff),
            test_commands: Vec::new(),
            difficulty_estimate: Some(difficulty),
            category: category.to_string(),
            tags,
            metadata,
            collected_at: Utc::now(),
        };

        Some(task)
    }

    /// Estimate difficulty based on issue characteristics.
    fn estimate_difficulty(&self, issue: &GitHubIssue, linked_pr: Option<&LinkedPR>) -> f64 {
        let mut score: f64 = 0.0;

        // Factor 1: Issue body length (more details often = more complex)
        if let Some(body) = &issue.body {
            let word_count = body.split_whitespace().count();
            score += match word_count {
                0..=50 => 0.1,
                51..=200 => 0.2,
                201..=500 => 0.3,
                _ => 0.4,
            };
        }

        // Factor 2: Comment count (more discussion = more complex)
        score += match issue.comments {
            0..=2 => 0.1,
            3..=5 => 0.15,
            6..=10 => 0.2,
            _ => 0.25,
        };

        // Factor 3: Linked PR diff size
        if let Some(pr) = linked_pr {
            if let Some(diff) = &pr.diff {
                let line_count = diff.lines().count();
                score += match line_count {
                    0..=50 => 0.1,
                    51..=200 => 0.2,
                    201..=500 => 0.3,
                    _ => 0.35,
                };
            }
        }

        score.min(1.0)
    }
}

impl Default for GitHubIssuesCollector {
    fn default() -> Self {
        Self::new(None)
    }
}

/// GitHub issue from the API.
#[derive(Debug, Deserialize)]
struct GitHubIssue {
    number: u64,
    title: String,
    body: Option<String>,
    #[allow(dead_code)]
    state: String,
    html_url: String,
    comments: u32,
    labels: Vec<GitHubLabel>,
    user: GitHubUser,
    pull_request: Option<serde_json::Value>,
    created_at: String,
    closed_at: Option<String>,
}

/// GitHub label.
#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

/// GitHub user.
#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

/// Timeline event from the GitHub API.
#[derive(Debug, Deserialize)]
struct TimelineEvent {
    event: String,
    commit_id: Option<String>,
    source: Option<EventSource>,
}

/// Event source containing issue reference.
#[derive(Debug, Deserialize)]
struct EventSource {
    issue: Option<SourceIssue>,
}

/// Issue referenced in an event source.
#[derive(Debug, Deserialize)]
struct SourceIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    labels: Vec<GitHubLabel>,
    pull_request: Option<serde_json::Value>,
}

/// Linked pull request information.
#[derive(Debug)]
#[allow(dead_code)]
struct LinkedPR {
    number: u64,
    title: String,
    body: Option<String>,
    diff: Option<String>,
    labels: Vec<String>,
    merge_commit_sha: Option<String>,
}

/// URL encoding helper module.
mod urlencoding {
    /// Encode a string for use in a URL query parameter.
    pub fn encode(input: &str) -> String {
        let mut encoded = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char);
                }
                _ => {
                    encoded.push('%');
                    encoded.push_str(&format!("{:02X}", byte));
                }
            }
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_config_creation() {
        let config = RepoConfig::new("owner", "repo")
            .with_label("bug")
            .with_min_comments(5)
            .with_require_linked_pr(true);

        assert_eq!(config.owner, "owner");
        assert_eq!(config.repo, "repo");
        assert_eq!(config.labels, vec!["bug"]);
        assert_eq!(config.min_comments, 5);
        assert!(config.require_linked_pr);
        assert_eq!(config.full_name(), "owner/repo");
    }

    #[test]
    fn test_repo_config_with_labels() {
        let config = RepoConfig::new("k8s", "kubernetes")
            .with_labels(vec!["bug".to_string(), "help-wanted".to_string()]);

        assert_eq!(config.labels.len(), 2);
    }

    #[test]
    fn test_default_repos() {
        let repos = default_repos();
        assert!(!repos.is_empty());

        // Check that kubernetes is in the defaults
        assert!(repos
            .iter()
            .any(|r| r.owner == "kubernetes" && r.repo == "kubernetes"));
    }

    #[test]
    fn test_collector_creation() {
        let collector = GitHubIssuesCollector::new(Some("token".to_string()));
        assert!(collector.has_token());
        assert!(!collector.repos().is_empty());

        let collector_no_token = GitHubIssuesCollector::new(None);
        assert!(!collector_no_token.has_token());
    }

    #[test]
    fn test_collector_empty() {
        let collector = GitHubIssuesCollector::empty(None);
        assert!(collector.repos().is_empty());
    }

    #[test]
    fn test_collector_add_repo() {
        let collector = GitHubIssuesCollector::empty(None)
            .add_repo(RepoConfig::new("test", "repo1"))
            .add_repo(RepoConfig::new("test", "repo2"));

        assert_eq!(collector.repos().len(), 2);
    }

    #[test]
    fn test_collector_with_repos() {
        let repos = vec![RepoConfig::new("a", "b"), RepoConfig::new("c", "d")];
        let collector = GitHubIssuesCollector::new(None).with_repos(repos);

        assert_eq!(collector.repos().len(), 2);
        // Default repos should be replaced
        assert!(collector.repos().iter().any(|r| r.owner == "a"));
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding::encode("hello"), "hello");
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("bug,feature"), "bug%2Cfeature");
        assert_eq!(urlencoding::encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn test_estimate_difficulty_minimal() {
        let collector = GitHubIssuesCollector::new(None);
        let issue = GitHubIssue {
            number: 1,
            title: "Bug".to_string(),
            body: Some("Short".to_string()),
            state: "closed".to_string(),
            html_url: "https://github.com/o/r/issues/1".to_string(),
            comments: 0,
            labels: vec![],
            user: GitHubUser {
                login: "user".to_string(),
            },
            pull_request: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            closed_at: Some("2024-01-02T00:00:00Z".to_string()),
        };

        let difficulty = collector.estimate_difficulty(&issue, None);
        assert!(difficulty < 0.5, "Minimal issue should have low difficulty");
    }

    #[test]
    fn test_estimate_difficulty_complex() {
        let collector = GitHubIssuesCollector::new(None);
        let long_body = (0..300).map(|_| "word ").collect::<String>();
        let issue = GitHubIssue {
            number: 1,
            title: "Complex Bug".to_string(),
            body: Some(long_body),
            state: "closed".to_string(),
            html_url: "https://github.com/o/r/issues/1".to_string(),
            comments: 15,
            labels: vec![],
            user: GitHubUser {
                login: "user".to_string(),
            },
            pull_request: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            closed_at: Some("2024-01-02T00:00:00Z".to_string()),
        };

        let large_diff = (0..300)
            .map(|i| format!("+ line {}\n", i))
            .collect::<String>();
        let linked_pr = LinkedPR {
            number: 100,
            title: "Fix".to_string(),
            body: None,
            diff: Some(large_diff),
            labels: vec![],
            merge_commit_sha: None,
        };

        let difficulty = collector.estimate_difficulty(&issue, Some(&linked_pr));
        assert!(
            difficulty > 0.5,
            "Complex issue should have high difficulty"
        );
    }

    #[test]
    fn test_default_config() {
        let config = GitHubIssuesConfig::default();
        assert_eq!(config.rate_limit_delay_ms, 100);
        assert_eq!(config.max_page_size, 100);
        assert_eq!(config.max_retries, 3);
    }

    #[tokio::test]
    async fn test_collect_empty_repos() {
        let collector = GitHubIssuesCollector::empty(None);
        let result = collector.collect(10, 1).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_collect_invalid_repo() {
        let collector = GitHubIssuesCollector::empty(None).add_repo(RepoConfig::new(
            "nonexistent-owner-12345",
            "nonexistent-repo-67890",
        ));

        let result = collector.collect(1, 1).await;
        // Should return error for non-existent repo (404)
        assert!(result.is_err());
    }
}
