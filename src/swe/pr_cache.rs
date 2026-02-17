//! SQLite-backed PR cache to avoid redundant LLM calls and API requests.

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use std::sync::Arc;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS pr_cache (
    repo            TEXT    NOT NULL,
    pr_number       INTEGER NOT NULL,

    -- Stage 1: GH Archive discovery
    gh_event_id     TEXT,
    actor           TEXT,
    title           TEXT,
    body            TEXT,
    language        TEXT,
    stars           INTEGER,
    base_sha        TEXT,
    merge_sha       TEXT,
    files_changed   INTEGER,
    has_org         INTEGER,

    -- Stage 2: Pre-classification (LLM triage)
    triage_difficulty TEXT,

    -- Stage 3: Deep processing
    patch           TEXT,
    test_patch      TEXT,
    difficulty_score INTEGER,
    quality_score   REAL,
    quality_passed  INTEGER,

    -- Final status
    status          TEXT NOT NULL DEFAULT 'discovered',
    rejection_reason TEXT,

    -- Timestamps
    first_seen_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),

    PRIMARY KEY (repo, pr_number)
);

CREATE INDEX IF NOT EXISTS idx_pr_cache_status ON pr_cache(status);
CREATE INDEX IF NOT EXISTS idx_pr_cache_triage ON pr_cache(triage_difficulty);
"#;

#[derive(Debug, Clone, Default)]
pub struct PrCacheEntry {
    pub repo: String,
    pub pr_number: u64,
    pub gh_event_id: Option<String>,
    pub actor: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub language: Option<String>,
    pub stars: Option<u32>,
    pub base_sha: Option<String>,
    pub merge_sha: Option<String>,
    pub files_changed: Option<usize>,
    pub has_org: Option<bool>,
    pub triage_difficulty: Option<String>,
    pub patch: Option<String>,
    pub test_patch: Option<String>,
    pub difficulty_score: Option<u8>,
    pub quality_score: Option<f64>,
    pub quality_passed: Option<bool>,
    pub status: String,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub total: u64,
    pub discovered: u64,
    pub enriched: u64,
    pub pre_classified: u64,
    pub extracted: u64,
    pub scored: u64,
    pub exported: u64,
    pub rejected: u64,
}

#[derive(Clone)]
pub struct PrCache {
    pool: SqlitePool,
}

impl PrCache {
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;

        sqlx::query(SCHEMA_SQL).execute(&pool).await?;

        tracing::info!(path = path, "PR cache opened");
        Ok(Self { pool })
    }

    pub async fn get(&self, repo: &str, pr: u64) -> Option<PrCacheEntry> {
        let row = sqlx::query("SELECT * FROM pr_cache WHERE repo = ?1 AND pr_number = ?2")
            .bind(repo)
            .bind(pr as i64)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()?;

        Some(PrCacheEntry {
            repo: row.get("repo"),
            pr_number: row.get::<i64, _>("pr_number") as u64,
            gh_event_id: row.get("gh_event_id"),
            actor: row.get("actor"),
            title: row.get("title"),
            body: row.get("body"),
            language: row.get("language"),
            stars: row.get::<Option<i32>, _>("stars").map(|v| v as u32),
            base_sha: row.get("base_sha"),
            merge_sha: row.get("merge_sha"),
            files_changed: row
                .get::<Option<i32>, _>("files_changed")
                .map(|v| v as usize),
            has_org: row.get::<Option<i32>, _>("has_org").map(|v| v != 0),
            triage_difficulty: row.get("triage_difficulty"),
            patch: row.get("patch"),
            test_patch: row.get("test_patch"),
            difficulty_score: row
                .get::<Option<i32>, _>("difficulty_score")
                .map(|v| v as u8),
            quality_score: row.get("quality_score"),
            quality_passed: row.get::<Option<i32>, _>("quality_passed").map(|v| v != 0),
            status: row.get("status"),
            rejection_reason: row.get("rejection_reason"),
        })
    }

    pub async fn upsert(&self, e: &PrCacheEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO pr_cache (
                repo, pr_number, gh_event_id, actor, title, body, language,
                stars, base_sha, merge_sha, files_changed, has_org,
                triage_difficulty, patch, test_patch, difficulty_score,
                quality_score, quality_passed, status, rejection_reason,
                updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,datetime('now'))
            ON CONFLICT(repo, pr_number) DO UPDATE SET
                gh_event_id = COALESCE(excluded.gh_event_id, pr_cache.gh_event_id),
                actor = COALESCE(excluded.actor, pr_cache.actor),
                title = COALESCE(excluded.title, pr_cache.title),
                body = COALESCE(excluded.body, pr_cache.body),
                language = COALESCE(excluded.language, pr_cache.language),
                stars = COALESCE(excluded.stars, pr_cache.stars),
                base_sha = COALESCE(excluded.base_sha, pr_cache.base_sha),
                merge_sha = COALESCE(excluded.merge_sha, pr_cache.merge_sha),
                files_changed = COALESCE(excluded.files_changed, pr_cache.files_changed),
                has_org = COALESCE(excluded.has_org, pr_cache.has_org),
                triage_difficulty = COALESCE(excluded.triage_difficulty, pr_cache.triage_difficulty),
                patch = COALESCE(excluded.patch, pr_cache.patch),
                test_patch = COALESCE(excluded.test_patch, pr_cache.test_patch),
                difficulty_score = COALESCE(excluded.difficulty_score, pr_cache.difficulty_score),
                quality_score = COALESCE(excluded.quality_score, pr_cache.quality_score),
                quality_passed = COALESCE(excluded.quality_passed, pr_cache.quality_passed),
                status = excluded.status,
                rejection_reason = COALESCE(excluded.rejection_reason, pr_cache.rejection_reason),
                updated_at = datetime('now')",
        )
        .bind(&e.repo)
        .bind(e.pr_number as i64)
        .bind(&e.gh_event_id)
        .bind(&e.actor)
        .bind(&e.title)
        .bind(&e.body)
        .bind(&e.language)
        .bind(e.stars.map(|v| v as i32))
        .bind(&e.base_sha)
        .bind(&e.merge_sha)
        .bind(e.files_changed.map(|v| v as i32))
        .bind(e.has_org.map(|v| v as i32))
        .bind(&e.triage_difficulty)
        .bind(&e.patch)
        .bind(&e.test_patch)
        .bind(e.difficulty_score.map(|v| v as i32))
        .bind(e.quality_score)
        .bind(e.quality_passed.map(|v| v as i32))
        .bind(&e.status)
        .bind(&e.rejection_reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns true if this PR should be skipped (already exported or rejected).
    pub async fn should_skip(&self, repo: &str, pr: u64) -> bool {
        let row = sqlx::query("SELECT status FROM pr_cache WHERE repo = ?1 AND pr_number = ?2")
            .bind(repo)
            .bind(pr as i64)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();

        match row {
            Some(r) => {
                let status: String = r.get("status");
                status == "exported" || status == "rejected"
            }
            None => false,
        }
    }

    /// Get cached triage difficulty (avoids re-running LLM pre-classification).
    pub async fn triage_difficulty(&self, repo: &str, pr: u64) -> Option<String> {
        let row = sqlx::query(
            "SELECT triage_difficulty FROM pr_cache WHERE repo = ?1 AND pr_number = ?2",
        )
        .bind(repo)
        .bind(pr as i64)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()?;

        row.get("triage_difficulty")
    }

    pub async fn mark_rejected(&self, repo: &str, pr: u64, reason: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pr_cache SET status = 'rejected', rejection_reason = ?3, updated_at = datetime('now')
             WHERE repo = ?1 AND pr_number = ?2",
        )
        .bind(repo)
        .bind(pr as i64)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_exported(&self, repo: &str, pr: u64) -> Result<()> {
        sqlx::query(
            "UPDATE pr_cache SET status = 'exported', updated_at = datetime('now')
             WHERE repo = ?1 AND pr_number = ?2",
        )
        .bind(repo)
        .bind(pr as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn stats(&self) -> CacheStats {
        let row = sqlx::query(
            "SELECT
                COUNT(*) as total,
                SUM(CASE WHEN status = 'discovered' THEN 1 ELSE 0 END) as discovered,
                SUM(CASE WHEN status = 'enriched' THEN 1 ELSE 0 END) as enriched,
                SUM(CASE WHEN status = 'pre_classified' THEN 1 ELSE 0 END) as pre_classified,
                SUM(CASE WHEN status = 'extracted' THEN 1 ELSE 0 END) as extracted,
                SUM(CASE WHEN status = 'scored' THEN 1 ELSE 0 END) as scored,
                SUM(CASE WHEN status = 'exported' THEN 1 ELSE 0 END) as exported,
                SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END) as rejected
             FROM pr_cache",
        )
        .fetch_one(&self.pool)
        .await;

        match row {
            Ok(r) => CacheStats {
                total: r.get::<i64, _>("total") as u64,
                discovered: r.get::<i64, _>("discovered") as u64,
                enriched: r.get::<i64, _>("enriched") as u64,
                pre_classified: r.get::<i64, _>("pre_classified") as u64,
                extracted: r.get::<i64, _>("extracted") as u64,
                scored: r.get::<i64, _>("scored") as u64,
                exported: r.get::<i64, _>("exported") as u64,
                rejected: r.get::<i64, _>("rejected") as u64,
            },
            Err(_) => CacheStats::default(),
        }
    }
}

/// Wraps an optional PrCache so the pipeline works with or without caching.
#[derive(Clone)]
pub struct OptionalCache(pub Option<Arc<PrCache>>);

impl std::fmt::Debug for OptionalCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(_) => write!(f, "OptionalCache(active)"),
            None => write!(f, "OptionalCache(none)"),
        }
    }
}

impl OptionalCache {
    pub fn none() -> Self {
        Self(None)
    }

    pub fn some(cache: PrCache) -> Self {
        Self(Some(Arc::new(cache)))
    }

    pub async fn should_skip(&self, repo: &str, pr: u64) -> bool {
        match &self.0 {
            Some(c) => c.should_skip(repo, pr).await,
            None => false,
        }
    }

    pub async fn triage_difficulty(&self, repo: &str, pr: u64) -> Option<String> {
        match &self.0 {
            Some(c) => c.triage_difficulty(repo, pr).await,
            None => None,
        }
    }

    pub async fn upsert(&self, entry: &PrCacheEntry) -> Result<()> {
        match &self.0 {
            Some(c) => c.upsert(entry).await,
            None => Ok(()),
        }
    }

    pub async fn mark_rejected(&self, repo: &str, pr: u64, reason: &str) -> Result<()> {
        match &self.0 {
            Some(c) => c.mark_rejected(repo, pr, reason).await,
            None => Ok(()),
        }
    }

    pub async fn mark_exported(&self, repo: &str, pr: u64) -> Result<()> {
        match &self.0 {
            Some(c) => c.mark_exported(repo, pr).await,
            None => Ok(()),
        }
    }

    pub async fn log_stats(&self) {
        if let Some(c) = &self.0 {
            let s = c.stats().await;
            tracing::info!(
                total = s.total,
                exported = s.exported,
                rejected = s.rejected,
                pre_classified = s.pre_classified,
                "PR cache stats"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_and_upsert() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        let entry = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 42,
            title: Some("Fix bug".to_string()),
            status: "discovered".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry).await.unwrap();

        let got = cache.get("owner/repo", 42).await.unwrap();
        assert_eq!(got.title.as_deref(), Some("Fix bug"));
        assert_eq!(got.status, "discovered");
    }

    #[tokio::test]
    async fn test_should_skip_exported() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        let entry = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 1,
            status: "discovered".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry).await.unwrap();
        assert!(!cache.should_skip("owner/repo", 1).await);

        cache.mark_exported("owner/repo", 1).await.unwrap();
        assert!(cache.should_skip("owner/repo", 1).await);
    }

    #[tokio::test]
    async fn test_should_skip_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        let entry = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 2,
            status: "discovered".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry).await.unwrap();
        cache
            .mark_rejected("owner/repo", 2, "too easy")
            .await
            .unwrap();
        assert!(cache.should_skip("owner/repo", 2).await);
    }

    #[tokio::test]
    async fn test_triage_difficulty_cached() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        assert!(cache.triage_difficulty("owner/repo", 99).await.is_none());

        let entry = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 99,
            triage_difficulty: Some("hard".to_string()),
            status: "pre_classified".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry).await.unwrap();
        assert_eq!(
            cache.triage_difficulty("owner/repo", 99).await.as_deref(),
            Some("hard")
        );
    }

    #[tokio::test]
    async fn test_upsert_preserves_existing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        let entry1 = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 10,
            title: Some("Initial".to_string()),
            triage_difficulty: Some("medium".to_string()),
            status: "pre_classified".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry1).await.unwrap();

        // Update with new quality score but no title -- title should be preserved
        let entry2 = PrCacheEntry {
            repo: "owner/repo".to_string(),
            pr_number: 10,
            quality_score: Some(0.85),
            status: "scored".to_string(),
            ..Default::default()
        };
        cache.upsert(&entry2).await.unwrap();

        let got = cache.get("owner/repo", 10).await.unwrap();
        assert_eq!(got.title.as_deref(), Some("Initial"));
        assert_eq!(got.triage_difficulty.as_deref(), Some("medium"));
        assert_eq!(got.quality_score, Some(0.85));
        assert_eq!(got.status, "scored");
    }

    #[tokio::test]
    async fn test_stats() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache = PrCache::open(db_path.to_str().unwrap()).await.unwrap();

        for i in 0..3 {
            cache
                .upsert(&PrCacheEntry {
                    repo: "r".to_string(),
                    pr_number: i,
                    status: "discovered".to_string(),
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        cache.mark_exported("r", 0).await.unwrap();
        cache.mark_rejected("r", 1, "bad").await.unwrap();

        let s = cache.stats().await;
        assert_eq!(s.total, 3);
        assert_eq!(s.exported, 1);
        assert_eq!(s.rejected, 1);
        assert_eq!(s.discovered, 1);
    }

    #[tokio::test]
    async fn test_optional_cache_none() {
        let oc = OptionalCache::none();
        assert!(!oc.should_skip("r", 1).await);
        assert!(oc.triage_difficulty("r", 1).await.is_none());
        oc.upsert(&PrCacheEntry::default()).await.unwrap();
        oc.mark_exported("r", 1).await.unwrap();
        oc.mark_rejected("r", 1, "x").await.unwrap();
    }
}
