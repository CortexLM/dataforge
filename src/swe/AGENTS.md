# AGENTS.md — src/swe/

## Purpose

Core SWE mining pipeline. Fetches merged pull requests from GH Archive, enriches them via GitHub API, classifies difficulty with LLMs, extracts patches via git clone, generates test specifications through an agentic multi-turn loop, scores quality, and exports SWE-bench-compatible task instances.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | `SweTask` struct, `SweTaskStatus` enum, re-exports |
| `gharchive.rs` | HTTP client for GH Archive hourly event dumps (gzip → JSON) |
| `enricher.rs` | GitHub API enrichment (PR metadata, diff, files) |
| `filters.rs` | Pre-filter (merged PRs, no bots, org repos, language, stars) |
| `extractor.rs` | Git clone + `git diff` patch extraction |
| `test_generator.rs` | Agentic multi-turn LLM test generation (up to 200 turns, `shell` + `submit_tests` tools) |
| `quality.rs` | LLM-based quality scoring and difficulty classification |
| `prompt_rewriter.rs` | Strips test plan leaks from PR body → `prompt.md` |
| `harness.rs` | Docker-isolated evaluation harness (sanity check → agent run → verify) |
| `docker_sandbox.rs` | Docker sandbox for test generation phase |
| `orchestrator.rs` | End-to-end pipeline orchestrator with `DifficultyTargets` |
| `pipeline.rs` | Streaming pipeline with semaphore-based concurrency |
| `github_search.rs` | `GitHubSearchClient` — GitHub Search API as alternative PR source (30 req/min) |
| `workspace_validator.rs` | `WorkspaceValidator` — pre-export Docker-based validation (install, tests, patch application) |
| `tool_server.rs` | Embedded Python HTTP tool server injected into Docker containers (read_file, list_dir, grep_files, search_files, apply_patch) |
| `pr_cache.rs` | SQLite-backed PR deduplication cache |
| `progress.rs` | `ProgressMonitor` — background progress logging for long-running pipeline runs |

## Key Types

- `SweTask` — Central task struct with patch, tests, metadata, quality score
- `SweTaskStatus` — `Candidate → Rejected | Ready → Exported → Validated`
- `GhArchiveClient` / `GhArchiveEvent` — GH Archive ingestion
- `EnrichedPullRequest` — GitHub API enriched PR data
- `ExtractedPatch` / `PatchExtractor` — Git diff extraction
- `TestGenerator` / `TestFile` — Agentic test generation
- `QualityScorer` / `QualityAssessment` — LLM quality gate
- `HarnessConfig` / `HarnessResult` / `HarnessSummary` — Evaluation harness
- `SwePipeline` / `SwePipelineEvent` / `SwePipelineRunResult` / `BenchmarkMetrics` — Streaming pipeline
- `SweOrchestrator` / `SweOrchestratorConfig` / `SweRunResult` — Orchestrator
- `ProgressMonitor` / `ProgressCounters` / `ProgressSnapshot` — Pipeline progress tracking
- `GitHubSearchClient` / `SearchConfig` — GitHub Search API client
- `WorkspaceValidator` / `ValidationOutcome` — Pre-export workspace validation

## Concurrency Limits

| Stage | Concurrency | Rate Limit |
|-------|-------------|------------|
| GH Archive fetch | 8 | None |
| GitHub enrichment | 10 | 5000 req/h |
| LLM pre-classification | 25 | OpenRouter |
| Deep processing (extract + test gen) | 8 | OpenRouter |

## Rules

- Never leak `fail_to_pass` / `pass_to_pass` into `prompt.md` — use `prompt_rewriter.rs`
- Always check `pr_cache` before processing a PR to avoid duplicates
- Pipeline uses semaphore-based concurrency — no chunk barriers
- All LLM calls must use function calling (`tools` + `tool_choice: "required"`)
- Harness statuses: `resolved`, `unresolved`, `agent_error`, `test_error`, `setup_error`, `sanity_fail`