# AGENTS.md — src/difficulty/

## Purpose

Difficulty classification system. Defines difficulty levels (Easy/Medium/Hard), resource limits per level, scoring calculations, and time/step expectations.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | `DifficultyLevel` enum, score ranges, time ranges, command step ranges, resource limits |

## Key Types

- `DifficultyLevel` — `Easy`, `Medium`, `Hard` (serde: `lowercase`)
- `ResourceLimits` — CPU, memory, storage, network, PIDs per difficulty
- `NetworkMode` — `None`, `Internal`, `External` (serde: `lowercase`)
- `CalibrationResult` — Calibration data from human testers
- `calculate_difficulty_score(mean_time, success_rate, mean_hints)` — Weighted difficulty score (0–1)
- `calculate_task_score(difficulty, success, partial, time, expected, valid)` — Final attempt score

## Difficulty Ranges

| Level | Score | Time | Steps | Success Rate |
|-------|-------|------|-------|-------------|
| Easy | 0.0–0.33 | 3–6 min | 5–10 | 90% |
| Medium | 0.34–0.66 | 8–15 min | 10–25 | 70% |
| Hard | 0.67–1.0 | 15–60 min | 25–50 | 40% |

## Rules

- Always use `#[serde(rename_all = "lowercase")]` for `DifficultyLevel`
- Score ranges, time ranges, and step ranges are authoritative — don't change without updating all consumers
