use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a task in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Draft,
    Review,
    Published,
    Deprecated,
}

/// Calibration data from human testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calibration {
    /// Whether the task has been tested by humans.
    pub human_tested: bool,
    /// Number of human testers who evaluated this task.
    pub num_testers: u32,
    /// Average time in seconds to complete the task.
    pub avg_time_seconds: f64,
    /// Success rate (0.0 - 1.0) of human testers.
    pub success_rate: f64,
    /// ISO 8601 timestamp of last calibration, if any.
    pub last_calibration: Option<String>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            human_tested: false,
            num_testers: 0,
            avg_time_seconds: 0.0,
            success_rate: 0.0,
            last_calibration: None,
        }
    }
}

/// Compatibility requirements for running a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compatibility {
    /// Minimum generator version required.
    pub min_generator_version: String,
    /// Supported Docker base images.
    pub base_images: Vec<String>,
    /// Required features or capabilities.
    pub required_features: Vec<String>,
}

impl Default for Compatibility {
    fn default() -> Self {
        Self {
            min_generator_version: "0.1.0".to_string(),
            base_images: Vec::new(),
            required_features: Vec::new(),
        }
    }
}

/// Metadata about a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Human-readable difficulty level (e.g., "easy", "medium", "hard").
    pub difficulty: String,
    /// Numerical difficulty score (0.0 - 1.0).
    pub difficulty_score: f64,
    /// Primary category (e.g., "debugging", "refactoring").
    pub category: String,
    /// Subcategory within the primary category.
    pub subcategory: String,
    /// Tags for searching and filtering.
    pub tags: Vec<String>,
    /// Author identifier.
    pub author: String,
    /// ISO 8601 timestamp of creation.
    pub created_at: String,
    /// ISO 8601 timestamp of last update.
    pub updated_at: String,
}

/// A complete registry entry for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRegistryEntry {
    /// Unique identifier for this task instance.
    pub id: String,
    /// Identifier of the template used to generate this task.
    pub template_id: String,
    /// Seed used for deterministic generation.
    pub seed: u64,
    /// Semantic version of this task definition.
    pub version: String,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Task metadata including difficulty and categorization.
    pub metadata: TaskMetadata,
    /// Human calibration data.
    pub calibration: Calibration,
    /// Runtime compatibility requirements.
    pub compatibility: Compatibility,
    /// Additional statistics as key-value pairs.
    pub statistics: HashMap<String, serde_json::Value>,
}

impl TaskRegistryEntry {
    /// Create a new task registry entry with default values.
    ///
    /// The entry starts in Draft status with version "1.0.0".
    pub fn new(id: String, template_id: String, seed: u64, metadata: TaskMetadata) -> Self {
        Self {
            id,
            template_id,
            seed,
            version: "1.0.0".to_string(),
            status: TaskStatus::Draft,
            metadata,
            calibration: Calibration::default(),
            compatibility: Compatibility::default(),
            statistics: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_serialization() {
        let status = TaskStatus::Draft;
        let json = serde_json::to_string(&status).expect("serialization should succeed");
        assert_eq!(json, "\"draft\"");

        let status = TaskStatus::Published;
        let json = serde_json::to_string(&status).expect("serialization should succeed");
        assert_eq!(json, "\"published\"");
    }

    #[test]
    fn test_calibration_default() {
        let calibration = Calibration::default();
        assert!(!calibration.human_tested);
        assert_eq!(calibration.num_testers, 0);
        assert_eq!(calibration.avg_time_seconds, 0.0);
        assert_eq!(calibration.success_rate, 0.0);
        assert!(calibration.last_calibration.is_none());
    }

    #[test]
    fn test_compatibility_default() {
        let compat = Compatibility::default();
        assert_eq!(compat.min_generator_version, "0.1.0");
        assert!(compat.base_images.is_empty());
        assert!(compat.required_features.is_empty());
    }

    #[test]
    fn test_task_registry_entry_new() {
        let metadata = TaskMetadata {
            difficulty: "medium".to_string(),
            difficulty_score: 0.5,
            category: "debugging".to_string(),
            subcategory: "runtime_errors".to_string(),
            tags: vec!["rust".to_string(), "panic".to_string()],
            author: "test_author".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let entry = TaskRegistryEntry::new(
            "task-001".to_string(),
            "template-debug-001".to_string(),
            12345,
            metadata,
        );

        assert_eq!(entry.id, "task-001");
        assert_eq!(entry.template_id, "template-debug-001");
        assert_eq!(entry.seed, 12345);
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.status, TaskStatus::Draft);
        assert_eq!(entry.metadata.difficulty, "medium");
        assert!(!entry.calibration.human_tested);
        assert!(entry.statistics.is_empty());
    }
}
