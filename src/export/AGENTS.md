# AGENTS.md — src/export/

## Purpose

Dataset export module. Writes SWE task data to Apache Parquet format and uploads to HuggingFace Hub.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports |
| `dataset.rs` | `DatasetManager` — load, download, and manage datasets; `DatasetConfig`, `DatasetSummary` |
| `parquet_writer.rs` | `write_parquet()`, `read_parquet()`, `write_parquet_bytes()` — Arrow/Parquet serialization |
| `hf_uploader.rs` | `HfUploader` — HuggingFace Hub API upload with `HfUploadConfig` |

## Key Types

- `DatasetManager` / `DatasetConfig` / `DatasetSummary`
- `HfUploader` / `HfUploadConfig`
- `write_parquet(tasks, path)` / `read_parquet(path)` / `write_parquet_bytes(tasks)` — core I/O functions
- `download_dataset(url, path)` / `load_dataset(path)` — dataset retrieval utilities

## Rules

- Never change Parquet schema fields without updating both `write_parquet` and `read_parquet`
- HuggingFace upload requires `HF_TOKEN` environment variable
- Parquet uses snappy + zstd compression (configured in `Cargo.toml` features)
