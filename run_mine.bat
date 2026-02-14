@echo off
set /p GITHUB_TOKEN="GitHub Token (ghp_...): "
set /p OPENROUTER_API_KEY="OpenRouter API Key (sk-or-v1-...): "

cd /d "%~dp0"

cargo run --release -- swe mine ^
  --output ./hard-tasks ^
  --pr-file ./processed.jsonl ^
  --max-tasks 100 ^
  --difficulty hard ^
  --once

pause
