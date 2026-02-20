#!/bin/bash
# Master launch script: starts SWE-forge mining pipeline + auto-publish to HuggingFace
# Generates 30+ datasets and uploads them every 30 minutes
set -euo pipefail

export OPENROUTER_API_KEY="${OPENROUTER_API_KEY:?Set OPENROUTER_API_KEY}"
export GITHUB_TOKEN="${GITHUB_TOKEN:?Set GITHUB_TOKEN}"
export HF_TOKEN="${HF_TOKEN:?Set HF_TOKEN}"
export HF_REPO="CortexLM/swe-forge"
export OUTPUT_DIR="generated-swe"
export RUST_LOG="info"

BINARY="./target/release/swe-forge"
MAX_TASKS=30
CONCURRENCY_DEEP=8
CONCURRENCY_ENRICH=30
CONCURRENCY_PRECLASSIFY=30
BACKLOG_MULTIPLIER=8

echo "[$(date)] === SWE-Forge Generation Pipeline ==="
echo "[$(date)] Max tasks: ${MAX_TASKS}"
echo "[$(date)] Docker concurrency: ${CONCURRENCY_DEEP}"
echo "[$(date)] Enrichment concurrency: ${CONCURRENCY_ENRICH}"
echo "[$(date)] Pre-classify concurrency: ${CONCURRENCY_PRECLASSIFY}"
echo "[$(date)] Output: ${OUTPUT_DIR}"
echo "[$(date)] HF repo: ${HF_REPO}"

# Ensure output directory exists
mkdir -p "${OUTPUT_DIR}"

# Build release binary if not present
if [ ! -f "${BINARY}" ]; then
    echo "[$(date)] Building release binary..."
    cargo build --release 2>&1 | tail -5
fi

echo "[$(date)] Starting mining pipeline (nohup)..."
nohup ${BINARY} swe mine \
    --max-tasks ${MAX_TASKS} \
    --output "${OUTPUT_DIR}" \
    --concurrency-deep ${CONCURRENCY_DEEP} \
    --concurrency-enrich ${CONCURRENCY_ENRICH} \
    --concurrency-preclassify ${CONCURRENCY_PRECLASSIFY} \
    --backlog-multiplier ${BACKLOG_MULTIPLIER} \
    --hf-repo "${HF_REPO}" \
    --hf-token "${HF_TOKEN}" \
    --cache-db swe_cache.db \
    --pr-file processed_prs.jsonl \
    > mining.log 2>&1 &

MINING_PID=$!
echo "[$(date)] Mining pipeline started (PID: ${MINING_PID})"

# Wait a moment for the pipeline to initialize
sleep 5

echo "[$(date)] Starting auto-publish to HuggingFace (every 30 min)..."
nohup python3 auto_publish.py > auto_publish.log 2>&1 &
PUBLISH_PID=$!
echo "[$(date)] Auto-publish started (PID: ${PUBLISH_PID})"

echo ""
echo "=== Processes Running ==="
echo "Mining PID:  ${MINING_PID} (log: mining.log)"
echo "Publish PID: ${PUBLISH_PID} (log: auto_publish.log)"
echo ""
echo "Monitor with:"
echo "  tail -f mining.log"
echo "  tail -f auto_publish.log"
echo ""
echo "Check generated tasks:"
echo "  find ${OUTPUT_DIR} -name workspace.yaml | wc -l"
echo ""
echo "Stop all:"
echo "  kill ${MINING_PID} ${PUBLISH_PID}"
