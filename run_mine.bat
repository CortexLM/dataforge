@echo off
set /p GITHUB_TOKEN="GitHub Token (ghp_...): "
set /p OPENROUTER_API_KEY="OpenRouter API Key (sk-or-v1-...): "

cd /d "%~dp0"

REM Mine 50 tasks per difficulty level (easy, medium, hard) in a single pipeline run.
REM Tasks are exported to ./generated-swe/easy-tasks, ./generated-swe/medium-tasks, ./generated-swe/hard-tasks.
cargo run --release -- swe mine ^
  --output ./generated-swe ^
  --pr-file ./processed.jsonl ^
  --difficulty-targets "easy:50,medium:50,hard:50" ^
  --once

pause
