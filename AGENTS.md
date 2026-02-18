# AGENTS.md — swe-forge

## Project Purpose

**swe-forge** is a high-performance SWE-bench dataset generator and evaluation harness written in Rust. It mines real GitHub pull requests from GH Archive, enriches them via the GitHub API, uses LLMs (via OpenRouter) to classify difficulty and generate test specifications through an agentic multi-turn loop, and exports SWE-bench-compatible task instances. It also includes a Docker-isolated evaluation harness that runs external coding agents on generated tasks and verifies their solutions.

## Architecture Overview

swe-forge is a single Rust binary crate (`src/main.rs`) with a library (`src/lib.rs`) organized into these modules:

```
src/
├── main.rs                  # CLI entry point (tokio async runtime)
├── lib.rs                   # Public module declarations
├── cli/                     # Clap-based CLI (commands: generate, evaluate, swe mine/harness/validate/export/load/benchmark)
├── swe/                     # Core mining pipeline (GH Archive → enrich → filter → classify → extract → test gen → export)
│   ├── gharchive.rs         # GH Archive HTTP ingestion (gzip → JSON events)
│   ├── enricher.rs          # GitHub API PR enrichment (title, body, diff, files)
│   ├── filters.rs           # Pre-filter (bots, org repos, language, stars)
│   ├── extractor.rs         # Git clone + diff patch extraction
│   ├── test_generator.rs    # Agentic multi-turn LLM test generation (up to 200 turns)
│   ├── quality.rs           # LLM-based quality scoring
│   ├── prompt_rewriter.rs   # PR body → agent prompt (strip test plan leaks)
│   ├── harness.rs           # Docker-isolated evaluation harness
│   ├── docker_sandbox.rs    # Docker sandbox for test generation
│   ├── orchestrator.rs      # End-to-end pipeline orchestrator
│   ├── pipeline.rs          # Streaming pipeline with semaphore-based concurrency
│   ├── pr_cache.rs          # SQLite-backed PR deduplication cache
│   ├── progress.rs          # Background progress monitor for long-running pipeline runs
│   ├── github_search.rs     # GitHub Search API client (alternative PR source)
│   ├── tool_server.rs       # Embedded Python HTTP tool server injected into Docker containers
│   └── workspace_validator.rs # Pre-export workspace validation (Docker-based)
├── llm/                     # LLM integration layer
│   ├── litellm.rs           # OpenAI-compatible API client (function calling, tools)
│   ├── providers/            # Provider implementations (OpenRouter)
│   ├── router.rs            # Multi-model routing (cost-optimized, round-robin)
│   ├── cache.rs             # Prompt caching for multi-conversation efficiency
│   └── cost.rs              # Usage tracking with daily/monthly budgets
├── agents/                  # Task validation agents (Docker-based verification)
├── execution/               # Docker execution layer (bollard crate, container lifecycle)
├── docker/                  # Dockerfile/docker-compose generation
├── export/                  # Parquet dataset export + HuggingFace Hub upload
├── difficulty/              # Difficulty levels, resource limits, scoring
├── anti_hardcoding/         # Canary strings, sealed parameters, contamination detection
├── runner/                  # Agent runner infrastructure (sandbox, verifier, agent adapters) [not compiled — not declared in lib.rs; exists as scaffolding]
├── utils/                   # JSON extraction from LLM responses
└── error.rs                 # Typed error hierarchy (thiserror)
```

### Data Flow

```
GH Archive (hourly dumps, 8x concurrent)
  → Pre-filter (merged PRs, no bots, org repos)
  → GitHub API enrichment (10x concurrent, rate-limited 5000/h)
  → Local filter (language, stars, files changed)
  → LLM pre-classification (25x concurrent, title+body only)
  → Patch extraction + agentic test generation (8x concurrent)
  → Quality scoring (LLM-based)
  → Export (workspace.yaml + prompt.md + checks.txt)
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (edition 2021, stable toolchain) |
| Async runtime | Tokio (full features) |
| CLI framework | Clap 4 (derive mode) |
| HTTP client | reqwest 0.13 (json, rustls, multipart) |
| Docker | bollard 0.16 (SSL) |
| Serialization | serde + serde_json + serde_yaml |
| Database | SQLx 0.7 (Postgres + SQLite, migrations) |
| Data export | Apache Arrow 54 + Parquet 54 |
| Caching | Redis 0.24 (tokio-comp, connection-manager) |
| Templating | Tera 1.20 |
| Error handling | thiserror 2.0 + anyhow 1.0 |
| Logging | tracing + tracing-subscriber (env-filter) |
| Linker | cc (via `.cargo/config.toml`) |
| LLM provider | OpenRouter (OpenAI-compatible function calling) |

## Build & Test Commands

```bash
# Build (debug)
cargo build

# Build (release, optimized)
cargo build --release

# Run all tests
cargo test

# Run tests (release mode, parallel)
cargo test --release -- --test-threads=$(nproc)

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --all -- --check

# Format fix
cargo fmt --all

# Run doc tests
cargo test --doc

# Run the CLI
cargo run -- swe mine --help
cargo run -- swe harness --help
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `OPENROUTER_API_KEY` | Yes (runtime) | OpenRouter API key for LLM calls (fallback: `LITELLM_API_KEY`) |
| `GITHUB_TOKEN` | Yes (runtime) | GitHub PAT for PR enrichment (fallback: `GITHUB_PERSONAL_ACCESS_TOKEN`) |
| `HF_TOKEN` | No | HuggingFace API token for dataset upload (used with `--hf-repo`) |
| `RUST_LOG` | No | Log level: `error`, `warn`, `info`, `debug`, `trace` |

## Git Hooks

Git hooks are in `.githooks/` and activated via `git config core.hooksPath .githooks`.

- **pre-commit**: Runs `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`
- **pre-push**: Runs format check + clippy + `cargo test --lib` + `cargo build`
- Both hooks can be skipped with `SKIP_GIT_HOOKS=1`

## CRITICAL RULES

1. **All errors must use typed error enums from `src/error.rs`** — Never use `unwrap()` or `expect()` in library code. Use `anyhow::Result` only in `main.rs` and CLI commands. Library modules must return typed errors via `thiserror` (`RegistryError`, `GeneratorError`, `LlmError`, `DockerError`, `ExportError`, `ValidationError`, `TemplateError`).

2. **All LLM interactions must use function calling (`tools` + `tool_choice`)** — Never parse free-form LLM text. Use OpenAI-compatible `tools` array with `tool_choice: "required"` for structured JSON output. See `src/llm/litellm.rs` for `ToolDefinition`, `ToolChoice`, and `ToolCallInfo` types.

3. **Never leak test plans into agent prompts** — The `prompt_rewriter.rs` module strips test-specific information from PR bodies before generating `prompt.md`. Any new prompt generation code must ensure `fail_to_pass` and `pass_to_pass` test commands are never visible to the agent being evaluated.

4. **Docker containers must have resource limits** — All container creation must use `apply_resource_limits()` from `src/docker/resources.rs`. Difficulty-based limits are enforced: PIDs (100–500), storage (1–5 GB), network mode (none/internal). Never create containers without limits.

5. **Respect GitHub API rate limits (5000 req/h)** — The pipeline uses semaphore-based concurrency (no chunk barriers). Each candidate needs ~2 API calls for enrichment. Never add unbounded concurrent GitHub API calls. Use the existing concurrency limits (enrichment: 10x, pre-classification: 25x, deep processing: 8x).

6. **All async code must be `Send + Sync` compatible** — The codebase uses `Arc<dyn LlmProvider>` extensively. Trait objects must be `Send + Sync`. Never introduce `Rc`, `RefCell`, or non-Send types in async contexts.

7. **Serde rename conventions** — Most serializable enums use `#[serde(rename_all = "snake_case")]` (e.g., `SweTaskStatus`, `HarnessStatus`, `CheckType`). `DifficultyLevel` and `NetworkMode` in `src/difficulty/` use `#[serde(rename_all = "lowercase")]`. Follow the existing convention for each type for YAML/JSON compatibility.

8. **Anti-hardcoding mechanisms must be preserved** — The `anti_hardcoding/` module provides canary strings, sealed parameters, and process validation. Never bypass contamination detection. Any new task generation must embed canary strings via `CanaryConfig::generate()`.

9. **Use `tracing` for all logging, never `println!`** — All log output must use `tracing::{info, warn, debug, error, trace}` macros. The log level is controlled by `RUST_LOG` env var or `--log-level` CLI arg.

10. **Parquet/Arrow exports must preserve schema** — The `export/parquet_writer.rs` module defines the schema for dataset export. Never change field types or remove fields from the Parquet schema without updating `read_parquet` and `write_parquet` together.

## DO's

- Use `anyhow::Result` for CLI command handlers in `src/cli/commands.rs`
- Use typed `thiserror` errors for all library module boundaries
- Add `#[cfg(test)] mod tests` blocks in the same file for unit tests
- Use `tokio::spawn` for concurrent work, `futures::stream` for bounded concurrency
- Follow the existing pattern of `mod.rs` re-exporting public types
- Use `Arc<dyn LlmProvider>` for LLM provider abstraction
- Add doc comments (`///`) to all public types and functions
- Use `BTreeMap` (not `HashMap`) for deterministic serialization in `SweTask`

## DON'Ts

- Don't use `unwrap()` or `expect()` in library code — use `?` operator
- Don't add new direct dependencies without checking if an existing dep covers the use case
- Don't use `println!` or `eprintln!` — use `tracing` macros
- Don't create Docker containers without resource limits
- Don't make unbounded concurrent API calls — always use semaphore or stream limits
- Don't store secrets (API keys, tokens) in code or config files
- Don't change the `workspace.yaml` schema without updating the harness parser
- Don't bypass the PR deduplication cache (`pr_cache.rs`) — it prevents reprocessing
