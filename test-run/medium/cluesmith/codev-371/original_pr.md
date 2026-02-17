# cluesmith/codev-371 (original PR)

cluesmith/codev (#371): [Bugfix #370] Fix Gemini --yolo mode in general consultations

## Summary
Fixes #370

## Root Cause
`MODEL_CONFIGS` hardcoded `args: ['--yolo']` for Gemini, passing it unconditionally to both general mode (`--prompt`) and protocol mode (`--type`). In general mode, this gave Gemini full write access via auto-approval, allowing it to silently modify files in the main worktree.

## Fix
- Removed `--yolo` from `MODEL_CONFIGS.gemini.args` (now `[]`)
- Added `generalMode` parameter to `runConsultation()`
- `--yolo` is now only passed in protocol mode (when `--type` is set), where structured reviews need file access
- General mode consultations no longer get write access

## Test Plan
- [x] Added regression test: general mode does NOT pass `--yolo` to Gemini CLI
- [x] Added regression test: protocol mode DOES pass `--yolo` to Gemini CLI
- [x] Updated existing model config test to reflect new default
- [x] All 37 consult tests pass
- [x] Build succeeds
- [x] Full test suite passes (1 pre-existing flaky test in `send-integration.test.ts` — unrelated)
