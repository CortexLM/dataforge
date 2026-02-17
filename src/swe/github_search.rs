//! GitHub Search API client as an alternative PR source.
//!
//! Instead of downloading entire GH Archive hourly dumps and filtering locally,
//! this module uses the GitHub Search API to directly target high-quality merged
//! PRs matching specific criteria (language, stars, date range).
//!
//! The Search API has its own rate limit (30 requests/min authenticated, 10
//! unauthenticated) separate from the REST API (5000/h).

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use tokio::sync::Semaphore;

use super::gharchive::{GhArchiveEvent, GhArchiveEventId};

/// Configuration for GitHub Search queries.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Minimum repository star count.
    pub min_stars: u32,
    /// Programming language filter (e.g. "python").
    pub language: Option<String>,
    /// Only include PRs merged after this date.
    pub merged_after: Option<DateTime<Utc>>,
    /// Maximum number of results to return.
    pub max_results: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            min_stars: 20,
            language: None,
            merged_after: None,
            max_results: 100,
        }
    }
}

/// GitHub Search API client for finding merged PRs.
pub struct GitHubSearchClient {
    client: Client,
    token: Option<String>,
    /// Rate-limit semaphore: 30 req/min for authenticated users.
    semaphore: Arc<Semaphore>,
}

impl GitHubSearchClient {
    /// Create a new client with an optional GitHub token.
    pub fn new(token: Option<String>) -> Self {
        // Authenticated: 30 req/min; unauthenticated: 10 req/min.
        // Use conservative concurrency of 5 to stay well within limits.
        let max_concurrent = if token.is_some() { 5 } else { 2 };
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            token,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Build the search query string for the GitHub Search API.
    pub fn build_query(config: &SearchConfig) -> String {
        let mut parts = vec!["is:pr".to_string(), "is:merged".to_string()];

        if let Some(ref lang) = config.language {
            parts.push(format!("language:{lang}"));
        }

        if config.min_stars > 0 {
            parts.push(format!("stars:>{}", config.min_stars));
        }

        if let Some(ref date) = config.merged_after {
            parts.push(format!("merged:>{}", date.format("%Y-%m-%d")));
        }

        parts.join("+")
    }

    /// Search for merged PRs matching the given configuration.
    ///
    /// Returns events in `GhArchiveEvent`-compatible format so the pipeline
    /// can use them interchangeably with GH Archive events.
    pub async fn search_merged_prs(
        &self,
        config: &SearchConfig,
    ) -> Result<Vec<GhArchiveEvent>, anyhow::Error> {
        let query = Self::build_query(config);
        let mut all_events = Vec::new();
        let per_page = 100.min(config.max_results);
        let max_pages = config.max_results.div_ceil(per_page);

        for page in 1..=max_pages {
            let _permit = self.semaphore.acquire().await.map_err(|e| {
                anyhow::anyhow!("Failed to acquire search rate-limit semaphore: {e}")
            })?;

            let url = format!(
                "https://api.github.com/search/issues?q={}&sort=updated&order=desc&per_page={}&page={}",
                query, per_page, page
            );

            let mut request = self
                .client
                .get(&url)
                .header("User-Agent", "swe_forge/1.0")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");

            if let Some(ref token) = self.token {
                request = request.header("Authorization", format!("Bearer {token}"));
            }

            let response = request
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("GitHub Search API request failed: {e}"))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                tracing::warn!(
                    status = %status,
                    body = %body,
                    "GitHub Search API returned error"
                );
                if status.as_u16() == 403 || status.as_u16() == 429 {
                    tracing::warn!("Rate limited, stopping search pagination");
                    break;
                }
                anyhow::bail!("GitHub Search API returned HTTP {status}");
            }

            let raw: Value = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse GitHub Search API response: {e}"))?;

            let items = raw
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if items.is_empty() {
                break;
            }

            for item in &items {
                if let Some(event) = parse_search_result(item) {
                    all_events.push(event);
                }
            }

            tracing::info!(
                page = page,
                items = items.len(),
                total = all_events.len(),
                "GitHub Search API page fetched"
            );

            if all_events.len() >= config.max_results {
                all_events.truncate(config.max_results);
                break;
            }

            // Respect rate limits with a small delay between pages
            tokio::time::sleep(Duration::from_millis(2100)).await;
        }

        tracing::info!(total = all_events.len(), "GitHub Search completed");

        Ok(all_events)
    }
}

/// Parse a single GitHub Search API result item into a `GhArchiveEvent`.
fn parse_search_result(item: &Value) -> Option<GhArchiveEvent> {
    let html_url = item.get("html_url").and_then(Value::as_str)?;

    // Extract repo from URL: https://github.com/owner/repo/pull/123
    let parts: Vec<&str> = html_url.split('/').collect();
    let repository = if parts.len() >= 5 {
        format!("{}/{}", parts[3], parts[4])
    } else {
        return None;
    };

    let pull_number = item.get("number").and_then(Value::as_u64)?;
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled change")
        .to_string();
    let body = item
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let actor = item
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let id = item.get("id").and_then(Value::as_u64).unwrap_or_else(|| {
        let fallback = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        u64::try_from(fallback).unwrap_or(0)
    });

    Some(GhArchiveEvent {
        id: GhArchiveEventId(format!("search-{id}")),
        event_type: "PullRequestEvent".to_string(),
        repository,
        actor,
        action: "merged".to_string(),
        pull_number,
        issue_number: None,
        base_sha: String::new(),
        merge_sha: String::new(),
        title,
        body,
        language_hint: None,
        stars: 0,
        has_org: true, // Search results are typically org repos
        event_payload: Value::Object(Default::default()),
        created_at: Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_query_basic() {
        let config = SearchConfig {
            min_stars: 50,
            language: Some("python".to_string()),
            merged_after: None,
            max_results: 100,
        };
        let query = GitHubSearchClient::build_query(&config);
        assert!(query.contains("is:pr"));
        assert!(query.contains("is:merged"));
        assert!(query.contains("language:python"));
        assert!(query.contains("stars:>50"));
    }

    #[test]
    fn build_query_with_date() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 6, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

        let config = SearchConfig {
            min_stars: 100,
            language: Some("rust".to_string()),
            merged_after: date,
            max_results: 50,
        };
        let query = GitHubSearchClient::build_query(&config);
        assert!(query.contains("merged:>2024-06-01"));
        assert!(query.contains("language:rust"));
        assert!(query.contains("stars:>100"));
    }

    #[test]
    fn build_query_no_language() {
        let config = SearchConfig {
            min_stars: 20,
            language: None,
            merged_after: None,
            max_results: 100,
        };
        let query = GitHubSearchClient::build_query(&config);
        assert!(!query.contains("language:"));
        assert!(query.contains("is:pr"));
        assert!(query.contains("is:merged"));
    }

    #[test]
    fn parse_search_result_valid() {
        let item = serde_json::json!({
            "html_url": "https://github.com/owner/repo/pull/42",
            "number": 42,
            "title": "Fix bug in parser",
            "body": "This PR fixes a parsing issue.",
            "user": {"login": "testuser"},
            "id": 12345
        });
        let event = parse_search_result(&item).expect("should parse");
        assert_eq!(event.repository, "owner/repo");
        assert_eq!(event.pull_number, 42);
        assert_eq!(event.title, "Fix bug in parser");
        assert_eq!(event.actor, "testuser");
        assert_eq!(event.action, "merged");
    }

    #[test]
    fn parse_search_result_missing_url() {
        let item = serde_json::json!({
            "number": 42,
            "title": "Fix bug"
        });
        assert!(parse_search_result(&item).is_none());
    }

    #[test]
    fn parse_search_result_missing_number() {
        let item = serde_json::json!({
            "html_url": "https://github.com/owner/repo/pull/42",
            "title": "Fix bug"
        });
        assert!(parse_search_result(&item).is_none());
    }

    #[test]
    fn client_creation() {
        let client = GitHubSearchClient::new(Some("test-token".to_string()));
        assert!(client.token.is_some());

        let client_no_token = GitHubSearchClient::new(None);
        assert!(client_no_token.token.is_none());
    }
}
