#!/usr/bin/env python3
"""
Auto-publish script: uploads task directories to HuggingFace every 30 minutes.

Finds all task directories (containing workspace.yaml) under the output dir,
and uploads the ENTIRE directory tree including tests/ subdirectory to HF.
Uses huggingface_hub for reliable uploads with proper batching.
"""

import os
import sys
import time
import logging
from pathlib import Path

from huggingface_hub import HfApi, CommitOperationAdd

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("auto_publish")

HF_TOKEN = os.environ.get("HF_TOKEN", "")
HF_REPO = os.environ.get("HF_REPO", "CortexLM/swe-forge")
OUTPUT_DIR = os.environ.get("OUTPUT_DIR", "generated-swe")
UPLOADED_MARKER = ".hf_uploaded"
INTERVAL = int(os.environ.get("PUBLISH_INTERVAL", "1800"))  # 30 minutes


def find_task_dirs(base_dir: str) -> list[Path]:
    """Find all directories containing workspace.yaml (task directories)."""
    tasks = []
    for ws in Path(base_dir).rglob("workspace.yaml"):
        tasks.append(ws.parent)
    return tasks


def collect_files_for_upload(task_dir: Path, repo_prefix: str) -> list[tuple[str, Path]]:
    """Collect all files in a task directory for upload, including tests/ subdirectory."""
    files = []
    for filepath in task_dir.rglob("*"):
        if filepath.is_file() and filepath.name != UPLOADED_MARKER:
            rel = filepath.relative_to(task_dir)
            repo_path = f"{repo_prefix}/{rel}"
            files.append((repo_path, filepath))
    return files


def upload_tasks(api: HfApi):
    """Find and upload all new task directories to HuggingFace."""
    if not os.path.isdir(OUTPUT_DIR):
        log.info(f"Output directory not found yet: {OUTPUT_DIR}")
        return

    task_dirs = find_task_dirs(OUTPUT_DIR)
    if not task_dirs:
        log.info("No task directories found yet")
        return

    new_tasks = [d for d in task_dirs if not (d / UPLOADED_MARKER).exists()]
    if not new_tasks:
        log.info(f"All {len(task_dirs)} tasks already uploaded")
        return

    log.info(f"Found {len(new_tasks)} new tasks to upload (total: {len(task_dirs)})")

    for task_dir in new_tasks:
        try:
            rel = task_dir.relative_to(OUTPUT_DIR)
            task_id = str(rel).replace(os.sep, "/")
            repo_prefix = f"tasks/{task_id}"

            file_pairs = collect_files_for_upload(task_dir, repo_prefix)
            if not file_pairs:
                log.warning(f"No files found in task dir: {task_dir}")
                continue

            has_tests_dir = any("tests/" in rp for rp, _ in file_pairs)
            test_file_count = sum(1 for rp, _ in file_pairs if "tests/" in rp)

            log.info(
                f"Uploading task {task_id}: {len(file_pairs)} files "
                f"(tests/ dir: {'yes' if has_tests_dir else 'NO'}, "
                f"test files: {test_file_count})"
            )

            operations = []
            for repo_path, local_path in file_pairs:
                operations.append(
                    CommitOperationAdd(
                        path_in_repo=repo_path,
                        path_or_fileobj=str(local_path),
                    )
                )

            # Batch upload all files in a single commit
            api.create_commit(
                repo_id=HF_REPO,
                repo_type="dataset",
                operations=operations,
                commit_message=f"Add task {task_id} ({len(file_pairs)} files, {test_file_count} test files)",
            )

            # Mark as uploaded
            (task_dir / UPLOADED_MARKER).touch()
            log.info(f"Successfully uploaded task: {task_id}")

        except Exception as e:
            log.error(f"Failed to upload task {task_dir}: {e}")
            continue

    uploaded = sum(1 for d in task_dirs if (d / UPLOADED_MARKER).exists())
    log.info(f"Upload status: {uploaded}/{len(task_dirs)} tasks uploaded to HF")


def main():
    if not HF_TOKEN:
        log.error("HF_TOKEN environment variable is required")
        sys.exit(1)

    log.info(f"Auto-publish started")
    log.info(f"  HF repo: {HF_REPO}")
    log.info(f"  Output dir: {OUTPUT_DIR}")
    log.info(f"  Interval: {INTERVAL}s ({INTERVAL // 60} min)")

    api = HfApi(token=HF_TOKEN)

    # Verify repo exists
    try:
        api.repo_info(repo_id=HF_REPO, repo_type="dataset")
        log.info(f"HF dataset repo exists: {HF_REPO}")
    except Exception:
        log.info(f"Creating HF dataset repo: {HF_REPO}")
        try:
            api.create_repo(repo_id=HF_REPO, repo_type="dataset", exist_ok=True)
        except Exception as e:
            log.warning(f"Could not create repo (may already exist): {e}")

    while True:
        try:
            upload_tasks(api)
        except Exception as e:
            log.error(f"Upload cycle failed: {e}")

        log.info(f"Sleeping {INTERVAL}s until next publish cycle...")
        time.sleep(INTERVAL)


if __name__ == "__main__":
    main()
