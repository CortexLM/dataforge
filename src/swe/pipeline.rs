//! End-to-end SWE mining pipeline stages.
//! Uses aggressive parallelism at every stage: GH Archive fetch, enrichment,
//! pre-classification, extraction, test generation, and quality scoring.
//! Tasks and tests are exported to disk in real-time as they are accepted.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

        let total_before_filter = events.len();
        events.retain(|e| e.action.to_lowercase() == "merged");
        tracing::info!(
            total_raw = total_before_filter,
            merged_events = events.len(),
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
        let enrich_sem = Arc::new(Semaphore::new(5));
        let preclassify_sem = Arc::new(Semaphore::new(15));
        let deep_sem = Arc::new(Semaphore::new(5));

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

        let mut pool: FuturesUnordered<_> = events
            .into_iter()
            .map(|event| {
                let enrich_sem = enrich_sem.clone();
                let preclassify_sem = preclassify_sem.clone();
                let deep_sem = deep_sem.clone();
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
                            Ok(e) => e,
                            Err(_) => return,
                        }
                    };

                    if enriched.title == "Untitled change" || enriched.merge_sha.is_empty() {
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
                    );
                    filtered_count.fetch_add(1, Ordering::Relaxed);
                    if !filter_result.accepted {
                        return;
                    }

                    if dt.is_none() && completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }
                    if dt.is_some() && all_targets_met(&*per_diff.lock().await, &dt) && once {
                        return;
                    }

                    // --- Stage 3: Pre-classify difficulty ---
                    // Check cache first to avoid redundant LLM calls
                    let cached_triage = cache.triage_difficulty(
                        &enriched.repository, enriched.number
                    ).await;

                    let triage_result: Option<String> = if let Some(cached) = cached_triage {
                        tracing::debug!(
                            repo = %enriched.repository, pr = enriched.number,
                            triage = %cached, "Using cached classification"
                        );
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

                    let patch = match extractor.extract_patch(&PatchExtractionInput {
                        repository: &enriched.repository,
                        pull_number: enriched.number,
                        files_changed: enriched.files_changed,
                        language: &enriched.language,
                        title: &enriched.title,
                        base_commit: Some(&enriched.base_sha),
                        merge_commit: Some(&enriched.merge_sha),
                    }).await {
                        Ok(p) => p,
                        Err(err) => {
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
                            task.prompt = format!(
                                "{repo} (#{pr}): {title}\n\n{rewritten}",
                                repo = enriched.repository,
                                pr = enriched.number,
                                title = enriched.title,
                            );
                        }
                        Err(err) => {
                            tracing::warn!(task_id = %task.id, error = %err, "Prompt rewrite failed");
                            return;
                        }
                    }

                    task.meta
                        .insert("pr_title".to_string(), enriched.title.clone());

                    if !task.has_tests() {
                        let language = task.language.clone();
                        if let Err(err) = test_generator.ensure_tests(&mut task, &language).await {
                            tracing::warn!(task_id = %task.id, error = %err, "Test generation failed");
                            return;
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

                    let (score, passed) = (assessment.score, assessment.passed);
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
                let all_met = targets.targets.iter().all(|(level, &quota)| {
                    counts.get(level).copied().unwrap_or(0) >= quota
                });
                if all_met && once {
                    tracing::info!("All difficulty targets met, stopping pool");
                    break;
                }
            } else if completed.load(Ordering::Relaxed) >= max_tasks && once {
                tracing::info!("Reached max_tasks={}, stopping pool", max_tasks);
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
                fs::write(tests_dir.join(&unique_name), &tf.content)?;
            }
        }
    }

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
