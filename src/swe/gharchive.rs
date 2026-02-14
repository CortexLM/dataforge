//! GH Archive ingestion for SWE-Infinite style mining.

use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use chrono::{DateTime, Timelike, Utc};
use flate2::read::GzDecoder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const GH_ARCHIVE_BASE_URL: &str = "https://data.gharchive.org";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhArchiveEventId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhArchiveEvent {
    pub id: GhArchiveEventId,
    pub event_type: String,
    pub repository: String,
    pub actor: String,
    pub action: String,
    pub pull_number: u64,
    pub issue_number: Option<u64>,
    pub base_sha: String,
    pub merge_sha: String,
    pub title: String,
    pub body: String,
    pub language_hint: Option<String>,
    pub stars: u32,
    pub has_org: bool,
    pub event_payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GhArchiveClient {
    token: Option<String>,
    client: Client,
}

impl GhArchiveClient {
    pub fn new(token: Option<String>) -> Self {
        Self {
            token,
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Fetch GH Archive events from recent hourly buckets.
    pub async fn fetch_events(
        &self,
        max_hours_back: u32,
    ) -> Result<Vec<GhArchiveEvent>, anyhow::Error> {
        let max_hours = max_hours_back.max(1);

        // Build all hour keys
        let keys: Vec<String> = (1..=max_hours)
            .map(|offset| {
                let bucket = Utc::now() - chrono::Duration::hours(offset as i64);
                format!("{}-{}", bucket.format("%Y-%m-%d"), bucket.hour())
            })
            .collect();

        // Fetch all hours in parallel (up to 8 concurrent)
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
        let mut handles = Vec::with_capacity(keys.len());

        for key in keys {
            let sem = semaphore.clone();
            let client = self.client.clone();
            let token = self.token.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                fetch_hour_events_static(&client, &token, &key).await
                    .inspect(|batch| {
                        tracing::info!(hour = %key, events = batch.len(), "Fetched GH Archive hour");
                    })
                    .map_err(|err| {
                        tracing::warn!(hour = %key, error = %err, "GH Archive hour fetch failed");
                        err
                    })
            }));
        }

        let mut events = Vec::new();
        for handle in handles {
            if let Ok(Ok(batch)) = handle.await {
                events.extend(batch);
            }
        }

        if events.is_empty() {
            anyhow::bail!(
                "No events found in GH Archive for the last {} hours. \
                 This may indicate a network issue or GH Archive downtime.",
                max_hours
            );
        }

        Ok(events)
    }

    pub fn to_filterable_payload(event: &GhArchiveEvent) -> HashMap<&str, String> {
        let mut map = HashMap::new();
        map.insert("repo", event.repository.clone());
        map.insert("action", event.action.clone());
        map.insert("language", event.language_hint.clone().unwrap_or_default());
        map
    }
}

async fn fetch_hour_events_static(
    client: &reqwest::Client,
    token: &Option<String>,
    hour_key: &str,
) -> Result<Vec<GhArchiveEvent>, anyhow::Error> {
    let mut request = client
        .get(format!("{GH_ARCHIVE_BASE_URL}/{hour_key}.json.gz"))
        .header("User-Agent", "dataforge/1.0");

    if let Some(ref token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed downloading gharchive {hour_key}: {e}"))?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "gharchive returned HTTP {} for {}",
            response.status(),
            hour_key
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("failed reading gharchive payload: {e}"))?;
    let mut decoder = GzDecoder::new(bytes.as_ref());
    let mut raw = String::new();
    decoder
        .read_to_string(&mut raw)
        .map_err(|e| anyhow::anyhow!("failed to decode gharchive payload: {e}"))?;

    let mut events = Vec::new();
    for line in raw.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(err) => {
                tracing::debug!(error = %err, "skip malformed gharchive line");
                continue;
            }
        };
        if let Some(event) = parse_github_archive_event(&value) {
            events.push(event);
        }
    }

    Ok(events)
}

fn parse_github_archive_event(value: &Value) -> Option<GhArchiveEvent> {
    let event_type = value.get("type").and_then(Value::as_str)?;
    let payload = value
        .get("payload")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let pull_request = payload.get("pull_request");
    let issue = payload.get("issue");
    let raw_action = payload
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let pr_merged = pull_request
        .and_then(|pr| pr.get("merged"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action = if raw_action == "closed" && pr_merged {
        "merged"
    } else {
        raw_action
    };

    let repository = value
        .get("repo")
        .and_then(|repo| repo.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("unknown/repo")
        .to_string();

    let actor = value
        .get("actor")
        .and_then(|actor| actor.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let pull_number = pull_request
        .and_then(|pr| pr.get("number"))
        .or_else(|| issue.and_then(|i| i.get("number")))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let issue_number = issue.and_then(|i| i.get("number")).and_then(Value::as_u64);
    let base_sha = pull_request
        .and_then(|pr| pr.get("base"))
        .and_then(|base| base.get("sha"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let merge_sha = pull_request
        .and_then(|pr| pr.get("merge_commit_sha"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let title = pull_request
        .and_then(|pr| pr.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled change")
        .to_string();
    let body = pull_request
        .and_then(|pr| pr.get("body"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let language_hint = pull_request
        .and_then(|pr| pr.get("head"))
        .and_then(|head| head.get("repo"))
        .and_then(|repo| repo.get("language"))
        .and_then(Value::as_str)
        .map(|s| s.to_lowercase());
    let stars = payload
        .get("repository")
        .and_then(|repo| repo.get("watchers_count"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);

    let id = value.get("id").and_then(Value::as_u64).unwrap_or_else(|| {
        let fallback = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        u64::try_from(fallback).unwrap_or(0)
    });

    let has_org = value.get("org").is_some();

    Some(GhArchiveEvent {
        id: GhArchiveEventId(format!("evt-{id}")),
        event_type: event_type.to_string(),
        repository,
        actor,
        action: action.to_string(),
        pull_number,
        issue_number,
        base_sha,
        merge_sha,
        title,
        body,
        language_hint,
        stars,
        has_org,
        event_payload: payload,
        created_at: Utc::now(),
    })
}
