#!/usr/bin/env bash
set -euo pipefail

read -rp "GitHub Token (ghp_...): " GITHUB_TOKEN
export GITHUB_TOKEN

read -rp "OpenRouter API Key (sk-or-v1-...): " OPENROUTER_API_KEY
export OPENROUTER_API_KEY

cd "$(dirname "$0")"

# Mine 50 tasks per difficulty level (easy, medium, hard) in a single pipeline run.
# Tasks are exported to ./generated-swe/easy-tasks, ./generated-swe/medium-tasks, ./generated-swe/hard-tasks.
cargo run --release -- swe mine \
  --output ./generated-swe \
  --pr-file ./processed.jsonl \
  --difficulty-targets "easy:50,medium:50,hard:50" \
  --once
