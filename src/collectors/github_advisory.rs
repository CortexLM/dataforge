//! GitHub Advisory Database collector.
//!
//! This module provides a collector for fetching security advisories from the
//! GitHub Advisory Database using the GraphQL API. It collects CVEs and GHSAs
//! with vulnerability information and fix references.

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use super::types::{
    CollectedTask, CollectorConfig, CollectorError, CollectorResult, PaginatedResponse, TaskSource,
};

/// GitHub GraphQL API endpoint.
const GITHUB_GRAPHQL_API: &str = "https://api.github.com/graphql";

/// Severity levels for security advisories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// Critical severity - immediate action required.
    Critical,
    /// High severity - urgent fix needed.
    High,
    /// Medium severity - should be addressed.
    Medium,
    /// Low severity - minor concern.
    Low,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::High => write!(f, "HIGH"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::Low => write!(f, "LOW"),
        }
    }
}

impl Severity {
    /// Convert severity to a difficulty score (0.0 - 1.0).
    pub fn to_difficulty(&self) -> f64 {
        match self {
            Severity::Critical => 0.95,
            Severity::High => 0.75,
            Severity::Medium => 0.50,
            Severity::Low => 0.25,
        }
    }
}

/// Package ecosystems supported by GitHub Advisory Database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Ecosystem {
    /// Python Package Index (pip).
    Pip,
    /// Node Package Manager.
    Npm,
    /// Maven (Java).
    Maven,
    /// Cargo (Rust).
    Cargo,
    /// Go modules.
    Go,
    /// RubyGems.
    Rubygems,
    /// NuGet (.NET).
    Nuget,
    /// Composer (PHP).
    Composer,
    /// GitHub Actions.
    Actions,
    /// Erlang/Hex.
    Erlang,
    /// Pub (Dart/Flutter).
    Pub,
    /// Swift Package Manager.
    Swift,
}

impl fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ecosystem::Pip => write!(f, "PIP"),
            Ecosystem::Npm => write!(f, "NPM"),
            Ecosystem::Maven => write!(f, "MAVEN"),
            Ecosystem::Cargo => write!(f, "CARGO"),
            Ecosystem::Go => write!(f, "GO"),
            Ecosystem::Rubygems => write!(f, "RUBYGEMS"),
            Ecosystem::Nuget => write!(f, "NUGET"),
            Ecosystem::Composer => write!(f, "COMPOSER"),
            Ecosystem::Actions => write!(f, "ACTIONS"),
            Ecosystem::Erlang => write!(f, "ERLANG"),
            Ecosystem::Pub => write!(f, "PUB"),
            Ecosystem::Swift => write!(f, "SWIFT"),
        }
    }
}

/// Configuration for the GitHub Advisory collector.
#[derive(Debug, Clone)]
pub struct GitHubAdvisoryConfig {
    /// Delay between requests in milliseconds.
    pub rate_limit_delay_ms: u64,
    /// Maximum items per request.
    pub max_page_size: usize,
    /// Maximum retry attempts on failure.
    pub max_retries: u32,
}

impl Default for GitHubAdvisoryConfig {
    fn default() -> Self {
        Self {
            rate_limit_delay_ms: 100,
            max_page_size: 100,
            max_retries: 3,
        }
    }
}

impl CollectorConfig for GitHubAdvisoryConfig {
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

/// Collector for GitHub Advisory Database.
///
/// Fetches security advisories (CVEs/GHSAs) with vulnerability information
/// including affected packages, severity levels, and fix references.
///
/// # Example
///
/// ```ignore
/// use dataforge::collectors::{GitHubAdvisoryCollector, Ecosystem, Severity};
///
/// let collector = GitHubAdvisoryCollector::new(Some("ghp_xxxxx".to_string()))
///     .with_ecosystem(Ecosystem::Pip)
///     .with_severities(vec![Severity::Critical, Severity::High]);
///
/// let response = collector.collect(10, None).await?;
/// for task in response.items {
///     println!("Advisory: {} - {}", task.id, task.problem_statement);
/// }
/// ```
pub struct GitHubAdvisoryCollector {
    /// HTTP client for API requests.
    http_client: Client,
    /// Optional GitHub API token for higher rate limits.
    api_token: Option<String>,
    /// Filter by ecosystem (optional).
    ecosystem: Option<Ecosystem>,
    /// Filter by severity levels.
    severities: Vec<Severity>,
    /// Collector configuration.
    config: GitHubAdvisoryConfig,
}

impl GitHubAdvisoryCollector {
    /// Create a new GitHub Advisory collector.
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
            ecosystem: None,
            severities: Vec::new(),
            config: GitHubAdvisoryConfig::default(),
        }
    }

    /// Filter advisories by ecosystem.
    pub fn with_ecosystem(mut self, ecosystem: Ecosystem) -> Self {
        self.ecosystem = Some(ecosystem);
        self
    }

    /// Filter advisories by severity levels.
    pub fn with_severities(mut self, severities: Vec<Severity>) -> Self {
        self.severities = severities;
        self
    }

    /// Add a severity filter.
    pub fn add_severity(mut self, severity: Severity) -> Self {
        if !self.severities.contains(&severity) {
            self.severities.push(severity);
        }
        self
    }

    /// Configure the collector with custom settings.
    pub fn with_config(mut self, config: GitHubAdvisoryConfig) -> Self {
        self.config = config;
        self
    }

    /// Check if an API token is configured.
    pub fn has_token(&self) -> bool {
        self.api_token.is_some()
    }

    /// Collect security advisories from GitHub Advisory Database.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of advisories to fetch
    /// * `cursor` - Optional cursor for pagination (from previous response)
    ///
    /// # Returns
    ///
    /// A paginated response containing collected tasks and pagination info.
    ///
    /// # Errors
    ///
    /// Returns `CollectorError` if:
    /// - The HTTP request fails
    /// - The GraphQL response contains errors
    /// - No API token is provided (required for GraphQL API)
    pub async fn collect(
        &self,
        limit: usize,
        cursor: Option<String>,
    ) -> CollectorResult<PaginatedResponse<CollectedTask>> {
        let token = self.api_token.as_ref().ok_or_else(|| {
            CollectorError::HttpError("GitHub API token required for GraphQL queries".to_string())
        })?;

        let effective_limit = limit.min(self.config.max_page_size);
        let query = self.build_graphql_query(effective_limit, cursor.as_deref());

        let response = self
            .http_client
            .post(GITHUB_GRAPHQL_API)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("User-Agent", "dataforge/1.0")
            .json(&serde_json::json!({ "query": query }))
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

        let graphql_response: GraphQLResponse = response
            .json()
            .await
            .map_err(|e| CollectorError::ParseError(format!("Failed to parse response: {}", e)))?;

        // Check for GraphQL errors
        if let Some(errors) = graphql_response.errors {
            if !errors.is_empty() {
                let error_messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
                return Err(CollectorError::InvalidResponse(format!(
                    "GraphQL errors: {}",
                    error_messages.join("; ")
                )));
            }
        }

        let data = graphql_response.data.ok_or_else(|| {
            CollectorError::InvalidResponse("No data in GraphQL response".to_string())
        })?;

        let advisories = data.security_advisories;
        let page_info = advisories.page_info;

        let tasks: Vec<CollectedTask> = advisories
            .nodes
            .into_iter()
            .filter_map(|node| self.convert_advisory_to_task(node))
            .collect();

        Ok(PaginatedResponse {
            items: tasks,
            next_cursor: if page_info.has_next_page {
                page_info.end_cursor
            } else {
                None
            },
            has_more: page_info.has_next_page,
        })
    }

    /// Build the GraphQL query for fetching advisories.
    fn build_graphql_query(&self, limit: usize, cursor: Option<&str>) -> String {
        let after_clause = cursor
            .map(|c| format!(", after: \"{}\"", c))
            .unwrap_or_default();

        let ecosystem_clause = self
            .ecosystem
            .map(|e| format!(", ecosystem: {}", e))
            .unwrap_or_default();

        let severity_clause = if self.severities.is_empty() {
            String::new()
        } else {
            let severities: Vec<_> = self.severities.iter().map(|s| s.to_string()).collect();
            format!(", severities: [{}]", severities.join(", "))
        };

        format!(
            r#"
            query {{
                securityAdvisories(first: {}{}{}{}, orderBy: {{field: PUBLISHED_AT, direction: DESC}}) {{
                    nodes {{
                        ghsaId
                        summary
                        description
                        severity
                        publishedAt
                        updatedAt
                        permalink
                        identifiers {{
                            type
                            value
                        }}
                        vulnerabilities(first: 10) {{
                            nodes {{
                                package {{
                                    ecosystem
                                    name
                                }}
                                vulnerableVersionRange
                                firstPatchedVersion {{
                                    identifier
                                }}
                            }}
                        }}
                        references {{
                            url
                        }}
                        cvss {{
                            score
                            vectorString
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        endCursor
                    }}
                }}
            }}
            "#,
            limit, after_clause, ecosystem_clause, severity_clause
        )
    }

    /// Convert a GraphQL advisory node to a CollectedTask.
    fn convert_advisory_to_task(&self, advisory: AdvisoryNode) -> Option<CollectedTask> {
        let ghsa_id = advisory.ghsa_id;

        // Find CVE identifier if available
        let cve_id = advisory
            .identifiers
            .iter()
            .find(|id| id.id_type == "CVE")
            .map(|id| id.value.clone());

        let id = cve_id.clone().unwrap_or_else(|| ghsa_id.clone());

        // Build problem statement from summary and description
        let problem_statement = format!(
            "{}\n\n{}",
            advisory.summary,
            advisory.description.unwrap_or_default()
        )
        .trim()
        .to_string();

        if problem_statement.is_empty() {
            return None;
        }

        // Extract affected packages
        let affected_packages: Vec<String> = advisory
            .vulnerabilities
            .nodes
            .iter()
            .map(|v| format!("{}:{}", v.package.ecosystem, v.package.name))
            .collect();

        // Extract fix commits from references
        let fix_references: Vec<String> = advisory
            .references
            .iter()
            .filter(|r| {
                r.url.contains("/commit/")
                    || r.url.contains("/pull/")
                    || r.url.contains("/releases/tag/")
            })
            .map(|r| r.url.clone())
            .collect();

        // Determine difficulty from severity
        let difficulty = advisory
            .severity
            .as_ref()
            .and_then(|s| match s.as_str() {
                "CRITICAL" => Some(Severity::Critical),
                "HIGH" => Some(Severity::High),
                "MEDIUM" | "MODERATE" => Some(Severity::Medium),
                "LOW" => Some(Severity::Low),
                _ => None,
            })
            .map(|s| s.to_difficulty())
            .unwrap_or(0.5);

        // Build tags
        let mut tags = vec!["security".to_string()];
        if let Some(ref severity) = advisory.severity {
            tags.push(severity.to_lowercase());
        }
        if cve_id.is_some() {
            tags.push("cve".to_string());
        }
        // Add ecosystem tags
        for vuln in &advisory.vulnerabilities.nodes {
            let ecosystem_tag = vuln.package.ecosystem.to_lowercase();
            if !tags.contains(&ecosystem_tag) {
                tags.push(ecosystem_tag);
            }
        }

        // Build metadata
        let metadata = serde_json::json!({
            "ghsa_id": ghsa_id,
            "cve_id": cve_id,
            "severity": advisory.severity,
            "cvss": advisory.cvss,
            "affected_packages": affected_packages,
            "fix_references": fix_references,
            "published_at": advisory.published_at,
            "updated_at": advisory.updated_at,
            "permalink": advisory.permalink,
            "vulnerable_versions": advisory.vulnerabilities.nodes.iter()
                .map(|v| serde_json::json!({
                    "package": format!("{}:{}", v.package.ecosystem, v.package.name),
                    "range": v.vulnerable_version_range,
                    "patched": v.first_patched_version.as_ref().map(|p| &p.identifier),
                }))
                .collect::<Vec<_>>(),
        });

        // Determine repository from first vulnerability or references
        let repo = advisory
            .references
            .iter()
            .find(|r| r.url.contains("github.com"))
            .and_then(|r| extract_repo_from_url(&r.url))
            .unwrap_or_else(|| {
                // Fall back to package name if available
                advisory
                    .vulnerabilities
                    .nodes
                    .first()
                    .map(|v| v.package.name.clone())
                    .unwrap_or_else(|| "unknown/unknown".to_string())
            });

        let task = CollectedTask {
            id,
            source: TaskSource::GitHubAdvisory,
            repo,
            base_commit: None,
            problem_statement,
            solution_patch: None,
            test_commands: Vec::new(),
            difficulty_estimate: Some(difficulty),
            category: "security".to_string(),
            tags,
            metadata,
            collected_at: Utc::now(),
        };

        Some(task)
    }
}

/// Extract repository path from a GitHub URL.
fn extract_repo_from_url(url: &str) -> Option<String> {
    // Handle URLs like https://github.com/owner/repo/...
    let url = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() >= 2 {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

/// GraphQL response wrapper.
#[derive(Debug, Deserialize)]
struct GraphQLResponse {
    data: Option<GraphQLData>,
    errors: Option<Vec<GraphQLError>>,
}

/// GraphQL error.
#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

/// GraphQL data payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLData {
    security_advisories: AdvisoriesConnection,
}

/// Connection for paginated advisories.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdvisoriesConnection {
    nodes: Vec<AdvisoryNode>,
    page_info: PageInfo,
}

/// Page info for pagination.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

/// Individual advisory node.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdvisoryNode {
    ghsa_id: String,
    summary: String,
    description: Option<String>,
    severity: Option<String>,
    published_at: Option<String>,
    updated_at: Option<String>,
    permalink: Option<String>,
    identifiers: Vec<Identifier>,
    vulnerabilities: VulnerabilitiesConnection,
    references: Vec<Reference>,
    cvss: Option<CvssInfo>,
}

/// Advisory identifier (CVE, GHSA, etc.).
#[derive(Debug, Deserialize)]
struct Identifier {
    #[serde(rename = "type")]
    id_type: String,
    value: String,
}

/// Connection for vulnerabilities.
#[derive(Debug, Deserialize)]
struct VulnerabilitiesConnection {
    nodes: Vec<VulnerabilityNode>,
}

/// Individual vulnerability node.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VulnerabilityNode {
    package: PackageInfo,
    vulnerable_version_range: Option<String>,
    first_patched_version: Option<PatchedVersion>,
}

/// Package information.
#[derive(Debug, Deserialize)]
struct PackageInfo {
    ecosystem: String,
    name: String,
}

/// Patched version info.
#[derive(Debug, Deserialize)]
struct PatchedVersion {
    identifier: String,
}

/// Reference URL.
#[derive(Debug, Deserialize)]
struct Reference {
    url: String,
}

/// CVSS score information.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CvssInfo {
    score: f64,
    vector_string: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
        assert_eq!(Severity::High.to_string(), "HIGH");
        assert_eq!(Severity::Medium.to_string(), "MEDIUM");
        assert_eq!(Severity::Low.to_string(), "LOW");
    }

    #[test]
    fn test_severity_to_difficulty() {
        assert!(Severity::Critical.to_difficulty() > 0.9);
        assert!(Severity::High.to_difficulty() > 0.6);
        assert!(Severity::Medium.to_difficulty() > 0.4);
        assert!(Severity::Low.to_difficulty() < 0.3);
    }

    #[test]
    fn test_ecosystem_display() {
        assert_eq!(Ecosystem::Pip.to_string(), "PIP");
        assert_eq!(Ecosystem::Npm.to_string(), "NPM");
        assert_eq!(Ecosystem::Cargo.to_string(), "CARGO");
        assert_eq!(Ecosystem::Go.to_string(), "GO");
    }

    #[test]
    fn test_collector_creation() {
        let collector = GitHubAdvisoryCollector::new(Some("test-token".to_string()));
        assert!(collector.has_token());

        let collector_no_token = GitHubAdvisoryCollector::new(None);
        assert!(!collector_no_token.has_token());
    }

    #[test]
    fn test_collector_builder() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()))
            .with_ecosystem(Ecosystem::Pip)
            .with_severities(vec![Severity::Critical, Severity::High])
            .add_severity(Severity::Medium);

        assert_eq!(collector.ecosystem, Some(Ecosystem::Pip));
        assert_eq!(collector.severities.len(), 3);
    }

    #[test]
    fn test_add_severity_dedup() {
        let collector = GitHubAdvisoryCollector::new(None)
            .add_severity(Severity::High)
            .add_severity(Severity::High)
            .add_severity(Severity::Critical);

        // Should not have duplicate High
        assert_eq!(collector.severities.len(), 2);
    }

    #[test]
    fn test_build_graphql_query_basic() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()));
        let query = collector.build_graphql_query(10, None);

        assert!(query.contains("securityAdvisories(first: 10"));
        assert!(query.contains("ghsaId"));
        assert!(query.contains("severity"));
        assert!(query.contains("vulnerabilities"));
    }

    #[test]
    fn test_build_graphql_query_with_filters() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()))
            .with_ecosystem(Ecosystem::Pip)
            .with_severities(vec![Severity::Critical, Severity::High]);

        let query = collector.build_graphql_query(10, Some("cursor123"));

        assert!(query.contains("after: \"cursor123\""));
        assert!(query.contains("ecosystem: PIP"));
        assert!(query.contains("severities: [CRITICAL, HIGH]"));
    }

    #[test]
    fn test_extract_repo_from_url() {
        assert_eq!(
            extract_repo_from_url("https://github.com/owner/repo/commit/abc123"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            extract_repo_from_url("https://github.com/org/project/pull/42"),
            Some("org/project".to_string())
        );
        assert_eq!(extract_repo_from_url("https://example.com/other"), None);
        assert_eq!(extract_repo_from_url("https://github.com/single"), None);
    }

    #[test]
    fn test_convert_advisory_to_task() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()));

        let advisory = AdvisoryNode {
            ghsa_id: "GHSA-1234-5678-abcd".to_string(),
            summary: "SQL Injection vulnerability".to_string(),
            description: Some("A SQL injection vulnerability exists in...".to_string()),
            severity: Some("HIGH".to_string()),
            published_at: Some("2024-01-15T00:00:00Z".to_string()),
            updated_at: Some("2024-01-16T00:00:00Z".to_string()),
            permalink: Some("https://github.com/advisories/GHSA-1234".to_string()),
            identifiers: vec![
                Identifier {
                    id_type: "GHSA".to_string(),
                    value: "GHSA-1234-5678-abcd".to_string(),
                },
                Identifier {
                    id_type: "CVE".to_string(),
                    value: "CVE-2024-0001".to_string(),
                },
            ],
            vulnerabilities: VulnerabilitiesConnection {
                nodes: vec![VulnerabilityNode {
                    package: PackageInfo {
                        ecosystem: "PIP".to_string(),
                        name: "vulnerable-package".to_string(),
                    },
                    vulnerable_version_range: Some("< 1.2.3".to_string()),
                    first_patched_version: Some(PatchedVersion {
                        identifier: "1.2.3".to_string(),
                    }),
                }],
            },
            references: vec![
                Reference {
                    url: "https://github.com/owner/repo/commit/abc123".to_string(),
                },
                Reference {
                    url: "https://nvd.nist.gov/vuln/detail/CVE-2024-0001".to_string(),
                },
            ],
            cvss: Some(CvssInfo {
                score: 7.5,
                vector_string: Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H".to_string()),
            }),
        };

        let task = collector
            .convert_advisory_to_task(advisory)
            .expect("Should convert successfully");

        assert_eq!(task.id, "CVE-2024-0001");
        assert_eq!(task.source, TaskSource::GitHubAdvisory);
        assert_eq!(task.repo, "owner/repo");
        assert!(task.problem_statement.contains("SQL Injection"));
        assert_eq!(task.category, "security");
        assert!(task.tags.contains(&"security".to_string()));
        assert!(task.tags.contains(&"cve".to_string()));
        assert!(task.tags.contains(&"high".to_string()));
        assert!(task.difficulty_estimate.is_some());
    }

    #[test]
    fn test_convert_advisory_no_cve() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()));

        let advisory = AdvisoryNode {
            ghsa_id: "GHSA-xxxx-yyyy-zzzz".to_string(),
            summary: "Security issue".to_string(),
            description: None,
            severity: Some("MEDIUM".to_string()),
            published_at: None,
            updated_at: None,
            permalink: None,
            identifiers: vec![Identifier {
                id_type: "GHSA".to_string(),
                value: "GHSA-xxxx-yyyy-zzzz".to_string(),
            }],
            vulnerabilities: VulnerabilitiesConnection { nodes: vec![] },
            references: vec![],
            cvss: None,
        };

        let task = collector
            .convert_advisory_to_task(advisory)
            .expect("Should convert successfully");

        // Should use GHSA ID when no CVE
        assert_eq!(task.id, "GHSA-xxxx-yyyy-zzzz");
        assert!(!task.tags.contains(&"cve".to_string()));
    }

    #[test]
    fn test_convert_advisory_empty_summary() {
        let collector = GitHubAdvisoryCollector::new(Some("token".to_string()));

        let advisory = AdvisoryNode {
            ghsa_id: "GHSA-test".to_string(),
            summary: "".to_string(),
            description: None,
            severity: None,
            published_at: None,
            updated_at: None,
            permalink: None,
            identifiers: vec![],
            vulnerabilities: VulnerabilitiesConnection { nodes: vec![] },
            references: vec![],
            cvss: None,
        };

        // Should return None for empty problem statement
        assert!(collector.convert_advisory_to_task(advisory).is_none());
    }

    #[tokio::test]
    async fn test_collect_requires_token() {
        let collector = GitHubAdvisoryCollector::new(None);
        let result = collector.collect(10, None).await;

        assert!(result.is_err());
        if let Err(CollectorError::HttpError(msg)) = result {
            assert!(msg.contains("token required"));
        } else {
            panic!("Expected HttpError about missing token");
        }
    }

    #[test]
    fn test_default_config() {
        let config = GitHubAdvisoryConfig::default();
        assert_eq!(config.rate_limit_delay_ms, 100);
        assert_eq!(config.max_page_size, 100);
        assert_eq!(config.max_retries, 3);
    }
}
