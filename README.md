# dataforge

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**High-performance SWE-bench dataset generator that mines real GitHub pull requests and produces evaluation-ready task instances.**

Built on top of [SweInfinite](https://github.com/unconst/SweInfinite) by [@unconst](https://github.com/unconst), fine-tuned and extended for automated large-scale dataset generation with difficulty-based filtering, structured LLM outputs, and full parallelism.

## What it does

dataforge connects to [GH Archive](https://www.gharchive.org/) to discover recently merged pull requests, enriches them via the GitHub API, classifies their difficulty using an LLM, generates test specifications, and exports SWE-bench-compatible task instances — all in a single pipeline.

```
GH Archive (millions of events)
  → Pre-filter (merged PRs, org repos, no bots)
  → GitHub API enrichment (title, body, diff, stars, language)
  → Local filter (language, stars, files changed)
  → LLM pre-classification (easy/medium/hard triage in ~0.5s per PR)
  → Patch extraction (git clone + diff)
  → LLM test generation (fail_to_pass + pass_to_pass test specs)
  → LLM quality scoring (difficulty classification + quality gate)
  → Export (workspace.yaml + prompt.md + checks.txt)
```

## Key features

- **Real GitHub data** — mines GH Archive for recently merged PRs across all public repositories. No synthetic data, no stubs, no fallbacks.
- **Difficulty filtering** — pre-classifies PRs as easy/medium/hard before expensive processing. Only spends LLM tokens on candidates matching your target difficulty.
- **Aggressive parallelism** — GH Archive hours fetched 8x concurrent, enrichment 3x concurrent with rate limiting, LLM pre-classification 10x concurrent, deep processing 3x concurrent.
- **Structured LLM outputs** — uses OpenRouter's `json_schema` and `json_object` response formats for reliable JSON parsing. No regex hacking.
- **Streaming chunks** — processes candidates in batches of 30 to avoid burning GitHub API rate limits while maintaining throughput.
- **JSONL tracking** — auto-appends processed PRs to a JSONL file so re-runs skip already-seen PRs.

## Quick start

### Prerequisites

- Rust 1.70+
- [OpenRouter](https://openrouter.ai/) API key
- GitHub Personal Access Token (PAT) with public repo read access

### Build

```bash
git clone https://github.com/your-org/dataforge.git
cd dataforge
cargo build --release
```

### Generate datasets

```bash
# Set credentials
export OPENROUTER_API_KEY="sk-or-v1-..."
export GITHUB_TOKEN="ghp_..."

# Mine 10 hard tasks
cargo run -- swe mine \
  --output ./hard-tasks \
  --pr-file ./processed.jsonl \
  --max-tasks 10 \
  --difficulty hard \
  --min-stars 10 \
  --once

# Mine 5 easy tasks (faster, more candidates match)
cargo run -- swe mine \
  --output ./easy-tasks \
  --max-tasks 5 \
  --difficulty easy \
  --once

# Mine without difficulty filter (accept all)
cargo run -- swe mine \
  --output ./all-tasks \
  --max-tasks 20 \
  --once
```

### Output structure

Each task is exported as a directory:

```
hard-tasks/
  owner-repo-1234/
    workspace.yaml    # Full task metadata (SWE-bench compatible)
    prompt.md         # Task description for the agent
    checks.txt        # fail_to_pass + pass_to_pass test commands
```

## CLI reference

```
dataforge swe mine [OPTIONS]

Options:
  -o, --output <DIR>          Output directory [default: ./swe-datasets]
  -m, --model <MODEL>         OpenRouter model [default: openai/gpt-5.2-codex:nitro]
  -n, --max-tasks <N>         Number of tasks to generate [default: 1]
  -d, --difficulty <LEVEL>    Filter: easy, medium, hard [optional]
      --min-stars <N>         Minimum repo stars [default: 20]
      --languages <LIST>      Comma-separated language filter [optional]
      --pr-file <PATH>        JSONL file to track processed PRs [optional]
      --once                  Run once then exit (vs continuous)
      --api-key <KEY>         OpenRouter API key (or OPENROUTER_API_KEY env)
```

## Difficulty classification

The LLM classifies each PR into three tiers based on the scope and complexity of changes:

| Level | Score | Typical changes | Examples |
|-------|-------|-----------------|----------|
| **Easy** | 0.1–0.35 | Typo fixes, config changes, single-file edits | Fix import, update version, rename variable |
| **Medium** | 0.4–0.65 | Bug fixes, feature additions, API changes | Fix race condition, add endpoint, refactor module |
| **Hard** | 0.7–1.0 | Cross-cutting changes, architectural refactors | New subsystem, protocol change, major migration |

Pre-classification uses only the PR title and body (~100 tokens, ~0.5s). Full classification uses the complete diff and test spec.

## Architecture

### Pipeline stages

| Stage | Parallelism | Rate limit | Description |
|-------|-------------|------------|-------------|
| GH Archive fetch | 8 concurrent | None | Download hourly event dumps |
| Pre-filter | N/A | None | Exclude bots, non-org repos, invalid PRs |
| Enrichment | 3 concurrent | GitHub 5000/h | Fetch PR metadata via GitHub API |
| Local filter | N/A | None | Language, stars, files changed |
| Pre-classification | 10 concurrent | OpenRouter | Fast LLM triage on title+body |
| Patch extraction | 3 concurrent | None | Git clone + diff extraction |
| Test generation | 3 concurrent | OpenRouter | LLM generates test commands |
| Quality scoring | 3 concurrent | OpenRouter | LLM classifies difficulty + quality gate |

### Rate limit management

GitHub API allows 5000 requests/hour per token. The pipeline processes candidates in chunks of 30 (each needing ~2 API calls for enrichment). Only candidates that pass the GH Archive pre-filter (org repos, no bots, valid PRs) are enriched — typically ~500/hour out of ~3000 merged PRs.

To increase throughput, use multiple GitHub tokens. The pipeline will respect rate limits automatically.

## Configuration

### Environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `OPENROUTER_API_KEY` | Yes | OpenRouter API key for LLM calls |
| `GITHUB_TOKEN` | Yes | GitHub PAT for PR enrichment |
| `RUST_LOG` | No | Log level: `error`, `warn`, `info`, `debug`, `trace` |

### Model selection

The default model is `openai/gpt-5.2-codex:nitro` via OpenRouter. Any OpenRouter-compatible model that supports structured outputs (`json_schema` or `json_object`) can be used:

```bash
cargo run -- swe mine --model anthropic/claude-sonnet-4 --max-tasks 5
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- swe mine --max-tasks 1 --once

# Check for warnings
cargo clippy
```

## Credits

This project is built on top of [SweInfinite](https://github.com/unconst/SweInfinite) by [@unconst](https://github.com/unconst). The original architecture for mining GitHub PRs and generating SWE-bench-style datasets was designed by the SweInfinite team. dataforge extends it with:

- Difficulty-based pre-classification and filtering
- Structured LLM outputs (JSON Schema) for reliable parsing
- Full pipeline parallelism (GH Archive, enrichment, LLM calls)
- Streaming chunk processing with rate limit management
- JSONL-based PR tracking (replaces SQLite)
- Relaxed test spec parsing for cross-model compatibility

Thank you [@unconst](https://github.com/unconst) for the foundational work.

## License

MIT — see [LICENSE](LICENSE).
