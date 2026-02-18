//! End-to-end SWE mining pipeline stages.
//! Uses aggressive parallelism at every stage: GH Archive fetch, enrichment,
//! pre-classification, extraction, test generation, and quality scoring.
//! Tasks and tests are exported to disk in real-time as they are accepted.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::{Mutex, Semaphore};

use crate::llm::LlmProvider;
use crate::swe::{
    enricher::{EnrichedPullRequest, PullRequestEnricher},
    extractor::{PatchExtractionInput, PatchExtractor, PatchExtractorConfig},
    filters::SweepFilter,
    gharchive::GhArchiveClient,
    orchestrator::DifficultyTargets,
    quality::{QualityConfig, QualityScorer},
    test_generator::TestGenerator,
    SweTask,
};

/// Configuration for real-time export of tasks to disk as they are accepted.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Base output directory.
    pub output_dir: String,
    /// JSONL file to append processed PRs to.
    pub pr_file: Option<String>,
    /// When true and difficulty_targets is set, export into per-difficulty subdirectories.
    pub per_difficulty_dirs: bool,
}

/// Optional dataset manager handle for real-time parquet + HF upload.
/// Wrapped in Arc so it can be shared across async tasks.
pub type DatasetHandle = Arc<crate::export::DatasetManager>;

/// Aggregate metrics collected during a full pipeline run for benchmarking analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub total_raw_events: usize,
    pub total_merged_events: usize,
    pub total_prefiltered: usize,
    pub enriched_count: usize,
    pub enrichment_failed: usize,
    pub filter_passed: usize,
    pub filter_rejected: usize,
    pub filter_rejection_reasons: HashMap<String, usize>,
    pub preclassify_count: usize,
    pub preclassify_easy: usize,
    pub preclassify_medium: usize,
    pub preclassify_hard: usize,
    pub extraction_attempted: usize,
    pub extraction_succeeded: usize,
    pub extraction_failed: usize,
    pub test_gen_attempted: usize,
    pub test_gen_succeeded: usize,
    pub test_gen_failed: usize,
    pub quality_scored: usize,
    pub quality_passed: usize,
    pub quality_failed: usize,
    pub difficulty_easy: usize,
    pub difficulty_medium: usize,
    pub difficulty_hard: usize,
    pub accepted_count: usize,
    pub validation_attempted: usize,
    pub validation_passed: usize,
    pub validation_failed: usize,
    pub total_processing_time_ms: u64,
    pub avg_per_pr_time_ms: f64,
    pub throughput_prs_per_sec: f64,
    pub avg_quality_score: f64,
    pub languages: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwePipelineEvent {
    CollectionStarted {
        requested: usize,
    },
    CandidateFiltered {
        event_id: String,
        accepted: bool,
        reasons: Vec<String>,
    },
    TaskExtracted {
        task_id: String,
    },
    TestGenerated {
        task_id: String,
    },
    QualityScored {
        task_id: String,
        score: f64,
        passed: bool,
    },
    WorkspaceValidated {
        task_id: String,
        passed: bool,
        reason: Option<String>,
    },
    PipelineCompleted {
        emitted: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SwePipelineConfig {
    pub min_stars: u32,
    pub languages: Vec<String>,
    pub max_candidates: usize,
    pub max_tasks: usize,
    pub once: bool,
    pub validate_docker: bool,
    pub skip_prs: HashSet<(String, u64)>,
    pub difficulty_filter: Option<String>,
    /// Per-difficulty quotas. When set, difficulty_filter is ignored and each
    /// difficulty level has its own independent quota.
    pub difficulty_targets: Option<DifficultyTargets>,
    /// SQLite PR cache for deduplication and triage caching.
    pub cache: super::OptionalCache,
    /// Override Docker image for mining containers (auto-select by language if None).
    pub mining_image: Option<String>,
    /// Enable pre-export workspace validation in a fresh Docker container.
    pub validate_workspace: bool,
    /// Override enrichment concurrency (default: 10).
    pub concurrency_enrich: Option<usize>,
    /// Override deep processing concurrency (default: 8).
    pub concurrency_deep: Option<usize>,
    /// Override pre-classification concurrency (default: 25).
    pub concurrency_preclassify: Option<usize>,
    /// Override deep processing backlog multiplier (default: 5).
    pub backlog_multiplier: Option<usize>,
}

impl Default for SwePipelineConfig {
    fn default() -> Self {
        Self {
            min_stars: 20,
            languages: vec![],
            max_candidates: 50,
            max_tasks: 1,
            once: true,
            validate_docker: false,
            skip_prs: HashSet::new(),
            difficulty_filter: None,
            difficulty_targets: None,
            cache: super::OptionalCache::none(),
            mining_image: None,
            validate_workspace: true,
            concurrency_enrich: None,
            concurrency_deep: None,
            concurrency_preclassify: None,
            backlog_multiplier: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwePipelineRunResult {
    pub tasks: Vec<SweTask>,
    pub filtered: usize,
    pub extracted: usize,
    pub scored: usize,
    pub finished_at: DateTime<Utc>,
    pub benchmark_metrics: Option<BenchmarkMetrics>,
}

pub struct SwePipeline {
    archive: GhArchiveClient,
    enricher: PullRequestEnricher,
    filter: SweepFilter,
    extractor: PatchExtractor,
    test_generator: TestGenerator,
    quality: QualityScorer,
    prompt_rewriter: super::PromptRewriter,
}

impl SwePipeline {
    pub fn new(config: &SwePipelineConfig, llm: Arc<dyn LlmProvider>) -> anyhow::Result<Self> {
        let archive = GhArchiveClient::new(None);
        let enricher = PullRequestEnricher::with_default()?;

        let mut filter_cfg = crate::swe::filters::FilterConfig::default();
        if !config.languages.is_empty() {
            filter_cfg.allowed_languages = config.languages.clone();
        }
        filter_cfg.min_stars = config.min_stars;
        let filter = crate::swe::filters::SweepFilter::new(filter_cfg);

        let extractor = PatchExtractor::new(PatchExtractorConfig {
            include_test_files: true,
            include_binary: false,
            require_real_extraction: true,
        });
        let test_generator = TestGenerator::with_image(llm.clone(), config.mining_image.clone());
        let quality = QualityScorer::new(llm.clone(), QualityConfig::default());
        let prompt_rewriter = super::PromptRewriter::new(llm);

        Ok(Self {
            archive,
            enricher,
            filter,
            extractor,
            test_generator,
            quality,
            prompt_rewriter,
        })
    }

    pub async fn run(
        &self,
        config: &SwePipelineConfig,
        event_tx: Option<Sender<SwePipelineEvent>>,
    ) -> anyhow::Result<SwePipelineRunResult> {
        self.run_with_export(config, event_tx, None).await
    }

    pub async fn run_with_export(
        &self,
        config: &SwePipelineConfig,
        event_tx: Option<Sender<SwePipelineEvent>>,
        export_config: Option<Arc<ExportConfig>>,
    ) -> anyhow::Result<SwePipelineRunResult> {
        self.run_full(config, event_tx, export_config, None).await
    }

    /// Full pipeline run with optional disk export and optional dataset (parquet + HF) manager.
    pub async fn run_full(
        &self,
        config: &SwePipelineConfig,
        event_tx: Option<Sender<SwePipelineEvent>>,
        export_config: Option<Arc<ExportConfig>>,
        dataset_handle: Option<DatasetHandle>,
    ) -> anyhow::Result<SwePipelineRunResult> {
        let pipeline_start = Instant::now();

        emit(
            &event_tx,
            SwePipelineEvent::CollectionStarted {
                requested: config.max_candidates,
            },
        )
        .await;

        // Cap hours_back to avoid downloading too much data
        let hours_back = ((config.max_candidates / 50) + 1).clamp(6, 12) as u32;
        let mut events = self.archive.fetch_events(hours_back).await?;

        let total_raw_events = events.len();
        events.retain(|e| e.action.to_lowercase() == "merged");
        let total_merged_events = events.len();
        tracing::info!(
            total_raw = total_raw_events,
            merged_events = total_merged_events,
            hours_back = hours_back,
            "GH Archive fetch complete, kept only merged PRs"
        );

        if events.is_empty() {
            anyhow::bail!("No merged PRs found in GH Archive data");
        }

        // Shuffle events for diversity, then truncate to candidates limit
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        events.shuffle(&mut rng);

        if config.max_candidates > 0 && events.len() > config.max_candidates {
            events.truncate(config.max_candidates);
        }

        // Pre-filter events using GH Archive data (no API calls needed)
        let before_prefilter = events.len();
        events.retain(|e| {
            // Must have a valid PR number
            if e.pull_number == 0 {
                return false;
            }
            // Skip already-processed PRs
            if config
                .skip_prs
                .contains(&(e.repository.clone(), e.pull_number))
            {
                return false;
            }
            // Exclude bots
            if e.actor.contains("[bot]") || e.actor == "dependabot" {
                return false;
            }
            // Prefer repos with an org (real projects, not personal forks)
            if !e.has_org {
                return false;
            }
            true
        });
        tracing::info!(
            before = before_prefilter,
            after = events.len(),
            "Pre-filtered events (excluded bots, non-org repos)"
        );

        // === POOL-BASED PIPELINE ===
        // Each event flows independently through: enrich -> filter -> pre-classify -> deep process.
        // Semaphores control concurrency at each stage. No chunk barriers.
        let deep_concurrency = config.concurrency_deep.unwrap_or(8);
        let enrich_sem = Arc::new(Semaphore::new(config.concurrency_enrich.unwrap_or(10)));
        let preclassify_sem =
            Arc::new(Semaphore::new(config.concurrency_preclassify.unwrap_or(25)));
        let deep_sem = Arc::new(Semaphore::new(deep_concurrency));
        // Backpressure: limit how many classified candidates can queue for deep processing.
        // Pre-classification blocks when this is full, preventing wasted LLM tokens.
        let deep_backlog_sem = Arc::new(Semaphore::new(
            deep_concurrency * config.backlog_multiplier.unwrap_or(5),
        ));
        let cancelled = Arc::new(AtomicBool::new(false));

        let enricher = &self.enricher;
        let filter = &self.filter;
        let quality = &self.quality;
        let extractor = &self.extractor;
        let test_generator = &self.test_generator;
        let prompt_rewriter = &self.prompt_rewriter;
        let difficulty_filter = config.difficulty_filter.clone();
        let cache = config.cache.clone();
        let difficulty_targets = config.difficulty_targets.clone();
        let max_tasks = config.max_tasks;
        let once = config.once;

        let completed = Arc::new(AtomicUsize::new(0));
        let tasks_mu: Arc<Mutex<Vec<SweTask>>> = Arc::new(Mutex::new(Vec::new()));
        let filtered_count = Arc::new(AtomicUsize::new(0));
        let extracted_count = Arc::new(AtomicUsize::new(0));
        let scored_count = Arc::new(AtomicUsize::new(0));
        // Per-difficulty completed counts for multi-target mode
        let per_difficulty_completed: Arc<Mutex<HashMap<String, usize>>> =
            Arc::new(Mutex::new(HashMap::new()));
        // Shared export config for real-time disk writes
        let export_cfg = export_config.clone();
        let ds_handle = dataset_handle.clone();

        let enriched_count_m = Arc::new(AtomicUsize::new(0));
        let enrichment_failed_m = Arc::new(AtomicUsize::new(0));
        let filter_passed_m = Arc::new(AtomicUsize::new(0));
        let filter_rejected_m = Arc::new(AtomicUsize::new(0));
        let filter_rejection_reasons_m: Arc<Mutex<HashMap<String, usize>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let preclassify_count_m = Arc::new(AtomicUsize::new(0));
        let preclassify_easy_m = Arc::new(AtomicUsize::new(0));
        let preclassify_medium_m = Arc::new(AtomicUsize::new(0));
        let preclassify_hard_m = Arc::new(AtomicUsize::new(0));
        let extraction_attempted_m = Arc::new(AtomicUsize::new(0));
        let extraction_succeeded_m = Arc::new(AtomicUsize::new(0));
        let extraction_failed_m = Arc::new(AtomicUsize::new(0));
        let test_gen_attempted_m = Arc::new(AtomicUsize::new(0));
        let test_gen_succeeded_m = Arc::new(AtomicUsize::new(0));
        let test_gen_failed_m = Arc::new(AtomicUsize::new(0));
        let quality_scored_m = Arc::new(AtomicUsize::new(0));
        let quality_passed_m = Arc::new(AtomicUsize::new(0));
        let quality_failed_m = Arc::new(AtomicUsize::new(0));
        let difficulty_easy_m = Arc::new(AtomicUsize::new(0));
        let difficulty_medium_m = Arc::new(AtomicUsize::new(0));
        let difficulty_hard_m = Arc::new(AtomicUsize::new(0));
        let accepted_count_m = Arc::new(AtomicUsize::new(0));
        let validation_attempted_m = Arc::new(AtomicUsize::new(0));
        let validation_passed_m = Arc::new(AtomicUsize::new(0));
        let validation_failed_m = Arc::new(AtomicUsize::new(0));
        let quality_scores_m: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
        let languages_m: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
        let validate_workspace = config.validate_workspace;

        let total_prefiltered = events.len();

        let mut pool: FuturesUnordered<_> = events
            .into_iter()
            .map(|event| {
                let enrich_sem = enrich_sem.clone();
                let preclassify_sem = preclassify_sem.clone();
                let deep_sem = deep_sem.clone();
                let deep_backlog_sem = deep_backlog_sem.clone();
                let df = difficulty_filter.clone();
                let dt = difficulty_targets.clone();
                let completed = completed.clone();
                let tasks_mu = tasks_mu.clone();
                let filtered_count = filtered_count.clone();
                let extracted_count = extracted_count.clone();
                let scored_count = scored_count.clone();
                let per_diff = per_difficulty_completed.clone();
                let export_cfg = export_cfg.clone();
                let ds_handle = ds_handle.clone();
                let cache = cache.clone();
                let enriched_count_m = enriched_count_m.clone();
                let enrichment_failed_m = enrichment_failed_m.clone();
                let filter_passed_m = filter_passed_m.clone();
                let filter_rejected_m = filter_rejected_m.clone();
                let filter_rejection_reasons_m = filter_rejection_reasons_m.clone();
                let preclassify_count_m = preclassify_count_m.clone();
                let preclassify_easy_m = preclassify_easy_m.clone();
                let preclassify_medium_m = preclassify_medium_m.clone();
                let preclassify_hard_m = preclassify_hard_m.clone();
                let extraction_attempted_m = extraction_attempted_m.clone();
                let extraction_succeeded_m = extraction_succeeded_m.clone();
                let extraction_failed_m = extraction_failed_m.clone();
                let test_gen_attempted_m = test_gen_attempted_m.clone();
                let test_gen_succeeded_m = test_gen_succeeded_m.clone();
                let test_gen_failed_m = test_gen_failed_m.clone();
                let quality_scored_m = quality_scored_m.clone();
                let quality_passed_m = quality_passed_m.clone();
                let quality_failed_m = quality_failed_m.clone();
                let difficulty_easy_m = difficulty_easy_m.clone();
                let difficulty_medium_m = difficulty_medium_m.clone();
                let difficulty_hard_m = difficulty_hard_m.clone();
                let accepted_count_m = accepted_count_m.clone();
                let validation_attempted_m = validation_attempted_m.clone();
                let validation_passed_m = validation_passed_m.clone();
                let validation_failed_m = validation_failed_m.clone();
                let quality_scores_m = quality_scores_m.clone();
                let languages_m = languages_m.clone();
                let cancelled = cancelled.clone();
                let mining_image = config.mining_image.clone();
                async move {
                    // Helper: check if all quotas are met (multi-target mode)
                    let all_targets_met = |per_diff: &HashMap<String, usize>, dt: &Option<DifficultyTargets>| -> bool {
                        match dt {
                            Some(ref targets) => targets.targets.iter().all(|(level, &quota)| {
                                per_diff.get(level).copied().unwrap_or(0) >= quota
                            }),
                            None => false,
                        }
                    };

                    // Early exit: cancellation token
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }

                    // Early exit: single-difficulty mode
                    if dt.is_none() && completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }
                    // Early exit: multi-difficulty mode
                    if dt.is_some() && all_targets_met(&*per_diff.lock().await, &dt) && once {
                        return;
                    }

                    // --- Cache check: skip already processed PRs ---
                    if cache.should_skip(&event.repository, event.pull_number).await {
                        tracing::debug!(
                            repo = %event.repository, pr = event.pull_number,
                            "Skipped by PR cache (already exported/rejected)"
                        );
                        return;
                    }

                    // --- Stage 1: Enrich ---
                    let enriched = {
                        let _permit = enrich_sem.acquire().await.unwrap();
                        match enricher.enrich(&event).await {
                            Ok(e) => {
                                enriched_count_m.fetch_add(1, Ordering::Relaxed);
                                e
                            }
                            Err(_) => {
                                enrichment_failed_m.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                        }
                    };

                    if enriched.title == "Untitled change" || enriched.merge_sha.is_empty() {
                        return;
                    }

                    // Reject PRs with no code changes
                    if enriched.files_changed == 0 || (enriched.added_lines == 0 && enriched.removed_lines == 0) {
                        tracing::debug!(
                            repo = %enriched.repository, pr = enriched.number,
                            "Skipped: no code changes (files={}, added={}, removed={})",
                            enriched.files_changed, enriched.added_lines, enriched.removed_lines,
                        );
                        return;
                    }

                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }

                    // Save enriched data to cache
                    let _ = cache.upsert(&super::PrCacheEntry {
                        repo: enriched.repository.clone(),
                        pr_number: enriched.number,
                        actor: Some(enriched.actor.clone()),
                        title: Some(enriched.title.clone()),
                        body: Some(enriched.body.clone()),
                        language: Some(enriched.language.clone()),
                        stars: Some(enriched.stars),
                        base_sha: Some(enriched.base_sha.clone()),
                        merge_sha: Some(enriched.merge_sha.clone()),
                        files_changed: Some(enriched.files_changed),
                        status: "enriched".to_string(),
                        ..Default::default()
                    }).await;

                    // --- Stage 2: Local filter ---
                    let added_lines = infer_added_lines(&enriched);
                    let filter_result = filter.keep_candidate(
                        &enriched.language,
                        enriched.stars,
                        enriched.files_changed,
                        added_lines,
                        &enriched.changed_files,
                        &enriched.title,
                        &enriched.body,
                    );
                    filtered_count.fetch_add(1, Ordering::Relaxed);
                    if filter_result.accepted {
                        filter_passed_m.fetch_add(1, Ordering::Relaxed);
                    } else {
                        filter_rejected_m.fetch_add(1, Ordering::Relaxed);
                        {
                            let mut reasons_map = filter_rejection_reasons_m.lock().await;
                            for reason in &filter_result.reasons {
                                let category = reason
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("unknown")
                                    .to_lowercase();
                                *reasons_map.entry(category).or_insert(0) += 1;
                            }
                        }
                        return;
                    }

                    if dt.is_none() && completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }
                    if dt.is_some() && all_targets_met(&*per_diff.lock().await, &dt) && once {
                        return;
                    }

                    // --- Stage 3: Pre-classify difficulty ---
                    // Acquire backpressure permit: blocks if deep processing queue is full,
                    // preventing pre-classification from racing far ahead.
                    let _backlog_permit = deep_backlog_sem.acquire().await.unwrap();
                    // Check cache first to avoid redundant LLM calls
                    let cached_triage = cache.triage_difficulty(
                        &enriched.repository, enriched.number
                    ).await;

                    let triage_result: Option<String> = if let Some(cached) = cached_triage {
                        tracing::debug!(
                            repo = %enriched.repository, pr = enriched.number,
                            triage = %cached, "Using cached classification"
                        );
                        preclassify_count_m.fetch_add(1, Ordering::Relaxed);
                        match cached.as_str() {
                            "easy" => { preclassify_easy_m.fetch_add(1, Ordering::Relaxed); }
                            "medium" => { preclassify_medium_m.fetch_add(1, Ordering::Relaxed); }
                            "hard" => { preclassify_hard_m.fetch_add(1, Ordering::Relaxed); }
                            _ => {}
                        }
                        Some(cached)
                    } else if dt.is_some() || df.is_some() {
                        let _permit = preclassify_sem.acquire().await.unwrap();
                        let filter_val = df.as_deref().unwrap_or("medium");
                        let classify_input = crate::swe::quality::ClassifyInput {
                            repo: &enriched.repository,
                            pr: enriched.number,
                            title: &enriched.title,
                            body: &enriched.body,
                            language: &enriched.language,
                            files_changed: enriched.files_changed,
                            added_lines: enriched.added_lines,
                            removed_lines: enriched.removed_lines,
                            changed_files: &enriched.changed_files,
                        };
                        match quality.classify(&classify_input, filter_val).await {
                            Ok(pre) => {
                                preclassify_count_m.fetch_add(1, Ordering::Relaxed);
                                match pre.difficulty.as_str() {
                                    "easy" => { preclassify_easy_m.fetch_add(1, Ordering::Relaxed); }
                                    "medium" => { preclassify_medium_m.fetch_add(1, Ordering::Relaxed); }
                                    "hard" => { preclassify_hard_m.fetch_add(1, Ordering::Relaxed); }
                                    _ => {}
                                }
                                // Save triage to cache
                                let _ = cache.upsert(&super::PrCacheEntry {
                                    repo: enriched.repository.clone(),
                                    pr_number: enriched.number,
                                    triage_difficulty: Some(pre.difficulty.clone()),
                                    status: "pre_classified".to_string(),
                                    ..Default::default()
                                }).await;
                                Some(pre.difficulty)
                            }
                            Err(_) => None, // on error, let it through
                        }
                    } else {
                        None
                    };

                    // Apply difficulty filter using triage result
                    if let Some(ref triage) = triage_result {
                        if dt.is_some() {
                            let counts = per_diff.lock().await;
                            if let Some(ref targets) = dt {
                                let current = counts.get(triage).copied().unwrap_or(0);
                                let quota = targets.targets.get(triage).copied().unwrap_or(0);
                                if quota == 0 {
                                    tracing::debug!(
                                        repo = %enriched.repository, pr = enriched.number,
                                        triage = %triage, "Skipped: difficulty not in targets"
                                    );
                                    let _ = cache.mark_rejected(
                                        &enriched.repository, enriched.number,
                                        "difficulty not in targets",
                                    ).await;
                                    return;
                                }
                                if current >= quota {
                                    tracing::debug!(
                                        repo = %enriched.repository, pr = enriched.number,
                                        triage = %triage, current, quota,
                                        "Skipped: quota already met for this difficulty"
                                    );
                                    return;
                                }
                            }
                            tracing::info!(
                                repo = %enriched.repository, pr = enriched.number,
                                triage = %triage, "Pre-classification (multi-target): ACCEPTED"
                            );
                        } else if let Some(ref df_val) = df {
                            if triage != df_val {
                                tracing::info!(
                                    repo = %enriched.repository, pr = enriched.number,
                                    triage_difficulty = %triage, filter = %df_val,
                                    skipped = true, "Pre-classification triage"
                                );
                                let _ = cache.mark_rejected(
                                    &enriched.repository, enriched.number,
                                    &format!("triage={}, filter={}", triage, df_val),
                                ).await;
                                return;
                            }
                            tracing::info!(
                                repo = %enriched.repository, pr = enriched.number,
                                triage = %triage, "Pre-classification: ACCEPTED"
                            );
                        }
                    }

                    if dt.is_none() && completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }
                    if dt.is_some() && all_targets_met(&*per_diff.lock().await, &dt) && once {
                        return;
                    }

                    // --- Stage 4: Deep processing (extraction + test gen + quality) ---
                    let _permit = deep_sem.acquire().await.unwrap();

                    if dt.is_none() && completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }
                    if dt.is_some() && all_targets_met(&*per_diff.lock().await, &dt) && once {
                        return;
                    }

                    extraction_attempted_m.fetch_add(1, Ordering::Relaxed);
                    let patch = match extractor.extract_patch(&PatchExtractionInput {
                        repository: &enriched.repository,
                        pull_number: enriched.number,
                        files_changed: enriched.files_changed,
                        language: &enriched.language,
                        title: &enriched.title,
                        base_commit: Some(&enriched.base_sha),
                        merge_commit: Some(&enriched.merge_sha),
                    }).await {
                        Ok(p) => {
                            extraction_succeeded_m.fetch_add(1, Ordering::Relaxed);
                            p
                        }
                        Err(err) => {
                            extraction_failed_m.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!(repo = %enriched.repository, pr = enriched.number, error = %err, "Extraction failed");
                            return;
                        }
                    };

                    let mut task = SweTask::from_pull_request(
                        &enriched.repository,
                        enriched.number,
                        &enriched.language,
                        &enriched.base_sha,
                        &enriched.merge_sha,
                        &patch,
                    );

                    let raw_body = if enriched.body.is_empty() {
                        "(no description)"
                    } else {
                        &enriched.body
                    };
                    task.original_pr_body = format!(
                        "{repo} (#{pr}): {title}\n\n{body}",
                        repo = enriched.repository,
                        pr = enriched.number,
                        title = enriched.title,
                        body = raw_body,
                    );

                    match prompt_rewriter
                        .rewrite(
                            &enriched.repository,
                            enriched.number,
                            &enriched.title,
                            raw_body,
                        )
                        .await
                    {
                        Ok(rewritten) => {
                            task.prompt = rewritten;
                        }
                        Err(err) => {
                            tracing::warn!(task_id = %task.id, error = %err, "Prompt rewrite failed");
                            return;
                        }
                    }

                    task.meta
                        .insert("pr_title".to_string(), enriched.title.clone());

                    if !task.has_tests() {
                        test_gen_attempted_m.fetch_add(1, Ordering::Relaxed);
                        let language = task.language.clone();
                        match test_generator.ensure_tests(&mut task, &language).await {
                            Ok(_) => {
                                test_gen_succeeded_m.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(err) => {
                                test_gen_failed_m.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!(task_id = %task.id, error = %err, "Test generation failed");
                                return;
                            }
                        }
                    }

                    let assessment = match quality.assess(&task).await {
                        Ok(a) => a,
                        Err(err) => {
                            tracing::warn!(task_id = %task.id, error = %err, "Quality assessment failed");
                            return;
                        }
                    };

                    scored_count.fetch_add(1, Ordering::Relaxed);
                    quality_scored_m.fetch_add(1, Ordering::Relaxed);

                    let (score, passed) = (assessment.score, assessment.passed);
                    quality_scores_m.lock().await.push(score);
                    if passed {
                        quality_passed_m.fetch_add(1, Ordering::Relaxed);
                    } else {
                        quality_failed_m.fetch_add(1, Ordering::Relaxed);
                    }
                    match assessment.difficulty_level.as_str() {
                        "easy" => { difficulty_easy_m.fetch_add(1, Ordering::Relaxed); }
                        "medium" => { difficulty_medium_m.fetch_add(1, Ordering::Relaxed); }
                        "hard" => { difficulty_hard_m.fetch_add(1, Ordering::Relaxed); }
                        _ => {}
                    }

                    task.quality_score = Some(score);
                    task.quality_passed = passed;
                    task.difficulty_score = match assessment.difficulty_level.as_str() {
                        "easy" => 1,
                        "medium" => 2,
                        "hard" => 3,
                        _ => 1,
                    };
                    task.meta.insert(
                        "difficulty".to_string(),
                        assessment.difficulty_level.clone(),
                    );

                    // Determine if this task's difficulty is accepted
                    let difficulty_ok = if let Some(ref targets) = dt {
                        // Multi-target mode: check per-difficulty quota
                        let counts = per_diff.lock().await;
                        let level = &assessment.difficulty_level;
                        let current = counts.get(level).copied().unwrap_or(0);
                        let quota = targets.targets.get(level).copied().unwrap_or(0);
                        quota > 0 && current < quota
                    } else {
                        // Single-difficulty filter mode
                        match df.as_deref() {
                            Some(f) => assessment.difficulty_level == f,
                            None => true,
                        }
                    };

                    tracing::info!(
                        task_id = %task.id,
                        difficulty = %assessment.difficulty_level,
                        score,
                        passed = passed && difficulty_ok,
                        "Task processed"
                    );

                    if passed && difficulty_ok {
                        // --- Pre-export workspace validation ---
                        if validate_workspace {
                            validation_attempted_m.fetch_add(1, Ordering::Relaxed);
                            let validator = crate::swe::workspace_validator::WorkspaceValidator::new(
                                mining_image.clone(),
                            );
                            match validator.validate(&task).await {
                                Ok(crate::swe::workspace_validator::ValidationOutcome::Passed) => {
                                    validation_passed_m.fetch_add(1, Ordering::Relaxed);
                                    tracing::info!(
                                        task_id = %task.id,
                                        "Workspace validation PASSED"
                                    );
                                }
                                Ok(crate::swe::workspace_validator::ValidationOutcome::Rejected { reason }) => {
                                    validation_failed_m.fetch_add(1, Ordering::Relaxed);
                                    tracing::warn!(
                                        task_id = %task.id,
                                        reason = %reason,
                                        "Workspace validation REJECTED"
                                    );
                                    let _ = cache.mark_rejected(
                                        &task.repo,
                                        task.id.rsplit('-').next()
                                            .and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
                                        &format!("validation: {}", reason),
                                    ).await;
                                    return;
                                }
                                Err(err) => {
                                    validation_failed_m.fetch_add(1, Ordering::Relaxed);
                                    tracing::warn!(
                                        task_id = %task.id,
                                        error = %err,
                                        "Workspace validation ERROR"
                                    );
                                    return;
                                }
                            }
                        }

                        accepted_count_m.fetch_add(1, Ordering::Relaxed);
                        {
                            let mut langs = languages_m.lock().await;
                            *langs.entry(task.language.clone()).or_insert(0) += 1;
                        }
                        task.status = crate::swe::SweTaskStatus::Ready;

                        if dt.is_some() {
                            // Multi-target: increment per-difficulty counter
                            let mut counts = per_diff.lock().await;
                            let level = assessment.difficulty_level.clone();
                            let current = counts.entry(level.clone()).or_insert(0);
                            *current += 1;
                            let new_count = *current;
                            drop(counts);

                            // Real-time export to disk
                            if let Some(ref ecfg) = export_cfg {
                                let out_dir = if ecfg.per_difficulty_dirs {
                                    format!("{}/{}-tasks", ecfg.output_dir, level)
                                } else {
                                    ecfg.output_dir.clone()
                                };
                                match export_task_to_disk(&task, &out_dir) {
                                    Ok(()) => {
                                        task.status = crate::swe::SweTaskStatus::Exported;
                                        task.workspace_path = Some(format!("{}/{}", out_dir, task.id));
                                        append_pr_to_file(&ecfg.pr_file, &task.repo, &task.id);
                                        let pr_num = task.id.rsplit('-').next()
                                            .and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                                        let _ = cache.mark_exported(&task.repo, pr_num).await;
                                        tracing::info!(
                                            task_id = %task.id,
                                            difficulty = %level,
                                            output = %out_dir,
                                            "Exported task to disk (real-time)"
                                        );
                                    }
                                    Err(err) => {
                                        tracing::warn!(task_id = %task.id, error = %err, "Real-time export failed");
                                    }
                                }
                            }

                            // Real-time parquet + HF upload
                            if let Some(ref ds) = ds_handle {
                                if let Err(e) = ds.add_task(task.clone()).await {
                                    tracing::warn!(error = %e, "Dataset manager add_task failed");
                                }
                            }

                            completed.fetch_add(1, Ordering::Relaxed);
                            extracted_count.fetch_add(1, Ordering::Relaxed);
                            tasks_mu.lock().await.push(task);
                            tracing::info!(
                                difficulty = %level,
                                count = new_count,
                                "Task accepted (multi-target)"
                            );
                        } else {
                            let prev = completed.fetch_add(1, Ordering::Relaxed);
                            if prev < max_tasks || !once {
                                // Real-time export to disk
                                if let Some(ref ecfg) = export_cfg {
                                    match export_task_to_disk(&task, &ecfg.output_dir) {
                                        Ok(()) => {
                                            task.status = crate::swe::SweTaskStatus::Exported;
                                            task.workspace_path = Some(format!("{}/{}", ecfg.output_dir, task.id));
                                            append_pr_to_file(&ecfg.pr_file, &task.repo, &task.id);
                                            let pr_num = task.id.rsplit('-').next()
                                                .and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                                            let _ = cache.mark_exported(&task.repo, pr_num).await;
                                            tracing::info!(
                                                task_id = %task.id,
                                                output = %ecfg.output_dir,
                                                "Exported task to disk (real-time)"
                                            );
                                        }
                                        Err(err) => {
                                            tracing::warn!(task_id = %task.id, error = %err, "Real-time export failed");
                                        }
                                    }
                                }

                                // Real-time parquet + HF upload
                                if let Some(ref ds) = ds_handle {
                                    if let Err(e) = ds.add_task(task.clone()).await {
                                        tracing::warn!(error = %e, "Dataset manager add_task failed");
                                    }
                                }

                                extracted_count.fetch_add(1, Ordering::Relaxed);
                                tasks_mu.lock().await.push(task);
                                tracing::info!(
                                    completed = prev + 1,
                                    max_tasks,
                                    "Task accepted into pool"
                                );
                            }
                        }
                    }
                }
            })
            .collect();

        while pool.next().await.is_some() {
            // Check completion: multi-target mode or single mode
            if let Some(ref targets) = difficulty_targets {
                let counts = per_difficulty_completed.lock().await;
                let all_met = targets
                    .targets
                    .iter()
                    .all(|(level, &quota)| counts.get(level).copied().unwrap_or(0) >= quota);
                if all_met && once {
                    tracing::info!("All difficulty targets met, stopping pool");
                    cancelled.store(true, Ordering::Relaxed);
                    break;
                }
            } else if completed.load(Ordering::Relaxed) >= max_tasks && once {
                tracing::info!("Reached max_tasks={}, stopping pool", max_tasks);
                cancelled.store(true, Ordering::Relaxed);
                break;
            }
        }

        config.cache.log_stats().await;

        let tasks = match Arc::try_unwrap(tasks_mu) {
            Ok(mu) => mu.into_inner(),
            Err(arc) => arc.lock().await.clone(),
        };
        let filtered_count = filtered_count.load(Ordering::Relaxed);
        let extracted = extracted_count.load(Ordering::Relaxed);
        let scored = scored_count.load(Ordering::Relaxed);

        let elapsed = pipeline_start.elapsed();
        let total_processing_time_ms = elapsed.as_millis() as u64;
        let enriched_total = enriched_count_m.load(Ordering::Relaxed);
        let avg_per_pr_time_ms = if enriched_total > 0 {
            total_processing_time_ms as f64 / enriched_total as f64
        } else {
            0.0
        };
        let elapsed_secs = elapsed.as_secs_f64();
        let throughput_prs_per_sec = if elapsed_secs > 0.0 {
            enriched_total as f64 / elapsed_secs
        } else {
            0.0
        };
        let quality_scores = quality_scores_m.lock().await;
        let avg_quality_score = if quality_scores.is_empty() {
            0.0
        } else {
            quality_scores.iter().sum::<f64>() / quality_scores.len() as f64
        };
        drop(quality_scores);

        let filter_rejection_reasons = match Arc::try_unwrap(filter_rejection_reasons_m) {
            Ok(mu) => mu.into_inner(),
            Err(arc) => arc.lock().await.clone(),
        };
        let languages = match Arc::try_unwrap(languages_m) {
            Ok(mu) => mu.into_inner(),
            Err(arc) => arc.lock().await.clone(),
        };

        let benchmark_metrics = BenchmarkMetrics {
            total_raw_events,
            total_merged_events,
            total_prefiltered,
            enriched_count: enriched_total,
            enrichment_failed: enrichment_failed_m.load(Ordering::Relaxed),
            filter_passed: filter_passed_m.load(Ordering::Relaxed),
            filter_rejected: filter_rejected_m.load(Ordering::Relaxed),
            filter_rejection_reasons,
            preclassify_count: preclassify_count_m.load(Ordering::Relaxed),
            preclassify_easy: preclassify_easy_m.load(Ordering::Relaxed),
            preclassify_medium: preclassify_medium_m.load(Ordering::Relaxed),
            preclassify_hard: preclassify_hard_m.load(Ordering::Relaxed),
            extraction_attempted: extraction_attempted_m.load(Ordering::Relaxed),
            extraction_succeeded: extraction_succeeded_m.load(Ordering::Relaxed),
            extraction_failed: extraction_failed_m.load(Ordering::Relaxed),
            test_gen_attempted: test_gen_attempted_m.load(Ordering::Relaxed),
            test_gen_succeeded: test_gen_succeeded_m.load(Ordering::Relaxed),
            test_gen_failed: test_gen_failed_m.load(Ordering::Relaxed),
            quality_scored: quality_scored_m.load(Ordering::Relaxed),
            quality_passed: quality_passed_m.load(Ordering::Relaxed),
            quality_failed: quality_failed_m.load(Ordering::Relaxed),
            difficulty_easy: difficulty_easy_m.load(Ordering::Relaxed),
            difficulty_medium: difficulty_medium_m.load(Ordering::Relaxed),
            difficulty_hard: difficulty_hard_m.load(Ordering::Relaxed),
            accepted_count: accepted_count_m.load(Ordering::Relaxed),
            validation_attempted: validation_attempted_m.load(Ordering::Relaxed),
            validation_passed: validation_passed_m.load(Ordering::Relaxed),
            validation_failed: validation_failed_m.load(Ordering::Relaxed),
            total_processing_time_ms,
            avg_per_pr_time_ms,
            throughput_prs_per_sec,
            avg_quality_score,
            languages,
        };

        emit(
            &event_tx,
            SwePipelineEvent::PipelineCompleted {
                emitted: tasks.len(),
            },
        )
        .await;

        Ok(SwePipelineRunResult {
            tasks,
            filtered: filtered_count,
            extracted,
            scored,
            finished_at: Utc::now(),
            benchmark_metrics: Some(benchmark_metrics),
        })
    }
}

fn infer_added_lines(pr: &EnrichedPullRequest) -> usize {
    pr.added_lines
}

async fn emit(tx: &Option<mpsc::Sender<SwePipelineEvent>>, event: SwePipelineEvent) {
    if let Some(sender) = tx {
        let _ = sender.send(event).await;
    }
}

fn export_task_to_disk(task: &SweTask, output_dir: &str) -> anyhow::Result<()> {
    let dir = Path::new(output_dir).join(&task.id);
    fs::create_dir_all(&dir)?;

    let prompt = format!("# {}\n\n{}\n", task.id, task.prompt);
    fs::write(dir.join("prompt.md"), prompt)?;

    if !task.original_pr_body.is_empty() {
        let original = format!("# {} (original PR)\n\n{}\n", task.id, task.original_pr_body);
        fs::write(dir.join("original_pr.md"), original)?;
    }

    let workspace = serde_yaml::to_string(task)?;
    fs::write(dir.join("workspace.yaml"), workspace)?;

    let tests_dir = dir.join("tests");
    fs::create_dir_all(&tests_dir)?;

    let mut written_basenames: HashSet<String> = HashSet::new();

    if let Some(test_files_json) = task.meta.get("test_files") {
        if let Ok(files) =
            serde_json::from_str::<Vec<crate::swe::test_generator::TestFile>>(test_files_json)
        {
            let mut seen_names = std::collections::HashSet::new();
            for tf in &files {
                // Flatten to basename only -- avoid nested tests/tests/ duplication
                let basename = std::path::Path::new(&tf.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| tf.path.clone());
                let unique_name = if seen_names.contains(&basename) {
                    let prefixed = format!("{}_{}", seen_names.len(), basename);
                    seen_names.insert(prefixed.clone());
                    prefixed
                } else {
                    seen_names.insert(basename.clone());
                    basename
                };
                written_basenames.insert(unique_name.clone());
                fs::write(tests_dir.join(&unique_name), &tf.content)?;
            }
        }
    }

    validate_test_file_references(task, &written_basenames);

    for (i, cmd) in task.fail_to_pass.iter().enumerate() {
        let filename = format!("fail_to_pass_{}.sh", i + 1);
        fs::write(
            tests_dir.join(&filename),
            format!("#!/bin/bash\n# This test must FAIL on base commit, PASS after fix\n{cmd}\n"),
        )?;
    }

    for (i, cmd) in task.pass_to_pass.iter().enumerate() {
        let filename = format!("pass_to_pass_{}.sh", i + 1);
        fs::write(
            tests_dir.join(&filename),
            format!("#!/bin/bash\n# This test must PASS on base commit AND after fix\n{cmd}\n"),
        )?;
    }

    if !task.fail_to_pass.is_empty() {
        let checks = task
            .fail_to_pass
            .iter()
            .chain(task.pass_to_pass.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.join("checks.txt"), checks)?;
    }

    Ok(())
}

fn append_pr_to_file(pr_file: &Option<String>, repo: &str, task_id: &str) {
    let Some(path) = pr_file else { return };
    let pr_number: u64 = task_id
        .rsplit('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let line = serde_json::json!({"repo": repo, "pr": pr_number});
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

/// Extract test file paths referenced in a shell command string.
///
/// Recognises common patterns:
/// - `tests/foo.py`, `src/test_bar.ts` (path-like tokens ending in test-file extensions)
/// - `python -m unittest tests.module` (dotted module notation)
fn extract_test_paths_from_command(cmd: &str) -> Vec<String> {
    let test_extensions = [".py", ".ts", ".js", ".java", ".rs", ".go", ".rb", ".sh"];
    let mut paths = Vec::new();

    for token in cmd.split_whitespace() {
        let clean = token.trim_matches(|c: char| c == '\'' || c == '"' || c == ';');
        if clean.contains('*') || clean.contains('?') {
            continue;
        }
        if test_extensions.iter().any(|ext| clean.ends_with(ext)) {
            paths.push(clean.to_string());
        }
    }

    if cmd.contains("python -m unittest") || cmd.contains("python3 -m unittest") {
        for token in cmd.split_whitespace() {
            let clean = token.trim_matches(|c: char| c == '\'' || c == '"' || c == ';');
            if clean.contains('.') && !clean.starts_with('-') && !clean.contains('/') {
                let as_path = clean.replace('.', "/") + ".py";
                if !paths.contains(&as_path) {
                    paths.push(as_path);
                }
            }
        }
    }

    paths
}

/// Validate that test file paths referenced in `fail_to_pass` and `pass_to_pass`
/// commands correspond to files present in `meta.test_files`. Logs warnings for
/// any missing references so operators can fix the task before evaluation.
fn validate_test_file_references(task: &SweTask, written_basenames: &HashSet<String>) {
    let test_file_paths: HashSet<String> = task
        .meta
        .get("test_files")
        .and_then(|json| {
            serde_json::from_str::<Vec<crate::swe::test_generator::TestFile>>(json).ok()
        })
        .unwrap_or_default()
        .iter()
        .map(|tf| tf.path.clone())
        .collect();

    let all_cmds = task.fail_to_pass.iter().chain(task.pass_to_pass.iter());

    for cmd in all_cmds {
        let referenced = extract_test_paths_from_command(cmd);
        for ref_path in &referenced {
            let basename = std::path::Path::new(ref_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| ref_path.clone());

            let found_in_meta = test_file_paths.contains(ref_path)
                || test_file_paths.iter().any(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy() == basename)
                        .unwrap_or(false)
                });

            let found_on_disk = written_basenames.contains(&basename);

            if !found_in_meta && !found_on_disk {
                tracing::warn!(
                    task_id = %task.id,
                    command = %cmd,
                    missing_file = %ref_path,
                    "Test command references file not found in meta.test_files or exported tests"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swe::SweTask;

    #[test]
    fn extract_paths_pytest_single() {
        let cmd = "python -m pytest tests/test_foo.py -q";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["tests/test_foo.py"]);
    }

    #[test]
    fn extract_paths_pytest_multiple() {
        let cmd = "python -m pytest tests/test_a.py tests/test_b.py -q";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["tests/test_a.py", "tests/test_b.py"]);
    }

    #[test]
    fn extract_paths_unittest_dotted() {
        let cmd = "python -m unittest tests.test_reshape_tensor";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["tests/test_reshape_tensor.py"]);
    }

    #[test]
    fn extract_paths_jest_ts() {
        let cmd = "yarn workspace @studio/pkg test --testPathPattern Foo.test.ts";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["Foo.test.ts"]);
    }

    #[test]
    fn extract_paths_java() {
        let cmd = "javac *.java && java -cp .:app RectangleBehaviorTest.java";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["RectangleBehaviorTest.java"]);
    }

    #[test]
    fn extract_paths_cd_env_pytest() {
        let cmd = "cd subdir && PYTHONPATH=repo python -m pytest tests/test_x.py -q";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["tests/test_x.py"]);
    }

    #[test]
    fn extract_paths_no_test_files() {
        let cmd = "python -m compileall -q python-src";
        let paths = extract_test_paths_from_command(cmd);
        assert!(paths.is_empty());
    }

    #[test]
    fn extract_paths_vitest() {
        let cmd = "pnpm --filter landing exec vitest --run src/components/test.test.ts";
        let paths = extract_test_paths_from_command(cmd);
        assert_eq!(paths, vec!["src/components/test.test.ts"]);
    }

    #[test]
    fn validate_warns_on_missing_file() {
        let mut task = SweTask::new("test-task-1", "owner/repo");
        task.fail_to_pass = vec!["python -m pytest tests/test_missing.py -q".to_string()];
        task.meta.insert(
            "test_files".to_string(),
            serde_json::to_string(&vec![crate::swe::test_generator::TestFile {
                path: "tests/test_other.py".to_string(),
                content: "pass".to_string(),
            }])
            .unwrap(),
        );
        let written: HashSet<String> = ["test_other.py".to_string()].into_iter().collect();
        validate_test_file_references(&task, &written);
    }

    #[test]
    fn validate_no_warn_when_file_present() {
        let mut task = SweTask::new("test-task-2", "owner/repo");
        task.fail_to_pass = vec!["python -m pytest tests/test_foo.py -q".to_string()];
        task.meta.insert(
            "test_files".to_string(),
            serde_json::to_string(&vec![crate::swe::test_generator::TestFile {
                path: "tests/test_foo.py".to_string(),
                content: "pass".to_string(),
            }])
            .unwrap(),
        );
        let written: HashSet<String> = ["test_foo.py".to_string()].into_iter().collect();
        validate_test_file_references(&task, &written);
    }

    #[test]
    fn export_task_creates_expected_files() {
        let tmp = std::env::temp_dir().join("swe_forge_test_export");
        let _ = fs::remove_dir_all(&tmp);

        let mut task = SweTask::new("test-export-1", "owner/repo");
        task.prompt = "Fix the bug".to_string();
        task.fail_to_pass = vec!["python -m pytest tests/test_fix.py -q".to_string()];
        task.pass_to_pass = vec!["python -m compileall -q src".to_string()];
        task.meta.insert(
            "test_files".to_string(),
            serde_json::to_string(&vec![crate::swe::test_generator::TestFile {
                path: "tests/test_fix.py".to_string(),
                content: "import unittest\nclass T(unittest.TestCase):\n    def test_a(self): pass"
                    .to_string(),
            }])
            .unwrap(),
        );

        let result = export_task_to_disk(&task, tmp.to_str().unwrap());
        assert!(result.is_ok(), "export_task_to_disk failed: {:?}", result);

        let task_dir = tmp.join("test-export-1");
        assert!(task_dir.join("prompt.md").exists());
        assert!(task_dir.join("workspace.yaml").exists());
        assert!(task_dir.join("checks.txt").exists());
        assert!(task_dir.join("tests/test_fix.py").exists());
        assert!(task_dir.join("tests/fail_to_pass_1.sh").exists());
        assert!(task_dir.join("tests/pass_to_pass_1.sh").exists());

        let checks = fs::read_to_string(task_dir.join("checks.txt")).unwrap();
        assert!(checks.contains("python -m pytest tests/test_fix.py -q"));
        assert!(checks.contains("python -m compileall -q src"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn infer_added_lines_returns_pr_value() {
        let pr = EnrichedPullRequest {
            repository: "owner/repo".to_string(),
            number: 1,
            title: "test".to_string(),
            body: String::new(),
            base_sha: String::new(),
            merge_sha: String::new(),
            language: "python".to_string(),
            files_changed: 1,
            added_lines: 42,
            removed_lines: 10,
            changed_files: Vec::new(),
            stars: 100,
            issue_number: None,
            actor: String::new(),
            linked_issues: Vec::new(),
            metadata: HashMap::new(),
        };
        assert_eq!(infer_added_lines(&pr), 42);
    }
}
