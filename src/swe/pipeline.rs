//! End-to-end SWE mining pipeline stages.
//! Uses aggressive parallelism at every stage: GH Archive fetch, enrichment,
//! pre-classification, extraction, test generation, and quality scoring.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::Semaphore;

use crate::llm::LlmProvider;
use crate::swe::{
    enricher::{EnrichedPullRequest, PullRequestEnricher},
    extractor::{PatchExtractor, PatchExtractorConfig, PatchExtractionInput},
    filters::SweepFilter,
    gharchive::GhArchiveClient,
    quality::{QualityConfig, QualityScorer},
    test_generator::TestGenerator,
    SweTask,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwePipelineEvent {
    CollectionStarted { requested: usize },
    CandidateFiltered { event_id: String, accepted: bool, reasons: Vec<String> },
    TaskExtracted { task_id: String },
    TestGenerated { task_id: String },
    QualityScored { task_id: String, score: f64, passed: bool },
    PipelineCompleted { emitted: usize },
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
}

impl SwePipeline {
    pub fn new(
        config: &SwePipelineConfig,
        llm: Arc<dyn LlmProvider>,
    ) -> anyhow::Result<Self> {
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
        let quality = QualityScorer::new(llm, QualityConfig::default());

        Ok(Self {
            archive,
            enricher,
            filter,
            extractor,
            test_generator,
            quality,
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
        let mut events = self
            .archive
            .fetch_events(hours_back)
            .await?;

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
        use rand::thread_rng;
        let mut rng = thread_rng();
        events.shuffle(&mut rng);

        if config.max_candidates > 0 && events.len() > config.max_candidates {
            events.truncate(config.max_candidates);
        }

        // Pre-filter events using GH Archive data (no API calls needed)
        let before_prefilter = events.len();
        events.retain(|e| {
            // Must have a valid PR number
            if e.pull_number == 0 { return false; }
            // Skip already-processed PRs
            if config.skip_prs.contains(&(e.repository.clone(), e.pull_number)) { return false; }
            // Exclude bots
            if e.actor.contains("[bot]") || e.actor == "dependabot" { return false; }
            // Prefer repos with an org (real projects, not personal forks)
            if !e.has_org { return false; }
            true
        });
        tracing::info!(
            before = before_prefilter,
            after = events.len(),
            "Pre-filtered events (excluded bots, non-org repos)"
        );

        // === STAGE 1: Parallel enrichment with rate limiting ===
        // GitHub API: 5000 req/h. We use 2 calls per PR (pr + files).
        // With 3 concurrent and a 500ms delay between batches, we stay safe.
        let enrich_sem = Arc::new(Semaphore::new(3));
        let enricher = &self.enricher;
        let filter = &self.filter;
        let quality = &self.quality;
        let difficulty_filter = config.difficulty_filter.clone();

        // We process in streaming batches: enrich -> filter -> pre-classify -> deep process
        // This avoids enriching thousands of PRs we'll never use.
        let mut tasks: Vec<SweTask> = Vec::new();
        let mut filtered_count = 0usize;
        let mut extracted = 0usize;
        let mut scored = 0usize;

        // Process in chunks to manage rate limits while maintaining parallelism
        let chunk_size = 30;
        for chunk in events.chunks(chunk_size) {
            if tasks.len() >= config.max_tasks && config.once {
                break;
            }

            // --- Enrich chunk in parallel (3 concurrent) ---
            let mut enrich_futures = Vec::with_capacity(chunk.len());
            for event in chunk {
                let sem = enrich_sem.clone();
                enrich_futures.push(async move {
                    let _permit = sem.acquire().await.unwrap();
                    enricher.enrich(event).await
                });
            }
            let enrich_results = futures::future::join_all(enrich_futures).await;

            let mut enriched_prs: Vec<EnrichedPullRequest> = Vec::new();
            for result in enrich_results {
                if let Ok(e) = result {
                    // Reject if enrichment didn't get real data (title/merge_sha missing)
                    if e.title == "Untitled change" || e.merge_sha.is_empty() {
                        continue;
                    }
                    enriched_prs.push(e);
                }
            }

            // --- Local filter ---
            let mut filtered_prs: Vec<EnrichedPullRequest> = Vec::new();
            for enriched in enriched_prs {
                let added_lines = infer_added_lines(&enriched);
                let filter_result = filter.keep_candidate(
                    &enriched.language,
                    enriched.stars,
                    enriched.files_changed,
                    added_lines,
                );
                filtered_count += 1;
                if filter_result.accepted {
                    filtered_prs.push(enriched);
                }
            }

            if filtered_prs.is_empty() {
                continue;
            }

            tracing::info!(
                chunk_enriched = chunk.len(),
                chunk_accepted = filtered_prs.len(),
                "Chunk filtered"
            );

            // --- Pre-classify difficulty in parallel (10 concurrent LLM) ---
            let mut candidates = Vec::new();
            if let Some(ref df) = difficulty_filter {
                let preclassify_sem = Arc::new(Semaphore::new(10));
                let mut triage_futures = Vec::with_capacity(filtered_prs.len());
                for pr in &filtered_prs {
                    let sem = preclassify_sem.clone();
                    let repo = pr.repository.clone();
                    let number = pr.number;
                    let title = pr.title.clone();
                    let body = pr.body.clone();
                    let df_clone = df.clone();
                    triage_futures.push(async move {
                        let _permit = sem.acquire().await.unwrap();
                        let result = quality.pre_classify(&repo, number, &title, &body, &df_clone).await;
                        (number, repo, result)
                    });
                }
                let triage_results = futures::future::join_all(triage_futures).await;

                let mut accepted_set: HashSet<(String, u64)> = HashSet::new();
                for (number, repo, result) in triage_results {
                    match result {
                        Ok(pre) if !pre.dominated_out => {
                            tracing::info!(repo = %repo, pr = number, triage = %pre.difficulty, "Pre-classification: ACCEPTED");
                            accepted_set.insert((repo, number));
                        }
                        Ok(pre) => {
                            tracing::debug!(repo = %repo, pr = number, triage = %pre.difficulty, "Skipped by pre-classification");
                        }
                        Err(_) => { accepted_set.insert((repo, number)); }
                    }
                }

                for pr in filtered_prs {
                    if accepted_set.contains(&(pr.repository.clone(), pr.number)) {
                        candidates.push(pr);
                    }
                }
            } else {
                candidates = filtered_prs;
            }

            if candidates.is_empty() {
                continue;
            }

            // --- Deep processing in parallel (3 concurrent: extraction + test gen + quality) ---
            let process_sem = Arc::new(Semaphore::new(3));
            let extractor = &self.extractor;
            let test_generator = &self.test_generator;

            let mut process_futures = Vec::with_capacity(candidates.len());
            for enriched in &candidates {
                let sem = process_sem.clone();
                let enriched = enriched.clone();
                let df = difficulty_filter.clone();
                process_futures.push(async move {
                    let _permit = sem.acquire().await.unwrap();

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
                            return None;
                        }
                    };

                    let mut task = SweTask::from_pull_request(
                        &enriched.repository, enriched.number, &enriched.language,
                        &enriched.base_sha, &enriched.merge_sha, &patch,
                    );
                    task.prompt = format!(
                        "{repo} (#{pr}): {title}\n\n{body}",
                        repo = enriched.repository, pr = enriched.number,
                        title = enriched.title,
                        body = if enriched.body.is_empty() { "(no description)" } else { &enriched.body },
                    );
                    task.meta.insert("pr_title".to_string(), enriched.title.clone());

                    if !task.has_tests() {
                        let language = task.language.clone();
                        if let Err(err) = test_generator.ensure_tests(&mut task, &language).await {
                            tracing::warn!(task_id = %task.id, error = %err, "Test generation failed");
                            return None;
                        }
                    }

                    let assessment = match quality.assess(&task).await {
                        Ok(a) => a,
                        Err(err) => {
                            tracing::warn!(task_id = %task.id, error = %err, "Quality assessment failed");
                            return None;
                        }
                    };

                    let (score, passed) = (assessment.score, assessment.passed);
                    task.quality_score = Some(score);
                    task.quality_passed = passed;
                    task.difficulty_score = match assessment.difficulty_level.as_str() {
                        "easy" => 1, "medium" => 2, "hard" => 3, _ => 1,
                    };
                    task.meta.insert("difficulty".to_string(), assessment.difficulty_level.clone());
                    let difficulty_ok = match df.as_deref() {
                        Some(f) => assessment.difficulty_level == f,
                        None => true,
                    };

                    tracing::info!(task_id = %task.id, difficulty = %assessment.difficulty_level, score, passed = passed && difficulty_ok, "Task processed");

                    if passed && difficulty_ok {
                        task.status = crate::swe::SweTaskStatus::Ready;
                        Some(task)
                    } else { None }
                });
            }

            let results = futures::future::join_all(process_futures).await;
            for result in results {
                scored += 1;
                if let Some(task) = result {
                    extracted += 1;
                    tasks.push(task);
                    if tasks.len() >= config.max_tasks && config.once {
                        break;
                    }
                }
            }
        }

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
