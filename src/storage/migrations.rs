//! Database migration helpers.
//!
//! This module provides utilities for running database migrations
//! and managing schema versions.

use sqlx::PgPool;
use thiserror::Error;

use super::schema;

/// Errors that can occur during migration operations.
#[derive(Debug, Error)]
pub enum MigrationError {
    /// Database query failed.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Migration script failed to execute.
    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    /// Invalid migration state detected.
    #[error("Invalid migration state: {0}")]
    InvalidState(String),
}

/// Migration runner for applying schema changes.
pub struct MigrationRunner {
    pool: PgPool,
}

impl MigrationRunner {
    /// Creates a new migration runner.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Runs all pending migrations.
    ///
    /// This method is idempotent - running it multiple times will not
    /// cause errors or duplicate schema objects due to IF NOT EXISTS clauses.
    pub async fn run_migrations(&self) -> Result<(), MigrationError> {
        self.ensure_migrations_table().await?;

        for (idx, statement) in schema::all_schema_statements().iter().enumerate() {
            let migration_name = format!("schema_v1_part_{}", idx);

            if !self.is_migration_applied(&migration_name).await? {
                self.apply_migration(&migration_name, statement).await?;
            }
        }

        Ok(())
    }

    /// Ensures the migrations tracking table exists.
    async fn ensure_migrations_table(&self) -> Result<(), MigrationError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _migrations (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL UNIQUE,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Checks if a migration has already been applied.
    async fn is_migration_applied(&self, name: &str) -> Result<bool, MigrationError> {
        let result: Option<(i32,)> = sqlx::query_as("SELECT id FROM _migrations WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.is_some())
    }

    /// Applies a single migration.
    async fn apply_migration(&self, name: &str, sql: &str) -> Result<(), MigrationError> {
        // Start a transaction
        let mut tx = self.pool.begin().await?;

        // Execute the migration SQL
        sqlx::query(sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| MigrationError::MigrationFailed(format!("{}: {}", name, e)))?;

        // Record the migration
        sqlx::query("INSERT INTO _migrations (name) VALUES ($1)")
            .bind(name)
            .execute(&mut *tx)
            .await?;

        // Commit the transaction
        tx.commit().await?;

        Ok(())
    }

    /// Returns a list of applied migrations.
    pub async fn list_applied_migrations(&self) -> Result<Vec<AppliedMigration>, MigrationError> {
        self.ensure_migrations_table().await?;

        let migrations: Vec<AppliedMigration> =
            sqlx::query_as("SELECT name, applied_at FROM _migrations ORDER BY applied_at")
                .fetch_all(&self.pool)
                .await?;

        Ok(migrations)
    }

    /// Resets the database by dropping all tables.
    ///
    /// **WARNING**: This will destroy all data! Use only in development/testing.
    pub async fn reset_database(&self) -> Result<(), MigrationError> {
        // Drop tables in reverse order of creation (due to foreign key constraints)
        let drop_statements = [
            "DROP TABLE IF EXISTS artifacts CASCADE",
            "DROP TABLE IF EXISTS quality_scores CASCADE",
            "DROP TABLE IF EXISTS cost_records CASCADE",
            "DROP TABLE IF EXISTS trajectory_steps CASCADE",
            "DROP TABLE IF EXISTS trajectories CASCADE",
            "DROP TABLE IF EXISTS _migrations CASCADE",
        ];

        for statement in drop_statements {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|e| MigrationError::MigrationFailed(format!("Drop failed: {}", e)))?;
        }

        Ok(())
    }
}

/// Record of an applied migration.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppliedMigration {
    /// Name of the migration.
    pub name: String,
    /// When the migration was applied.
    pub applied_at: chrono::DateTime<chrono::Utc>,
}

/// Runs migrations using sqlx's built-in migration system.
///
/// This function uses the migrations in the `migrations/` directory.
pub async fn run_sqlx_migrations(pool: &PgPool) -> Result<(), MigrationError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| MigrationError::MigrationFailed(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_error_display() {
        let err = MigrationError::MigrationFailed("test error".to_string());
        assert!(err.to_string().contains("test error"));

        let err = MigrationError::InvalidState("bad state".to_string());
        assert!(err.to_string().contains("bad state"));
    }
}
