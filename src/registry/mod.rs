//! Task registry module for managing benchmark tasks.
//!
//! This module provides functionality for:
//! - Registering and tracking benchmark tasks
//! - Managing task lifecycle states
//! - Version control for the task dataset

pub mod entry;
pub mod lifecycle;
pub mod version;

pub use entry::{Calibration, Compatibility, TaskMetadata, TaskRegistryEntry, TaskStatus};
pub use lifecycle::LifecycleManager;
pub use version::{DatasetVersion, VersionIncrement, VersionPolicy, VersionRelease};

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::RegistryError;

/// Name of the registry JSON file.
const REGISTRY_FILENAME: &str = "registry.json";

/// Manages the task registry, providing storage and retrieval of task entries.
pub struct TaskRegistry {
    /// Path to the registry directory.
    registry_path: PathBuf,
    /// In-memory cache of task entries.
    entries: HashMap<String, TaskRegistryEntry>,
}

impl TaskRegistry {
    /// Create a new task registry at the specified path.
    ///
    /// # Arguments
    /// * `registry_path` - Path to the directory where the registry will be stored
    pub fn new(registry_path: PathBuf) -> Self {
        Self {
            registry_path,
            entries: HashMap::new(),
        }
    }

    /// Get the path to the registry file.
    fn registry_file_path(&self) -> PathBuf {
        self.registry_path.join(REGISTRY_FILENAME)
    }

    /// Load the registry from disk.
    ///
    /// If the registry file doesn't exist, starts with an empty registry.
    pub fn load(&mut self) -> Result<(), RegistryError> {
        let file_path = self.registry_file_path();

        if !file_path.exists() {
            self.entries = HashMap::new();
            return Ok(());
        }

        let contents = fs::read_to_string(&file_path)?;
        let entries: Vec<TaskRegistryEntry> = serde_json::from_str(&contents)?;

        self.entries = entries.into_iter().map(|e| (e.id.clone(), e)).collect();

        Ok(())
    }

    /// Save the registry to disk.
    ///
    /// Creates the registry directory if it doesn't exist.
    pub fn save(&self) -> Result<(), RegistryError> {
        // Ensure directory exists
        if !self.registry_path.exists() {
            fs::create_dir_all(&self.registry_path)?;
        }

        let entries: Vec<&TaskRegistryEntry> = self.entries.values().collect();
        let contents = serde_json::to_string_pretty(&entries)?;

        fs::write(self.registry_file_path(), contents)?;

        Ok(())
    }

    /// Register a new task entry.
    ///
    /// # Arguments
    /// * `entry` - The task entry to register
    ///
    /// # Returns
    /// The ID of the registered task on success.
    ///
    /// # Errors
    /// Returns `DuplicateTask` if a task with the same ID already exists.
    pub fn register(&mut self, entry: TaskRegistryEntry) -> Result<String, RegistryError> {
        if self.entries.contains_key(&entry.id) {
            return Err(RegistryError::DuplicateTask(entry.id));
        }

        let task_id = entry.id.clone();
        self.entries.insert(task_id.clone(), entry);

        Ok(task_id)
    }

    /// Get a task entry by ID.
    pub fn get(&self, task_id: &str) -> Option<&TaskRegistryEntry> {
        self.entries.get(task_id)
    }

    /// Update the status of a task.
    ///
    /// # Arguments
    /// * `task_id` - ID of the task to update
    /// * `status` - New status for the task
    ///
    /// # Errors
    /// Returns `TaskNotFound` if no task with the given ID exists.
    pub fn update_status(
        &mut self,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), RegistryError> {
        let entry = self
            .entries
            .get_mut(task_id)
            .ok_or_else(|| RegistryError::TaskNotFound(task_id.to_string()))?;

        entry.status = status;
        entry.metadata.updated_at = chrono::Utc::now().to_rfc3339();

        Ok(())
    }

    /// Search for tasks matching the given criteria.
    ///
    /// All criteria are optional. Tasks must match all provided criteria (AND logic).
    ///
    /// # Arguments
    /// * `category` - Filter by primary category
    /// * `difficulty` - Filter by difficulty level
    /// * `tags` - Filter by tags (task must have at least one matching tag)
    /// * `status` - Filter by lifecycle status
    pub fn search(
        &self,
        category: Option<&str>,
        difficulty: Option<&str>,
        tags: Option<&[String]>,
        status: Option<TaskStatus>,
    ) -> Vec<&TaskRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| {
                // Filter by category
                if let Some(cat) = category {
                    if entry.metadata.category != cat {
                        return false;
                    }
                }

                // Filter by difficulty
                if let Some(diff) = difficulty {
                    if entry.metadata.difficulty != diff {
                        return false;
                    }
                }

                // Filter by tags (any match)
                if let Some(search_tags) = tags {
                    let has_matching_tag = search_tags
                        .iter()
                        .any(|tag| entry.metadata.tags.contains(tag));
                    if !has_matching_tag {
                        return false;
                    }
                }

                // Filter by status
                if let Some(s) = status {
                    if entry.status != s {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Get all tasks in a specific category.
    pub fn get_by_category(&self, category: &str) -> Vec<&TaskRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| entry.metadata.category == category)
            .collect()
    }

    /// Get all tasks with a specific difficulty level.
    pub fn get_by_difficulty(&self, difficulty: &str) -> Vec<&TaskRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| entry.metadata.difficulty == difficulty)
            .collect()
    }

    /// Get the total number of registered tasks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all task entries.
    pub fn all(&self) -> Vec<&TaskRegistryEntry> {
        self.entries.values().collect()
    }

    /// Filter tasks by their lifecycle status.
    pub fn filter_by_status(&self, status: TaskStatus) -> Vec<&TaskRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| entry.status == status)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn create_test_metadata(category: &str, difficulty: &str, tags: Vec<String>) -> TaskMetadata {
        TaskMetadata {
            difficulty: difficulty.to_string(),
            difficulty_score: 0.5,
            category: category.to_string(),
            subcategory: "test".to_string(),
            tags,
            author: "test".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn create_test_entry(id: &str, category: &str, difficulty: &str) -> TaskRegistryEntry {
        TaskRegistryEntry::new(
            id.to_string(),
            "template-001".to_string(),
            42,
            create_test_metadata(category, difficulty, vec!["test".to_string()]),
        )
    }

    #[test]
    fn test_registry_new() {
        let registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_task() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let entry = create_test_entry("task-001", "debugging", "medium");

        let result = registry.register(entry);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "task-001");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_register_duplicate() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let entry1 = create_test_entry("task-001", "debugging", "medium");
        let entry2 = create_test_entry("task-001", "debugging", "hard");

        registry
            .register(entry1)
            .expect("first registration should succeed");
        let result = registry.register(entry2);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_task() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let entry = create_test_entry("task-001", "debugging", "medium");
        registry
            .register(entry)
            .expect("registration should succeed");

        let retrieved = registry.get("task-001");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "task-001");

        let missing = registry.get("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_update_status() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let entry = create_test_entry("task-001", "debugging", "medium");
        registry
            .register(entry)
            .expect("registration should succeed");

        let result = registry.update_status("task-001", TaskStatus::Deprecated);
        assert!(result.is_ok());

        let entry = registry.get("task-001").unwrap();
        assert_eq!(entry.status, TaskStatus::Deprecated);
    }

    #[test]
    fn test_update_status_not_found() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let result = registry.update_status("nonexistent", TaskStatus::Draft);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_by_category() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "refactoring", "medium"))
            .unwrap();
        registry
            .register(create_test_entry("task-003", "debugging", "hard"))
            .unwrap();

        let results = registry.search(Some("debugging"), None, None, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_difficulty() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "refactoring", "medium"))
            .unwrap();
        registry
            .register(create_test_entry("task-003", "debugging", "easy"))
            .unwrap();

        let results = registry.search(None, Some("easy"), None, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_tags() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));

        let metadata1 = create_test_metadata(
            "debugging",
            "easy",
            vec!["rust".to_string(), "panic".to_string()],
        );
        let entry1 = TaskRegistryEntry::new("task-001".to_string(), "t1".to_string(), 1, metadata1);

        let metadata2 = create_test_metadata("debugging", "easy", vec!["python".to_string()]);
        let entry2 = TaskRegistryEntry::new("task-002".to_string(), "t2".to_string(), 2, metadata2);

        registry.register(entry1).unwrap();
        registry.register(entry2).unwrap();

        let results = registry.search(None, None, Some(&["rust".to_string()]), None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "task-001");
    }

    #[test]
    fn test_search_by_status() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "debugging", "easy"))
            .unwrap();
        registry
            .update_status("task-001", TaskStatus::Deprecated)
            .unwrap();

        let results = registry.search(None, None, None, Some(TaskStatus::Deprecated));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "task-001");
    }

    #[test]
    fn test_search_combined_filters() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "debugging", "hard"))
            .unwrap();
        registry
            .register(create_test_entry("task-003", "refactoring", "easy"))
            .unwrap();

        let results = registry.search(Some("debugging"), Some("easy"), None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "task-001");
    }

    #[test]
    fn test_get_by_category() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "refactoring", "easy"))
            .unwrap();

        let results = registry.get_by_category("debugging");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_by_difficulty() {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        registry
            .register(create_test_entry("task-001", "debugging", "easy"))
            .unwrap();
        registry
            .register(create_test_entry("task-002", "debugging", "hard"))
            .unwrap();

        let results = registry.get_by_difficulty("hard");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_save_and_load() {
        let test_dir = temp_dir().join("dataforge_test_registry");
        let _ = fs::remove_dir_all(&test_dir);

        // Save
        {
            let mut registry = TaskRegistry::new(test_dir.clone());
            registry
                .register(create_test_entry("task-001", "debugging", "easy"))
                .unwrap();
            registry
                .register(create_test_entry("task-002", "refactoring", "medium"))
                .unwrap();
            registry.save().expect("save should succeed");
        }

        // Load
        {
            let mut registry = TaskRegistry::new(test_dir.clone());
            registry.load().expect("load should succeed");
            assert_eq!(registry.len(), 2);
            assert!(registry.get("task-001").is_some());
            assert!(registry.get("task-002").is_some());
        }

        // Cleanup
        let _ = fs::remove_dir_all(&test_dir);
    }
}
