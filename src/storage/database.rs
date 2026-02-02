//! PostgreSQL database client for persistent storage.
//!
//! This module provides a database client that handles trajectory storage,
//! cost tracking, and quality scores using PostgreSQL with sqlx.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use crate::trajectory::{
    AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, Trajectory, TrajectoryStep,
};

use super::migrations::MigrationRunner;

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// Connection to the database failed.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Query execution failed.
    #[error("Query failed: {0}")]
    QueryFailed(#[from] sqlx::Error),

    /// Record not found.
    #[error("Record not found: {0}")]
    NotFound(String),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Migration error.
    #[error("Migration error: {0}")]
    Migration(#[from] super::migrations::MigrationError),

    /// Transaction error.
    #[error("Transaction error: {0}")]
    Transaction(String),
}

/// PostgreSQL database client.
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Connects to the database and returns a new client.
    ///
    /// # Arguments
    ///
    /// * `database_url` - PostgreSQL connection string (e.g., "postgres://user:pass@localhost/db")
    ///
    /// # Returns
    ///
    /// A new `Database` instance connected to the specified database.
    pub async fn connect(database_url: &str) -> Result<Self, DatabaseError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(database_url)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Self { pool })
    }

    /// Creates a new database client from an existing pool.
    ///
    /// # Arguments
    ///
    /// * `pool` - Existing PostgreSQL connection pool
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns a reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Runs database migrations.
    pub async fn run_migrations(&self) -> Result<(), DatabaseError> {
        let runner = MigrationRunner::new(self.pool.clone());
        runner.run_migrations().await?;
        Ok(())
    }

    // =========================================================================
    // Trajectory Operations
    // =========================================================================

    /// Saves a complete trajectory with all its steps.
    ///
    /// This operation is transactional - either all data is saved or none.
    pub async fn save_trajectory(&self, trajectory: &Trajectory) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await?;

        // Insert the main trajectory record
        let final_result_json = serde_json::to_value(&trajectory.final_result)?;
        let token_usage_json = serde_json::to_value(&trajectory.token_usage)?;

        sqlx::query(
            r#"
            INSERT INTO trajectories (
                id, task_id, model, scaffold_type, total_reward,
                final_result, duration_seconds, token_usage, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                task_id = EXCLUDED.task_id,
                model = EXCLUDED.model,
                scaffold_type = EXCLUDED.scaffold_type,
                total_reward = EXCLUDED.total_reward,
                final_result = EXCLUDED.final_result,
                duration_seconds = EXCLUDED.duration_seconds,
                token_usage = EXCLUDED.token_usage,
                updated_at = NOW()
            "#,
        )
        .bind(trajectory.id)
        .bind(&trajectory.task_id)
        .bind(&trajectory.model)
        .bind(&trajectory.scaffold_type)
        .bind(trajectory.total_reward)
        .bind(&final_result_json)
        .bind(trajectory.duration_seconds as i64)
        .bind(&token_usage_json)
        .bind(trajectory.created_at)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;

        // Delete existing steps for this trajectory (for upsert behavior)
        sqlx::query("DELETE FROM trajectory_steps WHERE trajectory_id = $1")
            .bind(trajectory.id)
            .execute(&mut *tx)
            .await?;

        // Insert all steps
        for step in &trajectory.steps {
            let state_json = serde_json::to_value(&step.state)?;
            let action_json = serde_json::to_value(&step.action)?;
            let observation_json = serde_json::to_value(&step.observation)?;

            sqlx::query(
                r#"
                INSERT INTO trajectory_steps (
                    trajectory_id, step_number, state, action, observation, reward, done, timestamp
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(trajectory.id)
            .bind(step.step_number as i32)
            .bind(&state_json)
            .bind(&action_json)
            .bind(&observation_json)
            .bind(step.reward)
            .bind(step.done)
            .bind(step.timestamp)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Retrieves a trajectory by its ID.
    ///
    /// Returns `None` if the trajectory doesn't exist.
    pub async fn get_trajectory(&self, id: Uuid) -> Result<Option<Trajectory>, DatabaseError> {
        // Fetch the main trajectory record
        let row = sqlx::query(
            r#"
            SELECT id, task_id, model, scaffold_type, total_reward,
                   final_result, duration_seconds, token_usage, created_at
            FROM trajectories
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        // Parse the trajectory fields
        let trajectory_id: Uuid = row.get("id");
        let task_id: String = row.get("task_id");
        let model: String = row.get("model");
        let scaffold_type: String = row.get("scaffold_type");
        let total_reward: f64 = row.get("total_reward");
        let final_result_json: serde_json::Value = row.get("final_result");
        let duration_seconds: i64 = row.get("duration_seconds");
        let token_usage_json: serde_json::Value = row.get("token_usage");
        let created_at: DateTime<Utc> = row.get("created_at");

        let final_result: TaskResult = serde_json::from_value(final_result_json)?;
        let token_usage: TokenUsage = serde_json::from_value(token_usage_json)?;

        // Fetch all steps
        let step_rows = sqlx::query(
            r#"
            SELECT step_number, state, action, observation, reward, done, timestamp
            FROM trajectory_steps
            WHERE trajectory_id = $1
            ORDER BY step_number
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let mut steps = Vec::with_capacity(step_rows.len());
        for step_row in step_rows {
            let step_number: i32 = step_row.get("step_number");
            let state_json: serde_json::Value = step_row.get("state");
            let action_json: serde_json::Value = step_row.get("action");
            let observation_json: serde_json::Value = step_row.get("observation");
            let reward: f64 = step_row.get("reward");
            let done: bool = step_row.get("done");
            let timestamp: DateTime<Utc> = step_row.get("timestamp");

            let state: EnvironmentState = serde_json::from_value(state_json)?;
            let action: AgentAction = serde_json::from_value(action_json)?;
            let observation: Observation = serde_json::from_value(observation_json)?;

            steps.push(TrajectoryStep {
                step_number: step_number as u32,
                state,
                action,
                observation,
                reward,
                done,
                timestamp,
            });
        }

        Ok(Some(Trajectory {
            id: trajectory_id,
            task_id,
            model,
            scaffold_type,
            steps,
            final_result,
            total_reward,
            created_at,
            duration_seconds: duration_seconds as u64,
            token_usage,
        }))
    }

    /// Lists trajectories matching the given filter.
    pub async fn list_trajectories(
        &self,
        filter: &TrajectoryFilter,
    ) -> Result<Vec<TrajectoryMeta>, DatabaseError> {
        let mut query = String::from(
            r#"
            SELECT t.id, t.task_id, t.model, t.total_reward, t.created_at
            FROM trajectories t
            "#,
        );

        let mut conditions = Vec::new();
        let mut param_idx = 1;

        // Build WHERE clause dynamically
        if filter.task_id.is_some() {
            conditions.push(format!("t.task_id = ${}", param_idx));
            param_idx += 1;
        }

        if filter.model.is_some() {
            conditions.push(format!("t.model = ${}", param_idx));
            param_idx += 1;
        }

        if filter.min_reward.is_some() {
            conditions.push(format!("t.total_reward >= ${}", param_idx));
            param_idx += 1;
        }

        if filter.passed_quality.is_some() {
            query.push_str(" LEFT JOIN quality_scores qs ON t.id = qs.trajectory_id");
            conditions.push(format!("qs.passed_filter = ${}", param_idx));
            param_idx += 1;
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(" ORDER BY t.created_at DESC");

        if filter.limit.is_some() {
            query.push_str(&format!(" LIMIT ${}", param_idx));
            param_idx += 1;

            if filter.offset.is_some() {
                query.push_str(&format!(" OFFSET ${}", param_idx));
            }
        }

        // Build the query with bindings
        let mut sqlx_query = sqlx::query(&query);

        if let Some(ref task_id) = filter.task_id {
            sqlx_query = sqlx_query.bind(task_id);
        }

        if let Some(ref model) = filter.model {
            sqlx_query = sqlx_query.bind(model);
        }

        if let Some(min_reward) = filter.min_reward {
            sqlx_query = sqlx_query.bind(min_reward);
        }

        if let Some(passed) = filter.passed_quality {
            sqlx_query = sqlx_query.bind(passed);
        }

        if let Some(limit) = filter.limit {
            sqlx_query = sqlx_query.bind(limit);

            if let Some(offset) = filter.offset {
                sqlx_query = sqlx_query.bind(offset);
            }
        }

        let rows = sqlx_query.fetch_all(&self.pool).await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(TrajectoryMeta {
                id: row.get("id"),
                task_id: row.get("task_id"),
                model: row.get("model"),
                total_reward: row.get("total_reward"),
                created_at: row.get("created_at"),
            });
        }

        Ok(results)
    }

    /// Deletes a trajectory and all associated data.
    pub async fn delete_trajectory(&self, id: Uuid) -> Result<(), DatabaseError> {
        let result = sqlx::query("DELETE FROM trajectories WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DatabaseError::NotFound(format!("Trajectory {}", id)));
        }

        Ok(())
    }

    // =========================================================================
    // Cost Operations
    // =========================================================================

    /// Records a cost entry.
    pub async fn record_cost(&self, record: &CostRecord) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO cost_records (model, input_tokens, output_tokens, cost_cents, task_id, recorded_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&record.model)
        .bind(record.input_tokens)
        .bind(record.output_tokens)
        .bind(record.cost_cents)
        .bind(&record.task_id)
        .bind(record.recorded_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Gets the total cost for a specific date (in cents).
    pub async fn get_daily_cost(&self, date: NaiveDate) -> Result<f64, DatabaseError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0) as total
            FROM cost_records
            WHERE DATE(recorded_at) = $1
            "#,
        )
        .bind(date)
        .fetch_one(&self.pool)
        .await?;

        let total_cents: i64 = row.get("total");
        Ok(total_cents as f64 / 100.0)
    }

    /// Gets the total cost for a specific month (in dollars).
    pub async fn get_monthly_cost(&self, year: i32, month: u32) -> Result<f64, DatabaseError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(cost_cents), 0) as total
            FROM cost_records
            WHERE EXTRACT(YEAR FROM recorded_at) = $1
              AND EXTRACT(MONTH FROM recorded_at) = $2
            "#,
        )
        .bind(year)
        .bind(month as i32)
        .fetch_one(&self.pool)
        .await?;

        let total_cents: i64 = row.get("total");
        Ok(total_cents as f64 / 100.0)
    }

    // =========================================================================
    // Quality Operations
    // =========================================================================

    /// Saves a quality score for a trajectory.
    pub async fn save_quality_score(&self, score: &QualityScore) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO quality_scores (
                trajectory_id, correctness_score, coherence_score,
                completeness_score, overall_score, passed_filter, reviewed_at, reviewer
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (trajectory_id) DO UPDATE SET
                correctness_score = EXCLUDED.correctness_score,
                coherence_score = EXCLUDED.coherence_score,
                completeness_score = EXCLUDED.completeness_score,
                overall_score = EXCLUDED.overall_score,
                passed_filter = EXCLUDED.passed_filter,
                reviewed_at = EXCLUDED.reviewed_at,
                reviewer = EXCLUDED.reviewer
            "#,
        )
        .bind(score.trajectory_id)
        .bind(score.correctness_score)
        .bind(score.coherence_score)
        .bind(score.completeness_score)
        .bind(score.overall_score)
        .bind(score.passed_filter)
        .bind(score.reviewed_at)
        .bind(&score.reviewer)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Gets the quality score for a trajectory.
    pub async fn get_quality_score(
        &self,
        trajectory_id: Uuid,
    ) -> Result<Option<QualityScore>, DatabaseError> {
        let row = sqlx::query(
            r#"
            SELECT trajectory_id, correctness_score, coherence_score,
                   completeness_score, overall_score, passed_filter, reviewed_at, reviewer
            FROM quality_scores
            WHERE trajectory_id = $1
            "#,
        )
        .bind(trajectory_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(QualityScore {
                trajectory_id: r.get("trajectory_id"),
                correctness_score: r.get("correctness_score"),
                coherence_score: r.get("coherence_score"),
                completeness_score: r.get("completeness_score"),
                overall_score: r.get("overall_score"),
                passed_filter: r.get("passed_filter"),
                reviewed_at: r.get("reviewed_at"),
                reviewer: r.get("reviewer"),
            })),
            None => Ok(None),
        }
    }
}

/// Filter criteria for listing trajectories.
#[derive(Debug, Default, Clone)]
pub struct TrajectoryFilter {
    /// Filter by task ID.
    pub task_id: Option<String>,
    /// Filter by model name.
    pub model: Option<String>,
    /// Filter by minimum reward.
    pub min_reward: Option<f64>,
    /// Filter by quality filter pass status.
    pub passed_quality: Option<bool>,
    /// Maximum number of results.
    pub limit: Option<i64>,
    /// Offset for pagination.
    pub offset: Option<i64>,
}

impl TrajectoryFilter {
    /// Creates a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the task ID filter.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Sets the model filter.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the minimum reward filter.
    pub fn with_min_reward(mut self, min_reward: f64) -> Self {
        self.min_reward = Some(min_reward);
        self
    }

    /// Sets the passed quality filter.
    pub fn with_passed_quality(mut self, passed: bool) -> Self {
        self.passed_quality = Some(passed);
        self
    }

    /// Sets the result limit.
    pub fn with_limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the offset for pagination.
    pub fn with_offset(mut self, offset: i64) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Metadata about a trajectory (without full step data).
#[derive(Debug, Clone)]
pub struct TrajectoryMeta {
    /// Unique identifier.
    pub id: Uuid,
    /// Task ID.
    pub task_id: String,
    /// Model used.
    pub model: String,
    /// Total reward achieved.
    pub total_reward: f64,
    /// When the trajectory was created.
    pub created_at: DateTime<Utc>,
}

/// Cost record for tracking API usage.
#[derive(Debug, Clone)]
pub struct CostRecord {
    /// Model name.
    pub model: String,
    /// Number of input tokens.
    pub input_tokens: i32,
    /// Number of output tokens.
    pub output_tokens: i32,
    /// Cost in cents.
    pub cost_cents: i64,
    /// Optional task ID this cost is associated with.
    pub task_id: Option<String>,
    /// When the cost was recorded.
    pub recorded_at: DateTime<Utc>,
}

impl CostRecord {
    /// Creates a new cost record.
    pub fn new(
        model: impl Into<String>,
        input_tokens: i32,
        output_tokens: i32,
        cost_cents: i64,
    ) -> Self {
        Self {
            model: model.into(),
            input_tokens,
            output_tokens,
            cost_cents,
            task_id: None,
            recorded_at: Utc::now(),
        }
    }

    /// Sets the task ID for this cost record.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }
}

/// Quality score for a trajectory.
#[derive(Debug, Clone)]
pub struct QualityScore {
    /// Trajectory this score belongs to.
    pub trajectory_id: Uuid,
    /// Score for correctness (0.0-1.0).
    pub correctness_score: Option<f64>,
    /// Score for coherence (0.0-1.0).
    pub coherence_score: Option<f64>,
    /// Score for completeness (0.0-1.0).
    pub completeness_score: Option<f64>,
    /// Overall quality score (0.0-1.0).
    pub overall_score: f64,
    /// Whether this trajectory passed the quality filter.
    pub passed_filter: bool,
    /// When this score was recorded.
    pub reviewed_at: DateTime<Utc>,
    /// Who or what reviewed this trajectory.
    pub reviewer: Option<String>,
}

impl QualityScore {
    /// Creates a new quality score.
    pub fn new(trajectory_id: Uuid, overall_score: f64) -> Self {
        Self {
            trajectory_id,
            correctness_score: None,
            coherence_score: None,
            completeness_score: None,
            overall_score,
            passed_filter: false,
            reviewed_at: Utc::now(),
            reviewer: None,
        }
    }

    /// Sets the component scores.
    pub fn with_component_scores(
        mut self,
        correctness: f64,
        coherence: f64,
        completeness: f64,
    ) -> Self {
        self.correctness_score = Some(correctness);
        self.coherence_score = Some(coherence);
        self.completeness_score = Some(completeness);
        self
    }

    /// Marks this trajectory as passing the quality filter.
    pub fn passed(mut self) -> Self {
        self.passed_filter = true;
        self
    }

    /// Sets the reviewer.
    pub fn with_reviewer(mut self, reviewer: impl Into<String>) -> Self {
        self.reviewer = Some(reviewer.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trajectory_filter_builder() {
        let filter = TrajectoryFilter::new()
            .with_task_id("task-1")
            .with_model("gpt-4")
            .with_min_reward(0.5)
            .with_limit(10)
            .with_offset(20);

        assert_eq!(filter.task_id, Some("task-1".to_string()));
        assert_eq!(filter.model, Some("gpt-4".to_string()));
        assert_eq!(filter.min_reward, Some(0.5));
        assert_eq!(filter.limit, Some(10));
        assert_eq!(filter.offset, Some(20));
    }

    #[test]
    fn test_cost_record_builder() {
        let record = CostRecord::new("gpt-4", 100, 50, 150).with_task_id("task-123");

        assert_eq!(record.model, "gpt-4");
        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.cost_cents, 150);
        assert_eq!(record.task_id, Some("task-123".to_string()));
    }

    #[test]
    fn test_quality_score_builder() {
        let id = Uuid::new_v4();
        let score = QualityScore::new(id, 0.85)
            .with_component_scores(0.9, 0.8, 0.85)
            .passed()
            .with_reviewer("human-reviewer");

        assert_eq!(score.trajectory_id, id);
        assert_eq!(score.overall_score, 0.85);
        assert_eq!(score.correctness_score, Some(0.9));
        assert_eq!(score.coherence_score, Some(0.8));
        assert_eq!(score.completeness_score, Some(0.85));
        assert!(score.passed_filter);
        assert_eq!(score.reviewer, Some("human-reviewer".to_string()));
    }

    #[test]
    fn test_database_error_display() {
        let err = DatabaseError::NotFound("test-id".to_string());
        assert!(err.to_string().contains("test-id"));

        let err = DatabaseError::ConnectionFailed("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));
    }
}
