# AGENTS.md — src/utils/

## Purpose

Shared utility functions used across modules, primarily for extracting structured JSON from LLM responses.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports |
| `json_extraction.rs` | JSON extraction from LLM responses: code blocks, regex, brace matching, truncation detection |

## Key Functions

- `extract_json_from_response(text)` — Primary extraction (tries code blocks, then regex, then brace matching)
- `try_extract_json_from_response(text)` — Returns `Option` instead of `Result`
- `extract_from_json_code_block(text)` — Extracts from ` ```json ... ``` ` blocks
- `extract_from_generic_code_block(text)` — Extracts from ` ``` ... ``` ` blocks
- `extract_json_with_regex(text)` — Regex-based JSON object extraction
- `find_matching_brace(text, start)` / `find_matching_bracket(text, start)` — Balanced delimiter matching
- `detect_truncated_json(text)` — Detects incomplete JSON responses
- `analyze_json_structure(text)` — Returns `JsonStructureAnalysis` with depth, key count, etc.

## Rules

- Prefer function calling over JSON extraction — these are fallback utilities
- `JsonExtractionError` should be used for all extraction failures
