#!/bin/bash
# Auto-publish script: uploads task directories to HuggingFace every 30 minutes
# The pipeline already uploads parquet shards in real-time.
# This script handles uploading the task workspace directories (prompt.md, workspace.yaml, tests/)
# that accumulate in generated-swe/ as tasks complete.

HF_TOKEN="${HF_TOKEN:?Set HF_TOKEN environment variable}"
HF_REPO="CortexLM/swe-forge"
OUTPUT_DIR="generated-swe"
UPLOADED_MARKER=".hf_uploaded"
INTERVAL=1800  # 30 minutes

upload_task_dir() {
    local task_dir="$1"
    local task_rel="$2"  # relative path from OUTPUT_DIR (e.g. osism/container-image-inventory-reconciler-489)
    local task_id=$(echo "$task_rel" | tr '/' '-')
    
    echo "[$(date)] Uploading task: $task_rel"
    
    find "$task_dir" -type f ! -name "$UPLOADED_MARKER" | while read -r filepath; do
        local rel_path="${filepath#$task_dir/}"
        local repo_path="tasks/${task_rel}/${rel_path}"
        local content_b64=$(base64 -w0 "$filepath")
        
        curl -s -X POST "https://huggingface.co/api/datasets/${HF_REPO}/commit/main" \
            -H "Authorization: Bearer ${HF_TOKEN}" \
            -H "Content-Type: application/json" \
            -d "{\"summary\":\"Add task ${task_id}\",\"actions\":[{\"action\":\"file\",\"path\":\"${repo_path}\",\"content\":\"${content_b64}\",\"encoding\":\"base64\"}]}" \
            > /dev/null 2>&1
    done
    
    touch "${task_dir}/${UPLOADED_MARKER}"
    echo "[$(date)] Uploaded task: $task_rel"
}

echo "[$(date)] Auto-publish started (interval: ${INTERVAL}s)"

while true; do
    if [ -d "$OUTPUT_DIR" ]; then
        # Find task directories recursively (they contain workspace.yaml)
        find "$OUTPUT_DIR" -name "workspace.yaml" -type f 2>/dev/null | while read -r ws_file; do
            task_dir=$(dirname "$ws_file")
            [ -f "${task_dir}/${UPLOADED_MARKER}" ] && continue
            # Compute relative path from OUTPUT_DIR
            task_rel="${task_dir#$OUTPUT_DIR/}"
            upload_task_dir "$task_dir" "$task_rel"
        done
        
        task_count=$(find "$OUTPUT_DIR" -name "workspace.yaml" -type f 2>/dev/null | wc -l)
        uploaded_count=$(find "$OUTPUT_DIR" -name "$UPLOADED_MARKER" -type f 2>/dev/null | wc -l)
        echo "[$(date)] Status: ${uploaded_count}/${task_count} tasks uploaded to HF"
    else
        echo "[$(date)] Output directory not found yet: $OUTPUT_DIR"
    fi
    
    echo "[$(date)] Sleeping ${INTERVAL}s until next publish cycle..."
    sleep $INTERVAL
done
