//! Artifact storage for large files and logs.
//!
//! This module provides file-based storage for large artifacts like
//! execution logs, generated files, and other binary data. Only metadata
//! is stored in the database; actual files are stored on the filesystem.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use super::database::Database;

/// Errors that can occur during artifact storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// IO operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(#[from] super::database::DatabaseError),

    /// SQL query failed.
    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),

    /// Artifact not found.
    #[error("Artifact not found: {0}")]
    NotFound(Uuid),

    /// Checksum verification failed.
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// Storage directory creation failed.
    #[error("Failed to create storage directory: {0}")]
    DirectoryCreationFailed(String),

    /// Invalid artifact type.
    #[error("Invalid artifact type: {0}")]
    InvalidType(String),
}

/// Storage system for large artifacts.
///
/// Artifacts are stored as files on the filesystem, organized by their
/// checksum to enable deduplication. Metadata is stored in the database.
pub struct ArtifactStorage {
    base_path: PathBuf,
    db: Arc<Database>,
}

impl ArtifactStorage {
    /// Creates a new artifact storage instance.
    ///
    /// # Arguments
    ///
    /// * `base_path` - Base directory for storing artifact files
    /// * `db` - Database client for metadata storage
    pub fn new(base_path: impl Into<PathBuf>, db: Arc<Database>) -> Self {
        Self {
            base_path: base_path.into(),
            db,
        }
    }

    /// Returns the base storage path.
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Ensures the storage directory structure exists.
    async fn ensure_directories(&self) -> Result<(), StorageError> {
        // Create base directory
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await.map_err(|e| {
                StorageError::DirectoryCreationFailed(format!(
                    "Failed to create base directory {:?}: {}",
                    self.base_path, e
                ))
            })?;
        }

        Ok(())
    }

    /// Stores an artifact and returns its unique identifier.
    ///
    /// # Arguments
    ///
    /// * `trajectory_id` - The trajectory this artifact belongs to
    /// * `artifact_type` - Type of artifact (e.g., "log", "file", "screenshot")
    /// * `data` - The artifact data to store
    ///
    /// # Returns
    ///
    /// The unique identifier for the stored artifact.
    pub async fn store(
        &self,
        trajectory_id: Uuid,
        artifact_type: &str,
        data: &[u8],
    ) -> Result<Uuid, StorageError> {
        self.ensure_directories().await?;

        let artifact_id = Uuid::new_v4();
        let checksum = Self::compute_checksum(data);
        let file_path = self.artifact_path(&checksum);
        let size_bytes = data.len() as i64;

        // Ensure the subdirectory exists (using first 2 chars of checksum)
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        // Write the file if it doesn't already exist (deduplication)
        if !file_path.exists() {
            let mut file = fs::File::create(&file_path).await?;
            file.write_all(data).await?;
            file.sync_all().await?;
        }

        // Store metadata in database
        let relative_path = file_path
            .strip_prefix(&self.base_path)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO artifacts (id, trajectory_id, artifact_type, path, size_bytes, checksum, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(artifact_id)
        .bind(trajectory_id)
        .bind(artifact_type)
        .bind(&relative_path)
        .bind(size_bytes)
        .bind(&checksum)
        .bind(Utc::now())
        .execute(self.db.pool())
        .await?;

        Ok(artifact_id)
    }

    /// Retrieves an artifact by its ID.
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The unique identifier of the artifact
    ///
    /// # Returns
    ///
    /// The artifact data as a byte vector.
    pub async fn retrieve(&self, artifact_id: Uuid) -> Result<Vec<u8>, StorageError> {
        // Get metadata from database
        let row = sqlx::query(
            r#"
            SELECT path, checksum
            FROM artifacts
            WHERE id = $1
            "#,
        )
        .bind(artifact_id)
        .fetch_optional(self.db.pool())
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Err(StorageError::NotFound(artifact_id)),
        };

        let relative_path: String = sqlx::Row::get(&row, "path");
        let expected_checksum: String = sqlx::Row::get(&row, "checksum");

        // Read the file
        let file_path = self.base_path.join(&relative_path);
        let mut file = fs::File::open(&file_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(artifact_id)
            } else {
                StorageError::Io(e)
            }
        })?;

        let mut data = Vec::new();
        file.read_to_end(&mut data).await?;

        // Verify checksum
        let actual_checksum = Self::compute_checksum(&data);
        if actual_checksum != expected_checksum {
            return Err(StorageError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        Ok(data)
    }

    /// Deletes an artifact.
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The unique identifier of the artifact to delete
    ///
    /// Note: The underlying file is only deleted if no other artifacts
    /// reference it (deduplication).
    pub async fn delete(&self, artifact_id: Uuid) -> Result<(), StorageError> {
        // Get the artifact metadata
        let row = sqlx::query(
            r#"
            SELECT path, checksum
            FROM artifacts
            WHERE id = $1
            "#,
        )
        .bind(artifact_id)
        .fetch_optional(self.db.pool())
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Err(StorageError::NotFound(artifact_id)),
        };

        let relative_path: String = sqlx::Row::get(&row, "path");
        let checksum: String = sqlx::Row::get(&row, "checksum");

        // Delete from database
        sqlx::query("DELETE FROM artifacts WHERE id = $1")
            .bind(artifact_id)
            .execute(self.db.pool())
            .await?;

        // Check if any other artifacts reference the same file
        let count_row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM artifacts
            WHERE checksum = $1
            "#,
        )
        .bind(&checksum)
        .fetch_one(self.db.pool())
        .await?;

        let remaining_refs: i64 = sqlx::Row::get(&count_row, "count");

        // Only delete the file if no other artifacts reference it
        if remaining_refs == 0 {
            let file_path = self.base_path.join(&relative_path);
            if file_path.exists() {
                fs::remove_file(&file_path).await?;

                // Clean up empty directories
                if let Some(parent) = file_path.parent() {
                    let _ = fs::remove_dir(parent).await; // Ignore errors (may not be empty)
                }
            }
        }

        Ok(())
    }

    /// Lists all artifacts for a trajectory.
    ///
    /// # Arguments
    ///
    /// * `trajectory_id` - The trajectory to list artifacts for
    ///
    /// # Returns
    ///
    /// A vector of artifact metadata.
    pub async fn list_for_trajectory(
        &self,
        trajectory_id: Uuid,
    ) -> Result<Vec<ArtifactMeta>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT id, artifact_type, size_bytes, checksum, created_at
            FROM artifacts
            WHERE trajectory_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(trajectory_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut artifacts = Vec::with_capacity(rows.len());
        for row in rows {
            artifacts.push(ArtifactMeta {
                id: sqlx::Row::get(&row, "id"),
                artifact_type: sqlx::Row::get(&row, "artifact_type"),
                size_bytes: sqlx::Row::get::<i64, _>(&row, "size_bytes") as u64,
                checksum: sqlx::Row::get(&row, "checksum"),
                created_at: sqlx::Row::get(&row, "created_at"),
            });
        }

        Ok(artifacts)
    }

    /// Computes the SHA-256 checksum of data.
    fn compute_checksum(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Returns the file path for an artifact based on its checksum.
    ///
    /// Uses the first 2 characters of the checksum as a subdirectory
    /// to avoid having too many files in a single directory.
    fn artifact_path(&self, checksum: &str) -> PathBuf {
        let subdir = &checksum[0..2.min(checksum.len())];
        self.base_path.join(subdir).join(checksum)
    }

    /// Gets the total storage size used by artifacts.
    pub async fn total_storage_size(&self) -> Result<u64, StorageError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(size_bytes), 0) as total
            FROM artifacts
            "#,
        )
        .fetch_one(self.db.pool())
        .await?;

        let total: i64 = sqlx::Row::get(&row, "total");
        Ok(total as u64)
    }

    /// Cleans up orphaned files (files on disk with no database reference).
    ///
    /// This can happen if the database is restored from a backup or
    /// if artifacts were partially created.
    pub async fn cleanup_orphans(&self) -> Result<usize, StorageError> {
        self.ensure_directories().await?;

        let mut cleaned = 0;
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                // Check subdirectory
                let mut sub_entries = fs::read_dir(&path).await?;
                while let Some(sub_entry) = sub_entries.next_entry().await? {
                    let file_path = sub_entry.path();
                    if file_path.is_file() {
                        if let Some(checksum) = file_path.file_name().and_then(|n| n.to_str()) {
                            // Check if any artifact references this checksum
                            let count_row = sqlx::query(
                                "SELECT COUNT(*) as count FROM artifacts WHERE checksum = $1",
                            )
                            .bind(checksum)
                            .fetch_one(self.db.pool())
                            .await?;

                            let refs: i64 = sqlx::Row::get(&count_row, "count");
                            if refs == 0 {
                                fs::remove_file(&file_path).await?;
                                cleaned += 1;
                            }
                        }
                    }
                }

                // Try to remove empty directories
                let _ = fs::remove_dir(&path).await;
            }
        }

        Ok(cleaned)
    }
}

/// Metadata about a stored artifact.
#[derive(Debug, Clone)]
pub struct ArtifactMeta {
    /// Unique identifier.
    pub id: Uuid,
    /// Type of artifact.
    pub artifact_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// SHA-256 checksum.
    pub checksum: String,
    /// When the artifact was created.
    pub created_at: DateTime<Utc>,
}

/// Common artifact types.
pub mod artifact_types {
    /// Execution log output.
    pub const LOG: &str = "log";
    /// Generated source file.
    pub const SOURCE_FILE: &str = "source_file";
    /// Screenshot or image.
    pub const SCREENSHOT: &str = "screenshot";
    /// Binary output.
    pub const BINARY: &str = "binary";
    /// Test results.
    pub const TEST_RESULTS: &str = "test_results";
    /// Configuration file.
    pub const CONFIG: &str = "config";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_checksum() {
        let data = b"Hello, World!";
        let checksum = ArtifactStorage::compute_checksum(data);

        // SHA-256 should produce a 64-character hex string
        assert_eq!(checksum.len(), 64);

        // Same data should produce same checksum
        let checksum2 = ArtifactStorage::compute_checksum(data);
        assert_eq!(checksum, checksum2);

        // Different data should produce different checksum
        let different_checksum = ArtifactStorage::compute_checksum(b"Different data");
        assert_ne!(checksum, different_checksum);
    }

    #[test]
    fn test_artifact_meta() {
        let meta = ArtifactMeta {
            id: Uuid::new_v4(),
            artifact_type: "log".to_string(),
            size_bytes: 1024,
            checksum: "abc123".to_string(),
            created_at: Utc::now(),
        };

        assert_eq!(meta.artifact_type, "log");
        assert_eq!(meta.size_bytes, 1024);
    }

    #[test]
    fn test_artifact_types() {
        assert_eq!(artifact_types::LOG, "log");
        assert_eq!(artifact_types::SOURCE_FILE, "source_file");
        assert_eq!(artifact_types::SCREENSHOT, "screenshot");
        assert_eq!(artifact_types::BINARY, "binary");
        assert_eq!(artifact_types::TEST_RESULTS, "test_results");
        assert_eq!(artifact_types::CONFIG, "config");
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::NotFound(Uuid::new_v4());
        assert!(err.to_string().contains("not found"));

        let err = StorageError::ChecksumMismatch {
            expected: "abc".to_string(),
            actual: "xyz".to_string(),
        };
        assert!(err.to_string().contains("abc"));
        assert!(err.to_string().contains("xyz"));
    }
}
