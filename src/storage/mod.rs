//! PostgreSQL persistent storage system.
//!
//! This module provides database-backed storage for trajectories, cost tracking,
//! quality scores, and large artifacts.
//!
//! # Overview
//!
//! The storage system consists of:
//! - **Database**: PostgreSQL client for structured data (trajectories, costs, quality scores)
//! - **Artifacts**: File-based storage for large data (logs, files, screenshots)
//! - **Migrations**: Schema management and versioning
//!
//! # Usage
//!
//! ```rust,ignore
//! use synth_bench::storage::{Database, ArtifactStorage, TrajectoryFilter};
//! use std::sync::Arc;
//!
//! // Connect to database
//! let db = Database::connect("postgres://user:pass@localhost/synth_bench").await?;
//!
//! // Run migrations
//! db.run_migrations().await?;
//!
//! // Save a trajectory
//! db.save_trajectory(&trajectory).await?;
//!
//! // Query trajectories
//! let filter = TrajectoryFilter::new()
//!     .with_model("gpt-4")
//!     .with_min_reward(0.8)
//!     .with_limit(10);
//! let trajectories = db.list_trajectories(&filter).await?;
//!
//! // Store artifacts
//! let artifact_storage = ArtifactStorage::new("/path/to/artifacts", Arc::new(db));
//! let artifact_id = artifact_storage.store(trajectory_id, "log", log_data).await?;
//! ```

pub mod artifacts;
pub mod database;
pub mod migrations;
pub mod schema;

// Re-export main types for convenience
pub use artifacts::{artifact_types, ArtifactMeta, ArtifactStorage, StorageError};
pub use database::{
    CostRecord, Database, DatabaseError, QualityScore, TrajectoryFilter, TrajectoryMeta,
};
pub use migrations::{AppliedMigration, MigrationError, MigrationRunner};
