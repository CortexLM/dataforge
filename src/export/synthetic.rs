//! Synthetic benchmark task export functionality for HuggingFace datasets.
//!
//! Provides exporters for converting `SyntheticTask` instances into HuggingFace-compatible
//! JSONL format, including dataset cards and separate solution files.

use crate::agents::task_executor::{AutomatedCheck, PartialCreditItem, SyntheticTask};
use crate::error::ExportError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

// ============================================================================
// Dataset Entry Types
// ============================================================================

/// Entry format for synthetic benchmark datasets on HuggingFace.
///
/// This structure represents a single row in the exported JSONL file,
/// containing all task information visible to test-takers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticDatasetEntry {
    /// Unique task identifier.
    pub id: String,
    /// Task category (e.g., "debugging", "file_manipulation").
    pub category: String,
    /// Task subcategory for finer classification.
    pub subcategory: String,
    /// Problem statement (what test-takers see).
    pub problem_statement: String,
    /// Difficulty level ("easy", "medium", "hard").
    pub difficulty: String,
    /// Required skills as comma-separated string.
    pub required_skills: String,
    /// Tags as comma-separated string.
    pub tags: String,
    /// Success criteria as JSON array string.
    pub success_criteria: String,
    /// Partial credit criteria as JSON string.
    pub partial_credit: String,
    /// Automated checks as JSON string.
    pub automated_checks: String,
    /// Anti-memorization canary token.
    pub canary_token: String,
    /// Task specification version.
    pub version: String,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}

/// Entry format for synthetic task solutions (kept separate from problems).
///
/// Solutions should be stored in a separate file that is not included
/// in public dataset releases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticSolutionEntry {
    /// Task ID this solution belongs to.
    pub task_id: String,
    /// High-level solution approach.
    pub approach: String,
    /// Key insights required (comma-separated).
    pub key_insights: String,
    /// Reference commands (comma-separated).
    pub reference_commands: String,
    /// Expected completion time in seconds.
    pub expected_time_seconds: u32,
    /// Number of steps in the solution.
    pub step_count: u32,
}

// ============================================================================
// Export Result
// ============================================================================

/// Result of a synthetic dataset export operation.
#[derive(Debug, Clone)]
pub struct SyntheticExportResult {
    /// Path to the main dataset file (JSONL).
    pub dataset_path: PathBuf,
    /// Path to solutions file (if exported).
    pub solutions_path: Option<PathBuf>,
    /// Path to dataset card (README.md).
    pub card_path: PathBuf,
    /// Number of tasks exported.
    pub task_count: usize,
    /// Category distribution (category -> count).
    pub category_distribution: HashMap<String, usize>,
    /// Export timestamp.
    pub exported_at: DateTime<Utc>,
}

// ============================================================================
// Synthetic Exporter
// ============================================================================

/// Exporter for synthetic benchmark tasks to HuggingFace format.
///
/// Exports `SyntheticTask` instances to:
/// - JSONL format for dataset loading
/// - Separate solutions file (optional)
/// - Dataset card (README.md with YAML frontmatter)
///
/// # Example
///
/// ```ignore
/// use dataforge::export::SyntheticExporter;
/// use dataforge::agents::task_executor::SyntheticTask;
///
/// let exporter = SyntheticExporter::new("./export", "1.0.0")
///     .with_solutions(false);
///
/// let tasks: Vec<SyntheticTask> = vec![/* ... */];
/// let result = exporter.export(&tasks)?;
/// println!("Exported {} tasks to {}", result.task_count, result.dataset_path.display());
/// ```
pub struct SyntheticExporter {
    /// Output directory for exported files.
    output_dir: PathBuf,
    /// Dataset version.
    version: String,
    /// Whether to include solutions in export.
    include_solutions: bool,
}

impl SyntheticExporter {
    /// Creates a new SyntheticExporter.
    ///
    /// # Arguments
    ///
    /// * `output_dir` - Directory where exported files will be written.
    /// * `version` - Version string for the dataset.
    pub fn new(output_dir: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            output_dir: output_dir.into(),
            version: version.into(),
            include_solutions: false,
        }
    }

    /// Set whether to include solutions in the export.
    ///
    /// When enabled, creates a separate `solutions.jsonl` file containing
    /// hidden solution information. This file should be kept private.
    pub fn with_solutions(mut self, include: bool) -> Self {
        self.include_solutions = include;
        self
    }

    /// Export a collection of synthetic tasks.
    ///
    /// Creates the output directory structure and exports:
    /// - `data/test.jsonl` - Main dataset file
    /// - `solutions.jsonl` - Solutions file (if enabled)
    /// - `README.md` - Dataset card
    ///
    /// # Arguments
    ///
    /// * `tasks` - Slice of SyntheticTask instances to export.
    ///
    /// # Returns
    ///
    /// A `SyntheticExportResult` containing paths and statistics.
    pub fn export(&self, tasks: &[SyntheticTask]) -> Result<SyntheticExportResult, ExportError> {
        if tasks.is_empty() {
            return Err(ExportError::NoTasks);
        }

        // Create output directory structure
        fs::create_dir_all(&self.output_dir)?;
        let data_dir = self.output_dir.join("data");
        fs::create_dir_all(&data_dir)?;

        // Export main dataset
        let dataset_path = self.export_jsonl(tasks)?;

        // Export solutions if enabled
        let solutions_path = if self.include_solutions {
            Some(self.export_solutions(tasks)?)
        } else {
            None
        };

        // Generate dataset card
        let card_content = self.generate_dataset_card(tasks)?;
        let card_path = self.output_dir.join("README.md");
        fs::write(&card_path, card_content)?;

        // Calculate category distribution
        let category_distribution = self.calculate_category_distribution(tasks);

        Ok(SyntheticExportResult {
            dataset_path,
            solutions_path,
            card_path,
            task_count: tasks.len(),
            category_distribution,
            exported_at: Utc::now(),
        })
    }

    /// Export tasks to JSONL format (HuggingFace compatible).
    ///
    /// Creates a `data/test.jsonl` file with one JSON object per line.
    pub fn export_jsonl(&self, tasks: &[SyntheticTask]) -> Result<PathBuf, ExportError> {
        let data_dir = self.output_dir.join("data");
        fs::create_dir_all(&data_dir)?;

        let output_path = data_dir.join("test.jsonl");
        let file = File::create(&output_path)?;
        let mut writer = BufWriter::new(file);

        for task in tasks {
            let entry = self.task_to_entry(task);
            let json_line = serde_json::to_string(&entry)?;
            writeln!(writer, "{}", json_line)?;
        }

        writer.flush()?;
        Ok(output_path)
    }

    /// Export solutions to separate JSONL file.
    ///
    /// Creates a `solutions.jsonl` file with hidden solution information.
    /// This file should be kept private and not included in public releases.
    pub fn export_solutions(&self, tasks: &[SyntheticTask]) -> Result<PathBuf, ExportError> {
        let output_path = self.output_dir.join("solutions.jsonl");
        let file = File::create(&output_path)?;
        let mut writer = BufWriter::new(file);

        for task in tasks {
            let solution = self.task_to_solution(task);
            let json_line = serde_json::to_string(&solution)?;
            writeln!(writer, "{}", json_line)?;
        }

        writer.flush()?;
        Ok(output_path)
    }

    /// Generate dataset card (README.md for HuggingFace).
    ///
    /// Creates a markdown file with YAML frontmatter containing
    /// dataset metadata and usage instructions.
    pub fn generate_dataset_card(&self, tasks: &[SyntheticTask]) -> Result<String, ExportError> {
        let category_distribution = self.calculate_category_distribution(tasks);
        let difficulty_distribution = self.calculate_difficulty_distribution(tasks);

        // Build category distribution table
        let mut category_table = String::new();
        let mut categories: Vec<_> = category_distribution.iter().collect();
        categories.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
        for (category, count) in categories {
            category_table.push_str(&format!("| {} | {} |\n", category, count));
        }

        // Build difficulty distribution table
        let mut difficulty_table = String::new();
        for level in &["easy", "medium", "hard"] {
            let count = difficulty_distribution.get(*level).unwrap_or(&0);
            difficulty_table.push_str(&format!("| {} | {} |\n", level, count));
        }

        let card = format!(
            r#"---
dataset_info:
  features:
  - name: id
    dtype: string
  - name: category
    dtype: string
  - name: subcategory
    dtype: string
  - name: problem_statement
    dtype: string
  - name: difficulty
    dtype: string
  - name: required_skills
    dtype: string
  - name: tags
    dtype: string
  - name: success_criteria
    dtype: string
  - name: partial_credit
    dtype: string
  - name: automated_checks
    dtype: string
  - name: canary_token
    dtype: string
  - name: version
    dtype: string
  - name: created_at
    dtype: string
  splits:
  - name: test
    num_examples: {task_count}
license: mit
task_categories:
- question-answering
language:
- en
tags:
- benchmark
- synthetic
- terminal
- cli
- agent-evaluation
pretty_name: Synthetic Benchmark Dataset v{version}
size_categories:
- {size_category}
---

# Synthetic Benchmark Dataset v{version}

## Description

Synthetic benchmark tasks generated by dataforge for evaluating AI agent capabilities
in terminal and command-line environments. Each task presents a problem statement that
agents must solve without access to the hidden solution methodology.

## Dataset Statistics

- **Total Tasks**: {task_count}
- **Version**: {version}
- **License**: MIT

## Categories

| Category | Count |
|----------|-------|
{category_table}
## Difficulty Distribution

| Difficulty | Count |
|------------|-------|
{difficulty_table}
## Usage

```python
from datasets import load_dataset

# Load the dataset
dataset = load_dataset("path/to/dataset")

# Access test split
for example in dataset["test"]:
    print(f"Task: {{example['id']}}")
    print(f"Problem: {{example['problem_statement']}}")
    print(f"Difficulty: {{example['difficulty']}}")
```

## Schema

Each entry contains:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique task identifier |
| `category` | string | Primary category |
| `subcategory` | string | Finer classification |
| `problem_statement` | string | Task description (visible to test-takers) |
| `difficulty` | string | Difficulty level (easy/medium/hard) |
| `required_skills` | string | Comma-separated required skills |
| `tags` | string | Comma-separated tags |
| `success_criteria` | string | JSON array of success criteria |
| `partial_credit` | string | JSON object with partial credit rules |
| `automated_checks` | string | JSON array of automated validation checks |
| `canary_token` | string | Anti-memorization token |
| `version` | string | Task specification version |
| `created_at` | string | ISO 8601 timestamp |

## Anti-Memorization

Each task contains a unique canary token to detect memorization and data contamination.
These tokens are embedded in the problem statement and can be used to verify that
solutions are generated freshly rather than recalled from training data.

## Evaluation

Tasks should be evaluated using the success criteria and automated checks defined
in each entry. Partial credit may be awarded based on the `partial_credit` rules.

## Citation

If you use this dataset, please cite:

```bibtex
@misc{{dataforge_v{version_underscore},
  title = {{Synthetic Benchmark Dataset v{version}}},
  year = {{{year}}},
  publisher = {{HuggingFace}},
  note = {{Generated by dataforge}}
}}
```
"#,
            task_count = tasks.len(),
            version = self.version,
            size_category = self.determine_size_category(tasks.len()),
            category_table = category_table,
            difficulty_table = difficulty_table,
            version_underscore = self.version.replace('.', "_"),
            year = Utc::now().format("%Y"),
        );

        Ok(card)
    }

    /// Convert SyntheticTask to dataset entry format.
    fn task_to_entry(&self, task: &SyntheticTask) -> SyntheticDatasetEntry {
        // Serialize partial credit criteria
        let partial_credit_json =
            serialize_partial_credit(&task.verification.partial_credit_criteria);

        // Serialize automated checks
        let automated_checks_json = serialize_automated_checks(&task.verification.automated_checks);

        SyntheticDatasetEntry {
            id: task.id.clone(),
            category: task.metadata.category.clone(),
            subcategory: task.metadata.subcategory.clone(),
            problem_statement: task.problem_statement.clone(),
            difficulty: format!("{:?}", task.difficulty.level).to_lowercase(),
            required_skills: task.metadata.tags.join(", "),
            tags: task.metadata.tags.join(", "),
            success_criteria: serde_json::to_string(&task.verification.success_criteria)
                .unwrap_or_else(|_| "[]".to_string()),
            partial_credit: partial_credit_json,
            automated_checks: automated_checks_json,
            canary_token: task.anti_memorization.canary_token.clone(),
            version: task.version.clone(),
            created_at: task.created_at.to_rfc3339(),
        }
    }

    /// Convert SyntheticTask to solution entry format.
    fn task_to_solution(&self, task: &SyntheticTask) -> SyntheticSolutionEntry {
        SyntheticSolutionEntry {
            task_id: task.id.clone(),
            approach: task.hidden_solution.approach.clone(),
            key_insights: task.hidden_solution.key_insights.join(", "),
            reference_commands: task.hidden_solution.reference_commands.join(", "),
            expected_time_seconds: task.hidden_solution.expected_time_seconds,
            step_count: task.hidden_solution.step_count,
        }
    }

    /// Calculate category distribution from tasks.
    fn calculate_category_distribution(&self, tasks: &[SyntheticTask]) -> HashMap<String, usize> {
        let mut distribution = HashMap::new();
        for task in tasks {
            *distribution
                .entry(task.metadata.category.clone())
                .or_insert(0) += 1;
        }
        distribution
    }

    /// Calculate difficulty distribution from tasks.
    fn calculate_difficulty_distribution(&self, tasks: &[SyntheticTask]) -> HashMap<String, usize> {
        let mut distribution = HashMap::new();
        for task in tasks {
            let level = format!("{:?}", task.difficulty.level).to_lowercase();
            *distribution.entry(level).or_insert(0) += 1;
        }
        distribution
    }

    /// Determine HuggingFace size category based on task count.
    fn determine_size_category(&self, count: usize) -> &'static str {
        match count {
            0..=99 => "n<1K",
            100..=999 => "1K<n<10K",
            1000..=9999 => "1K<n<10K",
            10000..=99999 => "10K<n<100K",
            _ => "n>100K",
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Serialize partial credit criteria to JSON string.
fn serialize_partial_credit(criteria: &[PartialCreditItem]) -> String {
    let items: Vec<serde_json::Value> = criteria
        .iter()
        .map(|item| {
            serde_json::json!({
                "criterion": item.criterion,
                "points": item.points
            })
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Serialize automated checks to JSON string.
fn serialize_automated_checks(checks: &[AutomatedCheck]) -> String {
    let items: Vec<serde_json::Value> = checks
        .iter()
        .map(|check| {
            serde_json::json!({
                "check_type": format!("{:?}", check.check_type),
                "target": check.target,
                "expected": check.expected
            })
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::task_executor::{
        AntiMemorizationConfig, AutomatedCheck, DifficultyScoring, HiddenSolution,
        PartialCreditItem, TaskMetadata, VerificationSpec,
    };
    use crate::difficulty::DifficultyLevel;
    use tempfile::TempDir;

    fn create_test_task(id: &str, category: &str, difficulty: DifficultyLevel) -> SyntheticTask {
        let hidden_solution = HiddenSolution::new("Use grep to find patterns")
            .with_key_insights(["insight1", "insight2"])
            .with_reference_commands(["grep pattern file.txt", "cat output.txt"])
            .with_expected_time_seconds(120)
            .with_step_count(2);

        let verification = VerificationSpec::new()
            .with_success_criteria(["Output file exists", "Content matches expected"])
            .with_partial_credit([
                PartialCreditItem::new("Found partial result", 0.25),
                PartialCreditItem::new("Correct approach", 0.5),
            ])
            .with_automated_checks([
                AutomatedCheck::file_exists("/tmp/output.txt"),
                AutomatedCheck::output_contains("result", "expected"),
            ]);

        let difficulty_scoring = DifficultyScoring::new(difficulty)
            .with_complexity_factors(["Multiple files", "Pattern matching"])
            .with_base_score(25.0);

        let metadata = TaskMetadata::new(category, "test-idea-1")
            .with_subcategory("log-analysis")
            .with_tags(["grep", "file-ops", "text-processing"]);

        let anti_memorization = AntiMemorizationConfig::new(format!("CANARY_{}", id))
            .with_dynamic_value("timestamp", "1234567890");

        SyntheticTask {
            id: id.to_string(),
            version: "1.0.0".to_string(),
            problem_statement: format!(
                "Find all error entries in the log files and report the count. [{}]",
                id
            ),
            hidden_solution,
            verification,
            difficulty: difficulty_scoring,
            metadata,
            anti_memorization,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_synthetic_exporter_new() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");
        assert_eq!(exporter.output_dir, PathBuf::from("/tmp/export"));
        assert_eq!(exporter.version, "1.0.0");
        assert!(!exporter.include_solutions);
    }

    #[test]
    fn test_synthetic_exporter_with_solutions() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0").with_solutions(true);
        assert!(exporter.include_solutions);
    }

    #[test]
    fn test_task_to_entry() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");
        let task = create_test_task("task-001", "debugging", DifficultyLevel::Medium);

        let entry = exporter.task_to_entry(&task);

        assert_eq!(entry.id, "task-001");
        assert_eq!(entry.category, "debugging");
        assert_eq!(entry.subcategory, "log-analysis");
        assert_eq!(entry.difficulty, "medium");
        assert!(entry.problem_statement.contains("Find all error entries"));
        assert!(entry.required_skills.contains("grep"));
        assert!(entry.tags.contains("file-ops"));
        assert!(entry.canary_token.contains("CANARY_task-001"));
        assert_eq!(entry.version, "1.0.0");
        assert!(!entry.created_at.is_empty());

        // Verify JSON serialization of complex fields
        let success_criteria: Vec<String> =
            serde_json::from_str(&entry.success_criteria).expect("should parse success_criteria");
        assert_eq!(success_criteria.len(), 2);
        assert!(success_criteria.contains(&"Output file exists".to_string()));

        let partial_credit: Vec<serde_json::Value> =
            serde_json::from_str(&entry.partial_credit).expect("should parse partial_credit");
        assert_eq!(partial_credit.len(), 2);

        let automated_checks: Vec<serde_json::Value> =
            serde_json::from_str(&entry.automated_checks).expect("should parse automated_checks");
        assert_eq!(automated_checks.len(), 2);
    }

    #[test]
    fn test_task_to_solution() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");
        let task = create_test_task("task-001", "debugging", DifficultyLevel::Medium);

        let solution = exporter.task_to_solution(&task);

        assert_eq!(solution.task_id, "task-001");
        assert_eq!(solution.approach, "Use grep to find patterns");
        assert!(solution.key_insights.contains("insight1"));
        assert!(solution.key_insights.contains("insight2"));
        assert!(solution
            .reference_commands
            .contains("grep pattern file.txt"));
        assert_eq!(solution.expected_time_seconds, 120);
        assert_eq!(solution.step_count, 2);
    }

    #[test]
    fn test_export_empty_tasks() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let exporter = SyntheticExporter::new(temp_dir.path(), "1.0.0");

        let result = exporter.export(&[]);

        assert!(matches!(result, Err(ExportError::NoTasks)));
    }

    #[test]
    fn test_export_jsonl() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let exporter = SyntheticExporter::new(temp_dir.path(), "1.0.0");

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "file_manipulation", DifficultyLevel::Medium),
            create_test_task("task-003", "debugging", DifficultyLevel::Hard),
        ];

        let path = exporter.export_jsonl(&tasks).expect("should export");

        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "test.jsonl");

        // Verify content
        let content = fs::read_to_string(&path).expect("should read file");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);

        // Verify each line is valid JSON
        for line in lines {
            let entry: SyntheticDatasetEntry =
                serde_json::from_str(line).expect("should parse JSON line");
            assert!(!entry.id.is_empty());
            assert!(!entry.problem_statement.is_empty());
        }
    }

    #[test]
    fn test_export_solutions() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let exporter = SyntheticExporter::new(temp_dir.path(), "1.0.0").with_solutions(true);

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "file_manipulation", DifficultyLevel::Medium),
        ];

        let path = exporter
            .export_solutions(&tasks)
            .expect("should export solutions");

        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "solutions.jsonl");

        // Verify content
        let content = fs::read_to_string(&path).expect("should read file");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Verify each line is valid JSON
        for line in lines {
            let solution: SyntheticSolutionEntry =
                serde_json::from_str(line).expect("should parse JSON line");
            assert!(!solution.task_id.is_empty());
            assert!(!solution.approach.is_empty());
        }
    }

    #[test]
    fn test_generate_dataset_card() {
        let exporter = SyntheticExporter::new("/tmp/export", "2.0.0");

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "debugging", DifficultyLevel::Easy),
            create_test_task("task-003", "file_manipulation", DifficultyLevel::Medium),
            create_test_task("task-004", "file_manipulation", DifficultyLevel::Hard),
        ];

        let card = exporter
            .generate_dataset_card(&tasks)
            .expect("should generate card");

        // Verify YAML frontmatter
        assert!(card.starts_with("---"));
        assert!(card.contains("license: mit"));
        assert!(card.contains("num_examples: 4"));
        assert!(card.contains("pretty_name: Synthetic Benchmark Dataset v2.0.0"));

        // Verify content sections
        assert!(card.contains("# Synthetic Benchmark Dataset v2.0.0"));
        assert!(card.contains("## Description"));
        assert!(card.contains("## Categories"));
        assert!(card.contains("## Difficulty Distribution"));
        assert!(card.contains("## Usage"));
        assert!(card.contains("## Anti-Memorization"));

        // Verify category distribution
        assert!(card.contains("| debugging | 2 |"));
        assert!(card.contains("| file_manipulation | 2 |"));

        // Verify difficulty distribution
        assert!(card.contains("| easy | 2 |"));
        assert!(card.contains("| medium | 1 |"));
        assert!(card.contains("| hard | 1 |"));

        // Verify Python usage example
        assert!(card.contains("from datasets import load_dataset"));
    }

    #[test]
    fn test_full_export() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let exporter = SyntheticExporter::new(temp_dir.path(), "1.0.0").with_solutions(true);

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "debugging", DifficultyLevel::Medium),
            create_test_task("task-003", "file_manipulation", DifficultyLevel::Hard),
        ];

        let result = exporter.export(&tasks).expect("should export");

        // Verify result struct
        assert_eq!(result.task_count, 3);
        assert!(result.dataset_path.exists());
        assert!(result.solutions_path.is_some());
        assert!(result.solutions_path.as_ref().unwrap().exists());
        assert!(result.card_path.exists());

        // Verify category distribution
        assert_eq!(result.category_distribution.get("debugging"), Some(&2));
        assert_eq!(
            result.category_distribution.get("file_manipulation"),
            Some(&1)
        );

        // Verify directory structure
        assert!(temp_dir.path().join("data").exists());
        assert!(temp_dir.path().join("data/test.jsonl").exists());
        assert!(temp_dir.path().join("solutions.jsonl").exists());
        assert!(temp_dir.path().join("README.md").exists());
    }

    #[test]
    fn test_export_without_solutions() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let exporter = SyntheticExporter::new(temp_dir.path(), "1.0.0").with_solutions(false);

        let tasks = vec![create_test_task(
            "task-001",
            "debugging",
            DifficultyLevel::Easy,
        )];

        let result = exporter.export(&tasks).expect("should export");

        assert!(result.solutions_path.is_none());
        assert!(!temp_dir.path().join("solutions.jsonl").exists());
    }

    #[test]
    fn test_calculate_category_distribution() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "debugging", DifficultyLevel::Medium),
            create_test_task("task-003", "debugging", DifficultyLevel::Hard),
            create_test_task("task-004", "file_manipulation", DifficultyLevel::Easy),
            create_test_task("task-005", "networking", DifficultyLevel::Medium),
        ];

        let distribution = exporter.calculate_category_distribution(&tasks);

        assert_eq!(distribution.get("debugging"), Some(&3));
        assert_eq!(distribution.get("file_manipulation"), Some(&1));
        assert_eq!(distribution.get("networking"), Some(&1));
        assert_eq!(distribution.len(), 3);
    }

    #[test]
    fn test_calculate_difficulty_distribution() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");

        let tasks = vec![
            create_test_task("task-001", "debugging", DifficultyLevel::Easy),
            create_test_task("task-002", "debugging", DifficultyLevel::Easy),
            create_test_task("task-003", "debugging", DifficultyLevel::Medium),
            create_test_task("task-004", "debugging", DifficultyLevel::Hard),
        ];

        let distribution = exporter.calculate_difficulty_distribution(&tasks);

        assert_eq!(distribution.get("easy"), Some(&2));
        assert_eq!(distribution.get("medium"), Some(&1));
        assert_eq!(distribution.get("hard"), Some(&1));
    }

    #[test]
    fn test_determine_size_category() {
        let exporter = SyntheticExporter::new("/tmp/export", "1.0.0");

        assert_eq!(exporter.determine_size_category(50), "n<1K");
        assert_eq!(exporter.determine_size_category(99), "n<1K");
        assert_eq!(exporter.determine_size_category(100), "1K<n<10K");
        assert_eq!(exporter.determine_size_category(999), "1K<n<10K");
        assert_eq!(exporter.determine_size_category(1000), "1K<n<10K");
        assert_eq!(exporter.determine_size_category(9999), "1K<n<10K");
        assert_eq!(exporter.determine_size_category(10000), "10K<n<100K");
        assert_eq!(exporter.determine_size_category(100000), "n>100K");
    }

    #[test]
    fn test_serialize_partial_credit() {
        let criteria = vec![
            PartialCreditItem::new("First criterion", 0.25),
            PartialCreditItem::new("Second criterion", 0.5),
        ];

        let json = serialize_partial_credit(&criteria);
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&json).expect("should parse JSON");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["criterion"], "First criterion");
        assert_eq!(parsed[0]["points"], 0.25);
        assert_eq!(parsed[1]["criterion"], "Second criterion");
        assert_eq!(parsed[1]["points"], 0.5);
    }

    #[test]
    fn test_serialize_automated_checks() {
        let checks = vec![
            AutomatedCheck::file_exists("/tmp/file.txt"),
            AutomatedCheck::output_contains("cmd", "pattern"),
            AutomatedCheck::exit_code("test.sh", 0),
        ];

        let json = serialize_automated_checks(&checks);
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&json).expect("should parse JSON");

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["check_type"], "FileExists");
        assert_eq!(parsed[0]["target"], "/tmp/file.txt");
        assert_eq!(parsed[1]["check_type"], "OutputContains");
        assert_eq!(parsed[2]["check_type"], "ExitCode");
    }

    #[test]
    fn test_synthetic_dataset_entry_serialization() {
        let entry = SyntheticDatasetEntry {
            id: "task-001".to_string(),
            category: "debugging".to_string(),
            subcategory: "log-analysis".to_string(),
            problem_statement: "Find errors in logs".to_string(),
            difficulty: "medium".to_string(),
            required_skills: "grep, awk".to_string(),
            tags: "debugging, logs".to_string(),
            success_criteria: r#"["criterion1"]"#.to_string(),
            partial_credit: r#"[{"criterion": "test", "points": 0.5}]"#.to_string(),
            automated_checks: r#"[]"#.to_string(),
            canary_token: "CANARY_123".to_string(),
            version: "1.0.0".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&entry).expect("should serialize");
        let parsed: SyntheticDatasetEntry =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(parsed.id, entry.id);
        assert_eq!(parsed.category, entry.category);
        assert_eq!(parsed.problem_statement, entry.problem_statement);
    }

    #[test]
    fn test_synthetic_solution_entry_serialization() {
        let solution = SyntheticSolutionEntry {
            task_id: "task-001".to_string(),
            approach: "Use grep".to_string(),
            key_insights: "insight1, insight2".to_string(),
            reference_commands: "grep pattern file".to_string(),
            expected_time_seconds: 120,
            step_count: 3,
        };

        let json = serde_json::to_string(&solution).expect("should serialize");
        let parsed: SyntheticSolutionEntry =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(parsed.task_id, solution.task_id);
        assert_eq!(parsed.approach, solution.approach);
        assert_eq!(parsed.expected_time_seconds, 120);
    }
}
