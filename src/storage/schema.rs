//! Database schema constants and SQL queries.
//!
//! This module contains all SQL schema definitions and query templates
//! for the PostgreSQL storage backend.

/// SQL schema for creating the trajectories table.
pub const CREATE_TRAJECTORIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS trajectories (
    id UUID PRIMARY KEY,
    task_id VARCHAR(255) NOT NULL,
    model VARCHAR(255) NOT NULL,
    scaffold_type VARCHAR(100) NOT NULL,
    total_reward DOUBLE PRECISION NOT NULL,
    final_result JSONB NOT NULL,
    duration_seconds BIGINT NOT NULL,
    token_usage JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// SQL schema for creating the trajectory_steps table.
pub const CREATE_TRAJECTORY_STEPS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS trajectory_steps (
    id SERIAL PRIMARY KEY,
    trajectory_id UUID NOT NULL REFERENCES trajectories(id) ON DELETE CASCADE,
    step_number INTEGER NOT NULL,
    state JSONB NOT NULL,
    action JSONB NOT NULL,
    observation JSONB NOT NULL,
    reward DOUBLE PRECISION NOT NULL,
    done BOOLEAN NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    UNIQUE(trajectory_id, step_number)
)
"#;

/// SQL schema for creating the cost_records table.
pub const CREATE_COST_RECORDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS cost_records (
    id SERIAL PRIMARY KEY,
    model VARCHAR(255) NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_cents BIGINT NOT NULL,
    task_id VARCHAR(255),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// SQL schema for creating the quality_scores table.
pub const CREATE_QUALITY_SCORES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS quality_scores (
    id SERIAL PRIMARY KEY,
    trajectory_id UUID NOT NULL REFERENCES trajectories(id) ON DELETE CASCADE,
    correctness_score DOUBLE PRECISION,
    coherence_score DOUBLE PRECISION,
    completeness_score DOUBLE PRECISION,
    overall_score DOUBLE PRECISION NOT NULL,
    passed_filter BOOLEAN NOT NULL DEFAULT FALSE,
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewer VARCHAR(100)
)
"#;

/// SQL schema for creating the artifacts table.
pub const CREATE_ARTIFACTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS artifacts (
    id UUID PRIMARY KEY,
    trajectory_id UUID REFERENCES trajectories(id) ON DELETE SET NULL,
    artifact_type VARCHAR(50) NOT NULL,
    path VARCHAR(1024) NOT NULL,
    size_bytes BIGINT NOT NULL,
    checksum VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// SQL for creating all required indexes.
pub const CREATE_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_trajectories_task_id ON trajectories(task_id);
CREATE INDEX IF NOT EXISTS idx_trajectories_model ON trajectories(model);
CREATE INDEX IF NOT EXISTS idx_trajectories_created_at ON trajectories(created_at);
CREATE INDEX IF NOT EXISTS idx_trajectory_steps_trajectory_id ON trajectory_steps(trajectory_id);
CREATE INDEX IF NOT EXISTS idx_cost_records_model ON cost_records(model);
CREATE INDEX IF NOT EXISTS idx_cost_records_recorded_at ON cost_records(recorded_at);
CREATE INDEX IF NOT EXISTS idx_quality_scores_trajectory_id ON quality_scores(trajectory_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_trajectory_id ON artifacts(trajectory_id)
"#;

/// Returns all schema creation statements in the correct order.
pub fn all_schema_statements() -> Vec<&'static str> {
    vec![
        CREATE_TRAJECTORIES_TABLE,
        CREATE_TRAJECTORY_STEPS_TABLE,
        CREATE_COST_RECORDS_TABLE,
        CREATE_QUALITY_SCORES_TABLE,
        CREATE_ARTIFACTS_TABLE,
        CREATE_INDEXES,
    ]
}

/// Table names in the schema.
pub mod tables {
    /// Trajectories table name.
    pub const TRAJECTORIES: &str = "trajectories";
    /// Trajectory steps table name.
    pub const TRAJECTORY_STEPS: &str = "trajectory_steps";
    /// Cost records table name.
    pub const COST_RECORDS: &str = "cost_records";
    /// Quality scores table name.
    pub const QUALITY_SCORES: &str = "quality_scores";
    /// Artifacts table name.
    pub const ARTIFACTS: &str = "artifacts";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_schema_statements_order() {
        let statements = all_schema_statements();
        assert_eq!(statements.len(), 6);
        // Trajectories must come first (other tables reference it)
        assert!(statements[0].contains("trajectories"));
        // Indexes should be last
        assert!(statements[5].contains("CREATE INDEX"));
    }

    #[test]
    fn test_table_constants() {
        assert_eq!(tables::TRAJECTORIES, "trajectories");
        assert_eq!(tables::TRAJECTORY_STEPS, "trajectory_steps");
        assert_eq!(tables::COST_RECORDS, "cost_records");
        assert_eq!(tables::QUALITY_SCORES, "quality_scores");
        assert_eq!(tables::ARTIFACTS, "artifacts");
    }
}
