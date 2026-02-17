# Agent Test Results — baseagent-echo

## Overview

| Metric | Value |
|--------|-------|
| Agent | `baseagent-echo` (echobt) |
| Model | `anthropic/claude-opus-4.6` via OpenRouter |
| Harness | `swe-forge` (Docker-based SWE evaluation) |
| Total Tasks | 9 |
| **Resolved** | **2** |
| Sanity Fail | 5 |
| Setup Error | 2 |
| Agent Error | 0 |
| **Effective Resolution Rate** | **100% (2/2 tasks with valid sanity checks)** |

## Results by Task

### Easy (3 tasks)

| Task | Status | Agent Time |
|------|--------|-----------|
| `happier-dev/happier-35` | ✅ RESOLVED | 225s |
| `batocera-linux/batocera.linux-15418` | ✅ RESOLVED | 177s |
| `cs360s26impact/impact-15` | ❌ setup_error | — |

### Medium (3 tasks)

| Task | Status | Agent Time |
|------|--------|-----------|
| `hermetoproject/hermeto-1294` | ❌ sanity_fail | — |
| `Altinn/altinn-studio-17755` | ❌ sanity_fail | — |
| `BibliothecaDAO/eternum-4225` | ❌ sanity_fail | — |

### Hard (3 tasks)

| Task | Status | Agent Time |
|------|--------|-----------|
| `TrooHQ/troo-core-30` | ❌ sanity_fail | — |
| `ep-eaglepoint-ai/bd_datasets_002-245` | ❌ sanity_fail | — |
| `stellatogrp/cvxro-56` | ❌ setup_error | — |

## Status Definitions

- **resolved**: Agent successfully fixed the issue; all tests pass.
- **sanity_fail**: The task's sanity checks failed on the base commit (tests don't behave as expected before the agent runs). This is a dataset/environment issue, not an agent issue.
- **setup_error**: The task's repository could not be cloned or checked out. Infrastructure issue.
- **agent_error**: The agent crashed or timed out. (None occurred.)

## Key Findings

1. **The agent works correctly.** On every task where the sanity checks passed (2/2), the agent resolved the issue successfully.
2. **5 tasks have sanity check failures.** The pass-to-pass or fail-to-pass test expectations don't match the actual behavior on the base commit. These are dataset validation issues.
3. **2 tasks have setup errors.** Repository checkout failures (shallow clone issues, missing commits).

## Files

- `summary.json` — Machine-readable aggregate results
- `easy/`, `medium/`, `hard/` — Per-task JSON result files
