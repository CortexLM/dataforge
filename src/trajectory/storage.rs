//! Storage for trajectory data.
//!
//! This module provides local file-based storage for trajectories,
//! allowing them to be saved and loaded for later analysis or training.

use std::path::PathBuf;

use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use super::types::Trajectory;

/// Errors that can occur during trajectory storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Failed to read or write to the filesystem.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to serialize or deserialize trajectory data.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Trajectory with the specified ID was not found.
    #[error("Trajectory not found: {0}")]
    NotFound(Uuid),

    /// The storage directory could not be created.
    #[error("Failed to create storage directory: {0}")]
    DirectoryCreationFailed(String),

    /// The trajectory file is corrupted or invalid.
    #[error("Invalid trajectory data: {0}")]
    InvalidData(String),
}

/// Local file storage for trajectories.
///
/// Trajectories are stored as JSON files in a specified directory,
/// with filenames based on the trajectory's UUID.
pub struct TrajectoryStorage {
    /// Base path for storing trajectory files.
    base_path: PathBuf,
}

impl TrajectoryStorage {
    /// Creates a new trajectory storage instance.
    ///
    /// # Arguments
    ///
    /// * `base_path` - Directory where trajectories will be stored
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Ensures the storage directory exists.
    async fn ensure_directory(&self) -> Result<(), StorageError> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await.map_err(|e| {
                StorageError::DirectoryCreationFailed(format!(
                    "Failed to create directory {:?}: {}",
                    self.base_path, e
                ))
            })?;
        }
        Ok(())
    }

    /// Saves a trajectory to storage.
    ///
    /// # Arguments
    ///
    /// * `trajectory` - The trajectory to save
    ///
    /// # Returns
    ///
    /// The path where the trajectory was saved.
    pub async fn save(&self, trajectory: &Trajectory) -> Result<PathBuf, StorageError> {
        self.ensure_directory().await?;

        let path = self.trajectory_path(&trajectory.id);

        // Serialize to pretty JSON for readability
        let json = serde_json::to_string_pretty(trajectory)?;

        let mut file = fs::File::create(&path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;

        Ok(path)
    }

    /// Loads a trajectory from storage.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the trajectory to load
    ///
    /// # Returns
    ///
    /// The loaded trajectory.
    pub async fn load(&self, id: &Uuid) -> Result<Trajectory, StorageError> {
        let path = self.trajectory_path(id);

        if !path.exists() {
            return Err(StorageError::NotFound(*id));
        }

        let mut file = fs::File::open(&path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        let trajectory: Trajectory = serde_json::from_str(&contents)?;

        // Validate that the loaded ID matches
        if trajectory.id != *id {
            return Err(StorageError::InvalidData(format!(
                "Trajectory ID mismatch: expected {}, got {}",
                id, trajectory.id
            )));
        }

        Ok(trajectory)
    }

    /// Lists all trajectory IDs in storage.
    ///
    /// # Returns
    ///
    /// A vector of UUIDs for all stored trajectories.
    pub async fn list(&self) -> Result<Vec<Uuid>, StorageError> {
        self.ensure_directory().await?;

        let mut trajectories = Vec::new();
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Only consider .json files
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Extract UUID from filename
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(id) = Uuid::parse_str(stem) {
                    trajectories.push(id);
                }
            }
        }

        // Sort by UUID for consistent ordering
        trajectories.sort();

        Ok(trajectories)
    }

    /// Returns the file path for a trajectory.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the trajectory
    ///
    /// # Returns
    ///
    /// The path where the trajectory would be stored.
    pub fn trajectory_path(&self, id: &Uuid) -> PathBuf {
        self.base_path.join(format!("{}.json", id))
    }

    /// Deletes a trajectory from storage.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the trajectory to delete
    pub async fn delete(&self, id: &Uuid) -> Result<(), StorageError> {
        let path = self.trajectory_path(id);

        if !path.exists() {
            return Err(StorageError::NotFound(*id));
        }

        fs::remove_file(&path).await?;
        Ok(())
    }

    /// Checks if a trajectory exists in storage.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the trajectory to check
    ///
    /// # Returns
    ///
    /// True if the trajectory exists, false otherwise.
    pub fn exists(&self, id: &Uuid) -> bool {
        self.trajectory_path(id).exists()
    }

    /// Returns the base storage path.
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Counts the number of stored trajectories.
    ///
    /// This is more efficient than `list().len()` as it doesn't parse UUIDs.
    pub async fn count(&self) -> Result<usize, StorageError> {
        self.ensure_directory().await?;

        let mut count = 0;
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Loads multiple trajectories by their IDs.
    ///
    /// # Arguments
    ///
    /// * `ids` - The UUIDs of the trajectories to load
    ///
    /// # Returns
    ///
    /// A vector of loaded trajectories. Missing trajectories are skipped.
    pub async fn load_many(&self, ids: &[Uuid]) -> Result<Vec<Trajectory>, StorageError> {
        let mut trajectories = Vec::with_capacity(ids.len());

        for id in ids {
            match self.load(id).await {
                Ok(trajectory) => trajectories.push(trajectory),
                Err(StorageError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(trajectories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::types::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_trajectory() -> Trajectory {
        Trajectory {
            id: Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "gpt-4".to_string(),
            scaffold_type: "react".to_string(),
            steps: vec![TrajectoryStep {
                step_number: 0,
                state: EnvironmentState::default(),
                action: AgentAction::default(),
                observation: Observation::default(),
                reward: 0.5,
                done: true,
                timestamp: Utc::now(),
            }],
            final_result: TaskResult::Success { score: 1.0 },
            total_reward: 0.5,
            created_at: Utc::now(),
            duration_seconds: 60,
            token_usage: TokenUsage::new(100, 50),
        }
    }

    #[tokio::test]
    async fn test_storage_new() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());
        assert_eq!(storage.base_path(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let trajectory = create_test_trajectory();
        let id = trajectory.id;

        // Save
        let saved_path = storage
            .save(&trajectory)
            .await
            .expect("Save should succeed");
        assert!(saved_path.exists());

        // Load
        let loaded = storage.load(&id).await.expect("Load should succeed");
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.task_id, trajectory.task_id);
        assert_eq!(loaded.model, trajectory.model);
    }

    #[tokio::test]
    async fn test_load_not_found() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let id = Uuid::new_v4();
        let result = storage.load(&id).await;

        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_list() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        // Save multiple trajectories
        let mut ids = Vec::new();
        for _ in 0..3 {
            let trajectory = create_test_trajectory();
            ids.push(trajectory.id);
            storage
                .save(&trajectory)
                .await
                .expect("Save should succeed");
        }

        // List
        let listed = storage.list().await.expect("List should succeed");
        assert_eq!(listed.len(), 3);

        for id in ids {
            assert!(listed.contains(&id));
        }
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let trajectory = create_test_trajectory();
        let id = trajectory.id;

        storage
            .save(&trajectory)
            .await
            .expect("Save should succeed");
        assert!(storage.exists(&id));

        storage.delete(&id).await.expect("Delete should succeed");
        assert!(!storage.exists(&id));
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let id = Uuid::new_v4();
        let result = storage.delete(&id).await;

        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let trajectory = create_test_trajectory();
        let id = trajectory.id;

        assert!(!storage.exists(&id));

        storage
            .save(&trajectory)
            .await
            .expect("Save should succeed");

        assert!(storage.exists(&id));
    }

    #[tokio::test]
    async fn test_count() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        assert_eq!(storage.count().await.expect("Count should succeed"), 0);

        for _ in 0..5 {
            let trajectory = create_test_trajectory();
            storage
                .save(&trajectory)
                .await
                .expect("Save should succeed");
        }

        assert_eq!(storage.count().await.expect("Count should succeed"), 5);
    }

    #[tokio::test]
    async fn test_load_many() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let mut ids = Vec::new();
        for _ in 0..3 {
            let trajectory = create_test_trajectory();
            ids.push(trajectory.id);
            storage
                .save(&trajectory)
                .await
                .expect("Save should succeed");
        }

        // Add a non-existent ID
        ids.push(Uuid::new_v4());

        let loaded = storage
            .load_many(&ids)
            .await
            .expect("Load many should succeed");
        assert_eq!(loaded.len(), 3); // Should skip the non-existent one
    }

    #[tokio::test]
    async fn test_trajectory_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TrajectoryStorage::new(temp_dir.path());

        let id = Uuid::new_v4();
        let path = storage.trajectory_path(&id);

        assert!(path.starts_with(temp_dir.path()));
        assert!(path.to_string_lossy().ends_with(".json"));
        assert!(path.to_string_lossy().contains(&id.to_string()));
    }

    #[tokio::test]
    async fn test_creates_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let nested_path = temp_dir.path().join("nested").join("path");

        assert!(!nested_path.exists());

        let storage = TrajectoryStorage::new(&nested_path);
        let trajectory = create_test_trajectory();

        storage
            .save(&trajectory)
            .await
            .expect("Save should succeed");

        assert!(nested_path.exists());
    }
}
