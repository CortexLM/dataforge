# AGENTS.md — src/cli/

## Purpose

Clap-based CLI interface. Defines all commands, argument parsing, and command dispatch.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports `parse_cli`, `run`, `run_with_cli` |
| `commands.rs` | `Cli` struct (Clap derive), `Commands` enum, all subcommand args, command handlers |

## Commands

| Command | Description |
|---------|-------------|
| `swe-forge generate` (alias: `gen`) | Generate SWE DataForge tasks from real GitHub PRs |
| `swe-forge evaluate` (alias: `eval`) | Evaluate generated tasks using an autonomous agent |
| `swe-forge swe mine` | Mine real PRs and export SWE-style tasks |
| `swe-forge swe harness` | Run evaluation harness on generated tasks |
| `swe-forge swe validate` | Validate generated SWE workspaces |
| `swe-forge swe export` | Export SWE workspaces to dataset format |
| `swe-forge swe load` | Load a dataset from HuggingFace or local parquet for inspection |
| `swe-forge swe benchmark` | Run a benchmark on N PRs and output pipeline metrics as JSON |

## Rules

- Use `anyhow::Result` for command handler return types
- Default model constant: `DEFAULT_MODEL = "openai/gpt-5.2-codex:nitro"`
- Default output dirs: `./generated-datasets` (generate), `./generated-swe` (swe mine)
- Global `--log-level` arg controls tracing filter
- API keys come from env vars or CLI args (env var takes precedence)
