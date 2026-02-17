# AGENTS.md — src/agents/

## Purpose

Task validation and Docker-based verification agents. These agents validate generated benchmark tasks for correctness, execute them in Docker containers, and score difficulty.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports |
| `docker_validator.rs` | `DockerValidatorAgent` — builds and runs tasks in Docker, validates output |
| `task_validator.rs` | `TaskValidatorAgent` — validates task ideas and assesses feasibility |
| `task_executor.rs` | `TaskExecutorAgent` — generates synthetic tasks with anti-memorization, difficulty scoring, verification specs |
| `error.rs` | `AgentError` enum, `AgentResult<T>` type alias |

## Key Types

- `DockerValidatorAgent` / `DockerValidatorConfig` / `DockerValidationResult`
- `TaskValidatorAgent` / `TaskValidatorConfig` / `ValidationAssessment` / `TaskIdea`
- `TaskExecutorAgent` / `TaskExecutorConfig` / `SyntheticTask` / `TaskMetadata`
- `AntiMemorizationConfig` — Config for anti-hardcoding in generated tasks
- `DifficultyScoring` — Difficulty assessment with scoring criteria
- `HiddenSolution` — Solution hidden from the agent during evaluation
- `VerificationSpec` — Specification for verifying task outputs
- `AutomatedCheck` / `CheckType` — Automated verification checks
- `PartialCreditItem` — Partial credit scoring for incomplete solutions

## Rules

- Docker validation must always use resource limits from `src/docker/resources.rs`
- Task executor must embed `AntiMemorizationConfig` canary strings
- All agent errors must use `AgentError` from `error.rs`
