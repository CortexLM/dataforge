# apogee-stealth/the-agency-1

apogee-stealth/the-agency (#1): DEV-3134 Add prep-pr, weekly-summary commands; pluggable review checks

## Summary

Adds two new slash commands and a pluggable review check system:

- **`/prep-pr`** — Walks through pre-submission checks, generates a PR title/description, collects testing steps, and creates a draft PR via `gh`. Supports review plugin checks when installed.
- **`/weekly-summary`** — Synthesizes the last 7 days of merged PRs into a thematic narrative briefing (not a changelog), written to `docs/reports/`.
- **Pluggable review checks** — `/review-pr` and `/prep-pr` now discover check files dynamically from `.ai/review-checks/` instead of using hardcoded tribal knowledge checks. Ships with 4 pre-packaged plugins (general, node-backend, react-frontend, unit-test).
- **`install-review-plugins` CLI command** — Interactive multi-select to install review plugins into a consumer project.
- **`/build` now auto-proceeds** between phases instead of pausing for user confirmation at each gate.
- **Agent refinements** — Dev and test-hardener agents now reference `.ai/UnitTestGeneration.md`, exclude `.tsx` and barrel export testing, and test-hardener adds to existing test files instead of creating separate ones.

## Test Plan

Install these new commands (and the review plugins) in your target repo and take them for a test drive.
