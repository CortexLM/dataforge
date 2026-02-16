# AGENTS.md — src/anti_hardcoding/

## Purpose

Anti-hardcoding mechanisms to ensure benchmark integrity. Detects if models have memorized benchmarks (contamination), prevents pre-computation of answers, and validates that agents follow proper problem-solving processes.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | `AntiHardcodingVerifier` — unified verifier combining all mechanisms; `VerificationResult` |
| `canary.rs` | `CanaryConfig` — unique identifiers embedded in tasks for contamination detection |
| `sealed.rs` | `SealedParameters` / `SealedData` — encrypted parameters revealed only at verification time |
| `process_validation.rs` | `ProcessTracer` / `ProcessValidationConfig` — validates command execution patterns |

## Key Types

- `AntiHardcodingVerifier` — Combines canary + process validation
- `CanaryConfig` — Generated via `CanaryConfig::generate(task_id, seed)`
- `ContaminationResult` — `contaminated`, `partial_match`, `confidence`
- `SealedParameters` / `SealedData` / `SealError`
- `ProcessTracer` / `ProcessValidationConfig` / `CommandExecution`
- `VerificationResult` — `valid`, `score`, `contamination`, `process_validation`, `issues`

## Scoring

- Confirmed contamination: 90% score penalty (`score *= 0.1`)
- Partial match: 30% penalty (`score *= 0.7`)
- High confidence (>0.5): up to 20% additional penalty

## Rules

- Every generated task must embed a canary via `CanaryConfig::generate()`
- Never bypass contamination detection in the verification pipeline
- Process validation patterns use regex — test patterns before deploying
- `required_pattern` must match at least one recorded `CommandExecution`
