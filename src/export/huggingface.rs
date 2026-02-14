//! HuggingFace dataset export functionality.
//!
//! Provides utilities for exporting benchmark tasks to HuggingFace dataset format.

use crate::error::ExportError;
use crate::registry::{TaskRegistry, TaskRegistryEntry, TaskStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Dataset card metadata for HuggingFace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCard {
    /// License for the dataset.
    pub license: String,
    /// Task categories (e.g., "text-generation").
    pub task_categories: Vec<String>,
    /// Languages supported.
    pub language: Vec<String>,
    /// Tags for discovery.
    pub tags: Vec<String>,
    /// Human-readable dataset name.
    pub pretty_name: String,
    /// Size categories (e.g., "1K<n<10K").
    pub size_categories: Vec<String>,
}

impl Default for DatasetCard {
    fn default() -> Self {
        Self {
            license: "apache-2.0".to_string(),
            task_categories: vec!["text-generation".to_string()],
            language: vec!["en".to_string()],
            tags: vec![
                "benchmark".to_string(),
                "terminal".to_string(),
                "cli".to_string(),
                "synthetic".to_string(),
            ],
            pretty_name: "Synthetic Terminal Benchmark".to_string(),
            size_categories: vec!["1K<n<10K".to_string()],
        }
    }
}

/// Exporter for HuggingFace dataset format.
pub struct HuggingFaceExporter {
    /// The task registry to export from.
    registry: TaskRegistry,
    /// HuggingFace repository ID (e.g., "org/dataset-name").
    repo_id: String,
}

impl HuggingFaceExporter {
    /// Creates a new HuggingFace exporter.
    ///
    /// # Arguments
    ///
    /// * `registry` - The task registry containing tasks to export.
    /// * `repo_id` - The HuggingFace repository ID (e.g., "org/dataset-name").
    pub fn new(registry: TaskRegistry, repo_id: String) -> Self {
        Self { registry, repo_id }
    }

    /// Exports the dataset to the specified output directory.
    ///
    /// # Arguments
    ///
    /// * `output_dir` - Base directory for the export.
    /// * `version` - Version tag for this export (e.g., "v1.0.0").
    /// * `include_solutions` - Whether to include solution files.
    ///
    /// # Returns
    ///
    /// The path to the created export directory.
    pub fn export(
        &self,
        output_dir: &Path,
        version: &str,
        include_solutions: bool,
    ) -> Result<PathBuf, ExportError> {
        // Validate version format
        if !version.starts_with('v') || version.len() < 2 {
            return Err(ExportError::InvalidVersion(format!(
                "Version must start with 'v' followed by version number, got: {}",
                version
            )));
        }

        // Get published tasks only
        let tasks: Vec<&TaskRegistryEntry> = self
            .registry
            .filter_by_status(TaskStatus::Published)
            .into_iter()
            .collect();

        if tasks.is_empty() {
            return Err(ExportError::NoTasks);
        }

        // Create versioned export directory
        let export_dir = output_dir.join(&self.repo_id).join(version);
        fs::create_dir_all(&export_dir)?;

        // Create subdirectories
        let tasks_dir = export_dir.join("tasks");
        let metadata_dir = export_dir.join("metadata");
        fs::create_dir_all(&tasks_dir)?;
        fs::create_dir_all(&metadata_dir)?;

        // Export each task
        for entry in &tasks {
            self.export_task(entry, &tasks_dir, include_solutions)?;
        }

        // Export metadata
        self.export_metadata(&metadata_dir, &tasks)?;

        // Create dataset card
        self.create_dataset_card(&export_dir, version, tasks.len())?;

        // Create README
        self.create_readme(&export_dir, version)?;

        Ok(export_dir)
    }

    /// Exports a single task to the tasks directory.
    fn export_task(
        &self,
        entry: &TaskRegistryEntry,
        tasks_dir: &Path,
        include_solutions: bool,
    ) -> Result<(), ExportError> {
        let task_dir = tasks_dir.join(&entry.id);
        fs::create_dir_all(&task_dir)?;

        // Export task metadata
        let metadata_path = task_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&entry)?;
        fs::write(metadata_path, metadata_json)?;

        // Create prompt file (description of the task)
        let prompt_content = format!(
            "# Task: {}\n\n\
            Category: {} / {}\n\
            Difficulty: {} (score: {:.2})\n\
            Template: {}\n\n\
            Tags: {}\n",
            entry.id,
            entry.metadata.category,
            entry.metadata.subcategory,
            entry.metadata.difficulty,
            entry.metadata.difficulty_score,
            entry.template_id,
            entry.metadata.tags.join(", ")
        );
        fs::write(task_dir.join("prompt.md"), prompt_content)?;

        if include_solutions {
            // Create placeholder solution file
            let solution_content = format!(
                "# Solution for {}\n\n\
                This is the reference solution for the task.\n\
                Template: {}\n\
                Seed: {}\n",
                entry.id, entry.template_id, entry.seed
            );
            fs::write(task_dir.join("solution.md"), solution_content)?;
        }

        Ok(())
    }

    /// Exports aggregate metadata to the metadata directory.
    fn export_metadata(
        &self,
        metadata_dir: &Path,
        tasks: &[&TaskRegistryEntry],
    ) -> Result<(), ExportError> {
        // Create tasks index
        let task_index: Vec<TaskIndexEntry> = tasks
            .iter()
            .map(|t| TaskIndexEntry {
                id: t.id.clone(),
                template_id: t.template_id.clone(),
                category: t.metadata.category.clone(),
                subcategory: t.metadata.subcategory.clone(),
                difficulty: t.metadata.difficulty.clone(),
                difficulty_score: t.metadata.difficulty_score,
            })
            .collect();

        let index_path = metadata_dir.join("tasks_index.json");
        let index_json = serde_json::to_string_pretty(&task_index)?;
        fs::write(index_path, index_json)?;

        // Create statistics file
        let stats = ExportStatistics::from_tasks(tasks);
        let stats_path = metadata_dir.join("statistics.json");
        let stats_json = serde_json::to_string_pretty(&stats)?;
        fs::write(stats_path, stats_json)?;

        Ok(())
    }

    /// Creates the HuggingFace dataset card (YAML frontmatter).
    fn create_dataset_card(
        &self,
        export_dir: &Path,
        version: &str,
        task_count: usize,
    ) -> Result<(), ExportError> {
        let card = DatasetCard::default();

        // Determine size category based on task count
        let size_category = match task_count {
            0..=100 => "n<1K",
            101..=1000 => "1K<n<10K",
            1001..=10000 => "10K<n<100K",
            _ => "n>100K",
        };

        let card_content = format!(
            "---\n\
            license: {}\n\
            task_categories:\n{}\n\
            language:\n{}\n\
            tags:\n{}\n\
            pretty_name: {}\n\
            size_categories:\n  - {}\n\
            ---\n",
            card.license,
            card.task_categories
                .iter()
                .map(|t| format!("  - {}", t))
                .collect::<Vec<_>>()
                .join("\n"),
            card.language
                .iter()
                .map(|l| format!("  - {}", l))
                .collect::<Vec<_>>()
                .join("\n"),
            card.tags
                .iter()
                .map(|t| format!("  - {}", t))
                .collect::<Vec<_>>()
                .join("\n"),
            card.pretty_name,
            size_category
        );

        let card_path = export_dir.join("dataset_card.yaml");
        fs::write(card_path, card_content)?;

        // Also include version info
        let version_info = serde_json::json!({
            "version": version,
            "repo_id": self.repo_id,
            "task_count": task_count,
            "exported_at": chrono::Utc::now().to_rfc3339(),
        });

        let version_path = export_dir.join("version.json");
        fs::write(version_path, serde_json::to_string_pretty(&version_info)?)?;

        Ok(())
    }

    /// Creates the README file for the dataset.
    fn create_readme(&self, export_dir: &Path, version: &str) -> Result<(), ExportError> {
        let readme_content = format!(
            "# {}\n\n\
            ## Description\n\n\
            Synthetic terminal benchmark dataset for evaluating LLM capabilities \
            in command-line and system administration tasks.\n\n\
            ## Version\n\n\
            {}\n\n\
            ## Repository\n\n\
            {}\n\n\
            ## Structure\n\n\
            ```\n\
            .\n\
            ├── tasks/           # Individual task directories\n\
            │   └── <task-id>/\n\
            │       ├── metadata.json\n\
            │       ├── prompt.md\n\
            │       └── solution.md (if included)\n\
            ├── metadata/        # Aggregate metadata\n\
            │   ├── tasks_index.json\n\
            │   └── statistics.json\n\
            ├── dataset_card.yaml\n\
            ├── version.json\n\
            └── README.md\n\
            ```\n\n\
            ## License\n\n\
            Apache 2.0\n",
            self.repo_id.split('/').next_back().unwrap_or(&self.repo_id),
            version,
            self.repo_id
        );

        let readme_path = export_dir.join("README.md");
        fs::write(readme_path, readme_content)?;

        Ok(())
    }
}

/// Entry in the task index file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskIndexEntry {
    id: String,
    template_id: String,
    category: String,
    subcategory: String,
    difficulty: String,
    difficulty_score: f64,
}

/// Export statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportStatistics {
    total_tasks: usize,
    by_category: std::collections::HashMap<String, usize>,
    by_difficulty: std::collections::HashMap<String, usize>,
    avg_difficulty_score: f64,
}

impl ExportStatistics {
    fn from_tasks(tasks: &[&TaskRegistryEntry]) -> Self {
        let mut by_category: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut by_difficulty: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut total_score = 0.0;

        for task in tasks {
            *by_category
                .entry(task.metadata.category.clone())
                .or_default() += 1;
            *by_difficulty
                .entry(task.metadata.difficulty.clone())
                .or_default() += 1;
            total_score += task.metadata.difficulty_score;
        }

        let avg_difficulty_score = if tasks.is_empty() {
            0.0
        } else {
            total_score / tasks.len() as f64
        };

        Self {
            total_tasks: tasks.len(),
            by_category,
            by_difficulty,
            avg_difficulty_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::TaskMetadata;
    use std::path::PathBuf;

    fn create_test_registry() -> TaskRegistry {
        let mut registry = TaskRegistry::new(PathBuf::from("/tmp/swe_forge-test"));

        let metadata = TaskMetadata {
            difficulty: "medium".to_string(),
            difficulty_score: 0.5,
            category: "debugging".to_string(),
            subcategory: "runtime_errors".to_string(),
            tags: vec!["rust".to_string()],
            author: "test".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let mut entry = crate::registry::TaskRegistryEntry::new(
            "task-001".to_string(),
            "template-001".to_string(),
            12345,
            metadata,
        );
        entry.status = TaskStatus::Published;

        registry.register(entry).expect("should register");
        registry
    }

    #[test]
    fn test_dataset_card_default() {
        let card = DatasetCard::default();
        assert_eq!(card.license, "apache-2.0");
        assert!(!card.tags.is_empty());
    }

    #[test]
    fn test_exporter_new() {
        let registry = TaskRegistry::new(PathBuf::from("/tmp/test"));
        let exporter = HuggingFaceExporter::new(registry, "org/dataset".to_string());
        assert_eq!(exporter.repo_id, "org/dataset");
    }

    #[test]
    fn test_export_no_tasks() {
        let registry = TaskRegistry::new(PathBuf::from("/tmp/swe_forge-test-empty"));
        let exporter = HuggingFaceExporter::new(registry, "org/dataset".to_string());

        let temp_dir = std::env::temp_dir().join("swe_forge-test-empty");
        let result = exporter.export(&temp_dir, "v1.0.0", false);

        assert!(matches!(result, Err(ExportError::NoTasks)));
    }

    #[test]
    fn test_export_invalid_version() {
        let registry = create_test_registry();
        let exporter = HuggingFaceExporter::new(registry, "org/dataset".to_string());

        let temp_dir = std::env::temp_dir().join("swe_forge-test-version");
        let result = exporter.export(&temp_dir, "1.0.0", false);

        assert!(matches!(result, Err(ExportError::InvalidVersion(_))));
    }
}
