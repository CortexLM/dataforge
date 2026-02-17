# AGENTS.md — src/swe/

## Purpose

Core SWE mining pipeline. Fetches merged pull requests from GH Archive, enriches them via GitHub API, classifies difficulty with LLMs, extracts patches via git clone, generates test specifications through an agentic multi-turn loop, scores quality, and exports SWE-bench-compatible task instances.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | `SweTask` struct, `SweTaskStatus` enum, re-exports |
| `gharchive.rs` | HTTP client for GH Archive hourly event dumps (gzip → JSON) |
| `enricher.rs` | GitHub API enrichment (PR metadata, diff, files, 3x concurrent) |
| `filters.rs` | Pre-filter (merged PRs, no bots, org repos, language, stars) |
| `extractor.rs` | Git clone + `git diff` patch extraction |
| `test_generator.rs` | Agentic multi-turn LLM test generation (up to 200 turns, `shell` + `submit_tests` tools) |
| `quality.rs` | LLM-based quality scoring and difficulty classification |
| `prompt_rewriter.rs` | Strips test plan leaks from PR body → `prompt.md` |
| `harness.rs` | Docker-isolated evaluation harness (sanity check → agent run → verify) |
| `docker_sandbox.rs` | Docker sandbox for test generation phase |
| `orchestrator.rs` | End-to-end pipeline orchestrator with `DifficultyTargets` |
| `pipeline.rs` | Streaming pipeline with chunk processing (batches of 30) |
| `pr_cache.rs` | JSONL-based PR deduplication cache |

## Key Types

- `SweTask` — Central task struct with patch, tests, metadata, quality score
- `SweTaskStatus` — `Candidate → Rejected | Ready → Exported → Validated`
- `GhArchiveClient` / `GhArchiveEvent` — GH Archive ingestion
- `EnrichedPullRequest` — GitHub API enriched PR data
- `ExtractedPatch` / `PatchExtractor` — Git diff extraction
- `TestGenerator` / `TestFile` — Agentic test generation
- `QualityScorer` / `QualityAssessment` — LLM quality gate
- `HarnessConfig` / `HarnessResult` / `HarnessSummary` — Evaluation harness
- `SwePipeline` / `SwePipelineEvent` — Streaming pipeline
- `SweOrchestrator` / `SweOrchestratorConfig` — Orchestrator

## Concurrency Limits

| Stage | Concurrency | Rate Limit |
|-------|-------------|------------|
| GH Archive fetch | 8 | None |
| GitHub enrichment | 3 | 5000 req/h |
| LLM pre-classification | 10 | OpenRouter |
| Patch extraction | 3 | None |
| Test generation | 3 | OpenRouter |

## Rules

- Never leak `fail_to_pass` / `pass_to_pass` into `prompt.md` — use `prompt_rewriter.rs`
- Always check `pr_cache` before processing a PR to avoid duplicates
- Process candidates in chunks of 30 to respect GitHub rate limits
- All LLM calls must use function calling (`tools` + `tool_choice: "required"`)
- Harness statuses: `resolved`, `unresolved`, `agent_error`, `test_error`, `setup_error`, `sanity_fail`
