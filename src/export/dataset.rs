//! Real-time dataset manager: accumulates tasks, writes Parquet splits,
//! and optionally uploads to HuggingFace as tasks arrive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::swe::SweTask;

use super::hf_uploader::{HfUploadConfig, HfUploader};
use super::parquet_writer;

/// Configuration for dataset export.
#[derive(Debug, Clone)]
pub struct DatasetConfig {
    pub output_dir: PathBuf,
    /// HuggingFace upload config. When set, uploads happen in real-time.
    pub hf_config: Option<HfUploadConfig>,
    /// Number of tasks to buffer before writing a parquet shard.
    pub shard_size: usize,
    /// Dataset name for the README.
    pub dataset_name: String,
}

impl Default for DatasetConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./generated-swe"),
            hf_config: None,
            shard_size: 50,
            dataset_name: "SWE-Forge Benchmark".to_string(),
        }
    }
}

struct ShardState {
    tasks: Vec<SweTask>,
    shard_index: usize,
    total_exported: usize,
    per_difficulty: HashMap<String, usize>,
}

/// Manages incremental dataset building with Parquet export and optional HF upload.
pub struct DatasetManager {
    config: DatasetConfig,
    state: Arc<Mutex<ShardState>>,
    uploader: Option<HfUploader>,
}

impl DatasetManager {
    pub async fn new(config: DatasetConfig) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.output_dir)?;

        let uploader = if let Some(ref hf_cfg) = config.hf_config {
            let u = HfUploader::new(hf_cfg.clone());
            u.ensure_repo_exists().await?;
            // Upload README / dataset card
            let card = Self::generate_dataset_card(&config.dataset_name, hf_cfg);
            u.upload_dataset_card(&card).await?;
            Some(u)
        } else {
            None
        };

        let state = Arc::new(Mutex::new(ShardState {
            tasks: Vec::new(),
            shard_index: 0,
            total_exported: 0,
            per_difficulty: HashMap::new(),
        }));

        Ok(Self {
            config,
            state,
            uploader,
        })
    }

    /// Add a single task. If the shard buffer is full, flushes to disk + HF.
    pub async fn add_task(&self, task: SweTask) -> anyhow::Result<()> {
        let should_flush;
        {
            let mut state = self.state.lock().await;
            let diff = task
                .meta
                .get("difficulty")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *state.per_difficulty.entry(diff).or_insert(0) += 1;
            state.tasks.push(task);
            state.total_exported += 1;
            should_flush = state.tasks.len() >= self.config.shard_size;
        }

        if should_flush {
            self.flush_shard().await?;
        }

        Ok(())
    }

    /// Flush buffered tasks to a parquet shard and optionally upload to HF.
    pub async fn flush_shard(&self) -> anyhow::Result<()> {
        let (tasks, shard_idx) = {
            let mut state = self.state.lock().await;
            if state.tasks.is_empty() {
                return Ok(());
            }
            let tasks = std::mem::take(&mut state.tasks);
            let idx = state.shard_index;
            state.shard_index += 1;
            (tasks, idx)
        };

        let filename = format!("shard-{:04}.parquet", shard_idx);
        let local_path = self.config.output_dir.join("data").join(&filename);
        std::fs::create_dir_all(local_path.parent().unwrap())?;

        parquet_writer::write_parquet(&tasks, &local_path)?;

        tracing::info!(
            shard = shard_idx,
            tasks = tasks.len(),
            path = %local_path.display(),
            "Flushed parquet shard to disk"
        );

        // Upload to HF
        if let Some(ref uploader) = self.uploader {
            let bytes = std::fs::read(&local_path)?;
            let hf_path = format!("data/{}", filename);
            let msg = format!("Add shard {} ({} tasks)", shard_idx, tasks.len());
            if let Err(e) = uploader.upload_file(&hf_path, &bytes, &msg).await {
                tracing::warn!(error = %e, "Failed to upload shard to HF (will retry on finalize)");
            }
        }

        Ok(())
    }

    /// Finalize: flush remaining tasks, write combined train.parquet, upload metadata.
    pub async fn finalize(&self) -> anyhow::Result<DatasetSummary> {
        // Flush any remaining buffered tasks
        self.flush_shard().await?;

        let state = self.state.lock().await;
        let total = state.total_exported;
        let per_difficulty = state.per_difficulty.clone();
        let shard_count = state.shard_index;
        drop(state);

        // Also write a combined train.parquet from all shards
        let data_dir = self.config.output_dir.join("data");
        if data_dir.exists() {
            let mut all_tasks = Vec::new();
            let mut entries: Vec<_> = std::fs::read_dir(&data_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| x == "parquet")
                        .unwrap_or(false)
                })
                .collect();
            entries.sort_by_key(|e| e.file_name());

            for entry in &entries {
                match parquet_writer::read_parquet(&entry.path()) {
                    Ok(tasks) => all_tasks.extend(tasks),
                    Err(e) => {
                        tracing::warn!(path = %entry.path().display(), error = %e, "Failed to read shard")
                    }
                }
            }

            if !all_tasks.is_empty() {
                let combined_path = self.config.output_dir.join("train.parquet");
                parquet_writer::write_parquet(&all_tasks, &combined_path)?;

                // Write per-difficulty splits
                let mut by_diff: HashMap<String, Vec<SweTask>> = HashMap::new();
                for task in all_tasks {
                    let d = task
                        .meta
                        .get("difficulty")
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    by_diff.entry(d).or_default().push(task);
                }
                for (diff, tasks) in &by_diff {
                    let split_path = self.config.output_dir.join(format!("{}.parquet", diff));
                    parquet_writer::write_parquet(tasks, &split_path)?;
                }

                // Upload combined + splits to HF
                if let Some(ref uploader) = self.uploader {
                    let combined_bytes =
                        std::fs::read(self.config.output_dir.join("train.parquet"))?;
                    let _ = uploader
                        .upload_file(
                            "train.parquet",
                            &combined_bytes,
                            "Add combined train.parquet",
                        )
                        .await;

                    for diff in by_diff.keys() {
                        let split_path = self.config.output_dir.join(format!("{}.parquet", diff));
                        if let Ok(bytes) = std::fs::read(&split_path) {
                            let _ = uploader
                                .upload_file(
                                    &format!("{}.parquet", diff),
                                    &bytes,
                                    &format!("Add {} split", diff),
                                )
                                .await;
                        }
                    }
                }
            }
        }

        // Upload task directories (workspace.yaml, prompt.md, tests/) to HF under tasks/
        if let Some(ref uploader) = self.uploader {
            let mut task_dirs = Vec::new();
            Self::find_task_dirs(&self.config.output_dir, &mut task_dirs);
            for task_dir in &task_dirs {
                let rel = task_dir
                    .strip_prefix(&self.config.output_dir)
                    .unwrap_or(task_dir);
                let task_id = rel
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                let repo_prefix = format!("tasks/{}", task_id);
                match uploader
                    .upload_directory(task_dir, &repo_prefix, &format!("Add task {}", task_id))
                    .await
                {
                    Ok(count) => {
                        tracing::info!(
                            task_id = %task_id,
                            files = count,
                            "Uploaded task directory to HF"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            task_id = %task_id,
                            error = %e,
                            "Failed to upload task directory to HF"
                        );
                    }
                }
            }
        }

        let summary = DatasetSummary {
            total_tasks: total,
            shard_count,
            per_difficulty,
            output_dir: self.config.output_dir.clone(),
            hf_repo: self.uploader.as_ref().map(|u| u.repo_url()),
        };

        tracing::info!(
            total = summary.total_tasks,
            shards = summary.shard_count,
            hf = ?summary.hf_repo,
            "Dataset finalized"
        );

        Ok(summary)
    }

    fn find_task_dirs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("workspace.yaml").exists() {
                        out.push(path);
                    } else {
                        Self::find_task_dirs(&path, out);
                    }
                }
            }
        }
    }

    fn generate_dataset_card(name: &str, hf_cfg: &HfUploadConfig) -> String {
        format!(
            r#"---
license: apache-2.0
task_categories:
  - text-generation
language:
  - en
tags:
  - swe-bench
  - benchmark
  - code
  - bug-fix
  - swe-forge
pretty_name: {name}
size_categories:
  - 1K<n<10K
dataset_info:
  features:
    - name: instance_id
      dtype: string
    - name: repo
      dtype: string
    - name: base_commit
      dtype: string
    - name: patch
      dtype: string
    - name: test_patch
      dtype: string
    - name: problem_statement
      dtype: string
    - name: hints_text
      dtype: string
    - name: created_at
      dtype: string
    - name: version
      dtype: string
    - name: FAIL_TO_PASS
      dtype: string
    - name: PASS_TO_PASS
      dtype: string
    - name: environment_setup_commit
      dtype: string
    - name: language
      dtype: string
    - name: difficulty
      dtype: string
    - name: difficulty_score
      dtype: uint8
    - name: quality_score
      dtype: float64
---

# {name}

SWE-bench compatible dataset generated by [swe-forge](https://github.com/CortexLM/swe-forge).

## Dataset Structure

This dataset follows the [SWE-bench](https://huggingface.co/datasets/princeton-nlp/SWE-bench) format
with additional fields for multi-language support, difficulty scoring, and quality metrics.

### Fields

| Field | Type | Description |
|---|---|---|
| `instance_id` | string | Unique task identifier (repo-pr format) |
| `repo` | string | GitHub repository (owner/name) |
| `base_commit` | string | Base commit SHA |
| `patch` | string | Gold solution patch (git diff) |
| `test_patch` | string | Test-only patch from the PR |
| `problem_statement` | string | LLM-rewritten issue description |
| `hints_text` | string | Original PR body / hints |
| `created_at` | string | Task creation timestamp |
| `version` | string | Version identifier |
| `FAIL_TO_PASS` | string | JSON list of tests that must pass after fix |
| `PASS_TO_PASS` | string | JSON list of regression tests |
| `environment_setup_commit` | string | Commit for environment setup |
| `language` | string | Primary programming language |
| `difficulty` | string | Difficulty level (easy/medium/hard) |
| `difficulty_score` | uint8 | Numeric difficulty (1=easy, 2=medium, 3=hard) |
| `quality_score` | float64 | Quality assessment score (0.0-1.0) |

### Splits

- `train.parquet` - All tasks combined
- `easy.parquet` - Easy difficulty tasks
- `medium.parquet` - Medium difficulty tasks
- `hard.parquet` - Hard difficulty tasks

## Usage

```python
from datasets import load_dataset

# Load all tasks
ds = load_dataset("{repo_id}")

# Load a specific difficulty
ds_hard = load_dataset("{repo_id}", data_files="hard.parquet")
```

## Evaluation

Use [swe-forge](https://github.com/CortexLM/swe-forge) to evaluate agents on this dataset:

```bash
swe-forge swe harness --input ./tasks --agent-dir ./my-agent
```

## License

Apache 2.0
"#,
            name = name,
            repo_id = hf_cfg.repo_id,
        )
    }
}

/// Summary returned after finalizing a dataset.
#[derive(Debug, Clone)]
pub struct DatasetSummary {
    pub total_tasks: usize,
    pub shard_count: usize,
    pub per_difficulty: HashMap<String, usize>,
    pub output_dir: PathBuf,
    pub hf_repo: Option<String>,
}

/// Load a dataset from a local parquet file or directory.
pub fn load_dataset(path: &Path) -> anyhow::Result<Vec<SweTask>> {
    if path.is_file() && path.extension().map(|e| e == "parquet").unwrap_or(false) {
        return parquet_writer::read_parquet(path);
    }

    if path.is_dir() {
        // Look for train.parquet first, then any parquet files
        let train = path.join("train.parquet");
        if train.exists() {
            return parquet_writer::read_parquet(&train);
        }

        let mut all_tasks = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "parquet")
                    .unwrap_or(false)
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let tasks = parquet_writer::read_parquet(&entry.path())?;
            all_tasks.extend(tasks);
        }
        return Ok(all_tasks);
    }

    anyhow::bail!(
        "Path is neither a parquet file nor a directory: {}",
        path.display()
    );
}

/// Download a dataset from HuggingFace and return the tasks.
pub async fn download_dataset(
    repo_id: &str,
    split: Option<&str>,
    output_dir: &Path,
) -> anyhow::Result<Vec<SweTask>> {
    let filename = match split {
        Some(s) => format!("{}.parquet", s),
        None => "train.parquet".to_string(),
    };

    let url = format!(
        "https://huggingface.co/datasets/{}/resolve/main/{}",
        repo_id, filename
    );

    tracing::info!(repo = repo_id, file = %filename, "Downloading dataset from HuggingFace");

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to download dataset from {} ({}): {}",
            url,
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }

    let bytes = resp.bytes().await?;

    std::fs::create_dir_all(output_dir)?;
    let local_path = output_dir.join(&filename);
    std::fs::write(&local_path, &bytes)?;

    tracing::info!(
        path = %local_path.display(),
        size = bytes.len(),
        "Dataset downloaded"
    );

    parquet_writer::read_parquet(&local_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_config_default() {
        let cfg = DatasetConfig::default();
        assert_eq!(cfg.shard_size, 50);
        assert!(cfg.hf_config.is_none());
    }

    #[test]
    fn test_load_nonexistent_path() {
        let result = load_dataset(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
