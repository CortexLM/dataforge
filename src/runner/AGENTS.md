# AGENTS.md — src/runner/

## Status

**Not compiled** — This module is not declared in `src/lib.rs` and is not part of the build. It exists as scaffolding for future agent runner infrastructure.

## Purpose

Agent runner infrastructure for benchmark evaluation. Spawns external AI agents against benchmark tasks in isolated sandboxes, captures outputs, and verifies results.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports, architecture docs |
| `config.rs` | `RunConfig` — task path, agent type, timeout, environment |
| `executor.rs` | `AgentRunner` — spawns agent process, captures output, records metadata |
| `result.rs` | `RunResult`, `RunStatus`, `ExecutionTrace`, `TokenUsage` |
| `sandbox.rs` | `Sandbox` / `SandboxConfig` — isolated execution environment |
| `verifier.rs` | `Verifier` — loads `task.yaml`, runs checks, produces `VerificationResult` with scores |
| `agents/baseagent.rs` | Base agent adapter implementation |
| `agents/generic.rs` | Generic agent adapter for external commands |
| `agents/mod.rs` | `AgentAdapter` trait, `AgentType` enum |

## Key Types

- `AgentRunner` / `RunConfig` — Run an agent against a task
- `RunResult` / `RunStatus` — Execution result with status and traces
- `Sandbox` / `SandboxConfig` / `SandboxError` — Isolated environment
- `Verifier` / `VerificationResult` / `CheckResult` — Output verification
- `AgentAdapter` (trait) / `AgentType` — Agent abstraction

## Data Flow

```
Task (prompt.md) → AgentRunner → Agent Process → Output Directory → Verifier
```

## Rules

- Agent timeout is configurable (default 600s) — always enforce it
- Sandbox must isolate agent from host filesystem
- Verifier loads checks from `task.yaml` — schema must match
