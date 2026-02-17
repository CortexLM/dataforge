//! Orchestrator glue for SWE mining end-to-end.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::export::{DatasetConfig, DatasetManager, HfUploadConfig};
use crate::llm::LlmProvider;
use crate::swe::pipeline::{BenchmarkMetrics, DatasetHandle, ExportConfig, SwePipelineConfig};
use crate::swe::progress::{ProgressCounters, ProgressMonitor};
use crate::swe::{SwePipelineRunResult, SweTask};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweRunResult {
    pub tasks: Vec<SweTask>,
    pub attempted: usize,
    pub passed: usize,
    pub skipped: usize,
    pub finished_at: String,
    pub benchmark_metrics: Option<BenchmarkMetrics>,
}

/// Per-difficulty quotas for multi-level mining in a single pipeline run.
/// e.g. { "easy": 50, "medium": 50, "hard": 50 }
#[derive(Debug, Clone, Default)]
pub struct DifficultyTargets {
    pub targets: HashMap<String, usize>,
}

impl DifficultyTargets {
    /// Parse from a string like "easy:50,medium:50,hard:50".
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let mut targets = HashMap::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (level, count) = part.split_once(':').ok_or_else(|| {
                anyhow::anyhow!(
                    "Invalid difficulty target '{}'. Expected format: easy:50,medium:50,hard:50",
                    part
                )
            })?;
            let level = level.trim().to_lowercase();
            if !matches!(level.as_str(), "easy" | "medium" | "hard") {
                anyhow::bail!(
                    "Unknown difficulty level '{}'. Use easy, medium, or hard.",
                    level
                );
            }
            let count: usize = count.trim().parse().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid count '{}' for difficulty '{}'",
                    count.trim(),
                    level
                )
            })?;
            targets.insert(level, count);
        }
        if targets.is_empty() {
            anyhow::bail!(
                "No valid difficulty targets found. Use format: easy:50,medium:50,hard:50"
            );
        }
        Ok(Self { targets })
    }

    pub fn total_tasks(&self) -> usize {
        self.targets.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct SweOrchestratorConfig {
    pub output_dir: String,
    pub min_stars: u32,
    pub languages: Vec<String>,
    pub max_tasks: usize,
    pub once: bool,
    pub validate_docker: bool,
    pub skip_prs: HashSet<(String, u64)>,
    pub pr_file: Option<String>,
    pub difficulty_filter: Option<String>,
    /// Multi-difficulty quotas. When set, overrides difficulty_filter and max_tasks.
    pub difficulty_targets: Option<DifficultyTargets>,
    /// HuggingFace upload config. When set, tasks are uploaded in real-time as parquet.
    pub hf_upload: Option<HfUploadConfig>,
    /// SQLite PR cache for deduplication and triage caching.
    pub cache: super::OptionalCache,
    /// Override Docker image for mining containers (auto-select by language if None).
    pub mining_image: Option<String>,
    /// Enable pre-export workspace validation (default: true).
    pub validate_workspace: bool,
    /// Override enrichment concurrency.
    pub concurrency_enrich: Option<usize>,
    /// Override deep processing concurrency.
    pub concurrency_deep: Option<usize>,
}

impl Default for SweOrchestratorConfig {
    fn default() -> Self {
        Self {
            output_dir: crate::swe::DEFAULT_SWE_OUTPUT_DIR.to_string(),
            min_stars: 20,
            languages: Vec::new(),
            max_tasks: 1,
            once: true,
            validate_docker: false,
            skip_prs: HashSet::new(),
            pr_file: None,
            difficulty_filter: None,
            difficulty_targets: None,
            hf_upload: None,
            cache: super::OptionalCache::none(),
            mining_image: None,
            validate_workspace: true,
            concurrency_enrich: None,
            concurrency_deep: None,
        }
    }
}

pub struct SweOrchestrator {
    llm: Arc<dyn LlmProvider>,
    config: SweOrchestratorConfig,
}

impl SweOrchestrator {
    pub fn new(llm: Arc<dyn LlmProvider>, config: SweOrchestratorConfig) -> Self {
        Self { llm, config }
    }

    pub async fn mine(&self) -> anyhow::Result<SweRunResult> {
        let is_multi = self.config.difficulty_targets.is_some();

        let (max_tasks, candidate_multiplier) =
            if let Some(ref targets) = self.config.difficulty_targets {
                let total = targets.total_tasks();
                let has_hard = targets.targets.contains_key("hard");
                let mult = if has_hard { 200 } else { 100 };
                tracing::info!(?targets, total, "Starting multi-difficulty mining");
                (total, mult)
            } else if self.config.difficulty_filter.as_deref() == Some("hard") {
                (self.config.max_tasks, 200)
            } else if self.config.difficulty_filter.is_some() {
                (self.config.max_tasks, 100)
            } else {
                (self.config.max_tasks, 50)
            };

        let pipeline_config = SwePipelineConfig {
            min_stars: self.config.min_stars,
            languages: self.config.languages.clone(),
            max_candidates: max_tasks.saturating_mul(candidate_multiplier).max(10),
            max_tasks,
            once: self.config.once,
            validate_docker: self.config.validate_docker,
            skip_prs: self.config.skip_prs.clone(),
            difficulty_filter: if is_multi {
                None
            } else {
                self.config.difficulty_filter.clone()
            },
            difficulty_targets: self.config.difficulty_targets.clone(),
            cache: self.config.cache.clone(),
            mining_image: self.config.mining_image.clone(),
            validate_workspace: self.config.validate_workspace,
            concurrency_enrich: self.config.concurrency_enrich,
            concurrency_deep: self.config.concurrency_deep,
        };

        // Real-time export config: tasks are written to disk inside the pipeline worker loop
        let export_config = Arc::new(ExportConfig {
            output_dir: self.config.output_dir.clone(),
            pr_file: self.config.pr_file.clone(),
            per_difficulty_dirs: is_multi,
        });

        fs::create_dir_all(&self.config.output_dir)?;

        // Create dataset manager for parquet + optional HF upload
        let dataset_handle: Option<DatasetHandle> = {
            let ds_config = DatasetConfig {
                output_dir: std::path::PathBuf::from(&self.config.output_dir),
                hf_config: self.config.hf_upload.clone(),
                shard_size: 50,
                dataset_name: "SWE-Forge Benchmark".to_string(),
            };
            Some(Arc::new(DatasetManager::new(ds_config).await?))
        };

        // Start background progress monitor (logs every 30 seconds)
        let progress_counters = ProgressCounters::new();
        let monitor = ProgressMonitor::start(progress_counters, max_tasks, Duration::from_secs(30));

        let pipeline = crate::swe::pipeline::SwePipeline::new(&pipeline_config, self.llm.clone())?;
        let run: SwePipelineRunResult = pipeline
            .run_full(
                &pipeline_config,
                None,
                Some(export_config),
                dataset_handle.clone(),
            )
            .await?;

        monitor.stop().await;

        // Finalize dataset: flush remaining shard, write combined parquet, upload splits
        if let Some(ref ds) = dataset_handle {
            match ds.finalize().await {
                Ok(summary) => {
                    tracing::info!(
                        total = summary.total_tasks,
                        shards = summary.shard_count,
                        hf = ?summary.hf_repo,
                        "Dataset finalized"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Dataset finalize failed");
                }
            }
        }

        let tasks = run.tasks;
        let passed = tasks.iter().filter(|t| t.quality_passed).count();

        if is_multi {
            let mut per_level: HashMap<String, usize> = HashMap::new();
            for task in &tasks {
                if task.quality_passed {
                    let level = task
                        .meta
                        .get("difficulty")
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    *per_level.entry(level).or_insert(0) += 1;
                }
            }
            for (level, count) in &per_level {
                tracing::info!(level = %level, count = count, "Tasks exported for difficulty level");
            }
        }

        let skipped = tasks.len().saturating_sub(passed);
        Ok(SweRunResult {
            attempted: run.scored,
            tasks,
            passed,
            skipped,
            finished_at: run.finished_at.to_rfc3339(),
            benchmark_metrics: run.benchmark_metrics,
        })
    }
}
