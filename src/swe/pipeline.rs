//! End-to-end SWE mining pipeline stages.
//! Uses aggressive parallelism at every stage: GH Archive fetch, enrichment,
//! pre-classification, extraction, test generation, and quality scoring.

use std::collections::HashSet;
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
    quality::{QualityConfig, QualityScorer},
    test_generator::TestGenerator,
    SweTask,
};

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
        let test_generator = TestGenerator::new(llm.clone());
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
        let max_tasks = config.max_tasks;
        let once = config.once;

        let completed = Arc::new(AtomicUsize::new(0));
        let tasks_mu: Arc<Mutex<Vec<SweTask>>> = Arc::new(Mutex::new(Vec::new()));
        let filtered_count = Arc::new(AtomicUsize::new(0));
        let extracted_count = Arc::new(AtomicUsize::new(0));
        let scored_count = Arc::new(AtomicUsize::new(0));

        let mut pool: FuturesUnordered<_> = events
            .into_iter()
            .map(|event| {
                let enrich_sem = enrich_sem.clone();
                let preclassify_sem = preclassify_sem.clone();
                let deep_sem = deep_sem.clone();
                let df = difficulty_filter.clone();
                let completed = completed.clone();
                let tasks_mu = tasks_mu.clone();
                let filtered_count = filtered_count.clone();
                let extracted_count = extracted_count.clone();
                let scored_count = scored_count.clone();
                async move {
                    if completed.load(Ordering::Relaxed) >= max_tasks && once {
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

                    if completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }

                    // --- Stage 3: Pre-classify difficulty ---
                    if let Some(ref df_val) = df {
                        let _permit = preclassify_sem.acquire().await.unwrap();
                        match quality
                            .pre_classify(
                                &enriched.repository,
                                enriched.number,
                                &enriched.title,
                                &enriched.body,
                                df_val,
                            )
                            .await
                        {
                            Ok(pre) if pre.dominated_out => {
                                tracing::debug!(
                                    repo = %enriched.repository,
                                    pr = enriched.number,
                                    triage = %pre.difficulty,
                                    "Skipped by pre-classification"
                                );
                                return;
                            }
                            Ok(pre) => {
                                tracing::info!(
                                    repo = %enriched.repository,
                                    pr = enriched.number,
                                    triage = %pre.difficulty,
                                    "Pre-classification: ACCEPTED"
                                );
                            }
                            Err(_) => { /* on error, let it through */ }
                        }
                    }

                    if completed.load(Ordering::Relaxed) >= max_tasks && once {
                        return;
                    }

                    // --- Stage 4: Deep processing (extraction + test gen + quality) ---
                    let _permit = deep_sem.acquire().await.unwrap();

                    if completed.load(Ordering::Relaxed) >= max_tasks && once {
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
                    }) {
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
                    let difficulty_ok = match df.as_deref() {
                        Some(f) => assessment.difficulty_level == f,
                        None => true,
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
                        let prev = completed.fetch_add(1, Ordering::Relaxed);
                        if prev < max_tasks || !once {
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
            })
            .collect();

        while pool.next().await.is_some() {
            if completed.load(Ordering::Relaxed) >= max_tasks && once {
                tracing::info!("Reached max_tasks={}, stopping pool", max_tasks);
                break;
            }
        }

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
    (pr.title.len() + pr.body.len()) % 700 + 15
}

async fn emit(tx: &Option<mpsc::Sender<SwePipelineEvent>>, event: SwePipelineEvent) {
    if let Some(sender) = tx {
        let _ = sender.send(event).await;
    }
}
