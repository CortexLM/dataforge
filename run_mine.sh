#!/usr/bin/env bash
set -euo pipefail

read -rp "GitHub Token (ghp_...): " GITHUB_TOKEN
export GITHUB_TOKEN

read -rp "OpenRouter API Key (sk-or-v1-...): " OPENROUTER_API_KEY
export OPENROUTER_API_KEY

cd "$(dirname "$0")"

cargo run --release -- swe mine \
  --output ./hard-tasks \
  --pr-file ./processed.jsonl \
  --max-tasks 100 \
  --difficulty hard \
  --once
