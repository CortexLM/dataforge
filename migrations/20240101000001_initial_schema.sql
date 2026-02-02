-- Trajectories table
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
);

-- Trajectory steps table
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
);

-- Cost tracking table
CREATE TABLE IF NOT EXISTS cost_records (
    id SERIAL PRIMARY KEY,
    model VARCHAR(255) NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_cents BIGINT NOT NULL,
    task_id VARCHAR(255),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Quality scores table
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
);

-- Artifacts table (metadata only, actual files stored separately)
CREATE TABLE IF NOT EXISTS artifacts (
    id UUID PRIMARY KEY,
    trajectory_id UUID REFERENCES trajectories(id) ON DELETE SET NULL,
    artifact_type VARCHAR(50) NOT NULL,
    path VARCHAR(1024) NOT NULL,
    size_bytes BIGINT NOT NULL,
    checksum VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_trajectories_task_id ON trajectories(task_id);
CREATE INDEX IF NOT EXISTS idx_trajectories_model ON trajectories(model);
CREATE INDEX IF NOT EXISTS idx_trajectories_created_at ON trajectories(created_at);
CREATE INDEX IF NOT EXISTS idx_trajectory_steps_trajectory_id ON trajectory_steps(trajectory_id);
CREATE INDEX IF NOT EXISTS idx_cost_records_model ON cost_records(model);
CREATE INDEX IF NOT EXISTS idx_cost_records_recorded_at ON cost_records(recorded_at);
CREATE INDEX IF NOT EXISTS idx_quality_scores_trajectory_id ON quality_scores(trajectory_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_trajectory_id ON artifacts(trajectory_id);
