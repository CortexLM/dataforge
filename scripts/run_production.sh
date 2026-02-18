#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# SWE-Forge Production Runner
# Launches the SWE mining pipeline via nohup with monitoring, crash detection,
# and progress reporting.
# ============================================================================

# --- Configuration (override via environment) ---
OUTPUT_DIR="${SWE_OUTPUT_DIR:-./generated-swe}"
DIFFICULTY_TARGETS="${SWE_DIFFICULTY_TARGETS:-easy:50,medium:50,hard:50}"
CACHE_DB="${SWE_CACHE_DB:-swe_cache.db}"
PR_FILE="${SWE_PR_FILE:-./processed.jsonl}"
LOG_FILE="${SWE_LOG_FILE:-mine_output.log}"
PID_FILE="${SWE_PID_FILE:-.swe_mine.pid}"
MONITOR_INTERVAL="${SWE_MONITOR_INTERVAL:-60}"
STALL_TIMEOUT="${SWE_STALL_TIMEOUT:-1800}"  # 30 minutes
MAX_RESTARTS="${SWE_MAX_RESTARTS:-3}"
LOG_MAX_SIZE="${SWE_LOG_MAX_SIZE:-104857600}"  # 100MB
BINARY="${SWE_BINARY:-cargo run --release --}"
EXTRA_ARGS="${SWE_EXTRA_ARGS:-}"

# --- State ---
RESTART_COUNT=0
SHUTDOWN_REQUESTED=0

# --- Signal handlers ---
cleanup() {
    SHUTDOWN_REQUESTED=1
    echo "[$(date -Iseconds)] Shutdown requested, stopping pipeline..."
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null || true
            sleep 5
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi
    write_summary
    echo "[$(date -Iseconds)] Shutdown complete."
    exit 0
}

trap cleanup SIGINT SIGTERM SIGHUP

# --- Helpers ---
log() {
    echo "[$(date -Iseconds)] $*"
}

rotate_log() {
    if [ -f "$LOG_FILE" ]; then
        local size
        size=$(stat -f%z "$LOG_FILE" 2>/dev/null || stat -c%s "$LOG_FILE" 2>/dev/null || echo 0)
        if [ "$size" -gt "$LOG_MAX_SIZE" ]; then
            local rotated="${LOG_FILE}.$(date +%Y%m%d_%H%M%S)"
            mv "$LOG_FILE" "$rotated"
            gzip "$rotated" &
            log "Log rotated: $rotated.gz"
        fi
    fi
}

count_accepted() {
    local level="$1"
    if [ -f "$LOG_FILE" ]; then
        grep -c "difficulty.*${level}.*Task accepted" "$LOG_FILE" 2>/dev/null || echo 0
    else
        echo 0
    fi
}

count_exported() {
    if [ -f "$LOG_FILE" ]; then
        grep -c "Exported task to disk" "$LOG_FILE" 2>/dev/null || echo 0
    else
        echo 0
    fi
}

is_running() {
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE")
        kill -0 "$pid" 2>/dev/null
        return $?
    fi
    return 1
}

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g'
}

write_summary() {
    local easy_count medium_count hard_count total_exported
    easy_count=$(count_accepted "easy")
    medium_count=$(count_accepted "medium")
    hard_count=$(count_accepted "hard")
    total_exported=$(count_exported)

    local escaped_log_file escaped_output_dir
    escaped_log_file=$(json_escape "$LOG_FILE")
    escaped_output_dir=$(json_escape "$OUTPUT_DIR")

    cat > "${OUTPUT_DIR}/run_summary.json" <<EOF
{
  "timestamp": "$(date -Iseconds)",
  "status": "$([ "$SHUTDOWN_REQUESTED" -eq 1 ] && echo "stopped" || echo "completed")",
  "restart_count": $RESTART_COUNT,
  "total_exported": $total_exported,
  "per_difficulty": {
    "easy": $easy_count,
    "medium": $medium_count,
    "hard": $hard_count
  },
  "log_file": "$escaped_log_file",
  "output_dir": "$escaped_output_dir"
}
EOF
    log "Summary written to ${OUTPUT_DIR}/run_summary.json"
}

start_pipeline() {
    mkdir -p "$OUTPUT_DIR"

    log "Starting SWE mining pipeline..."
    log "  Output: $OUTPUT_DIR"
    log "  Targets: $DIFFICULTY_TARGETS"
    log "  Cache DB: $CACHE_DB"
    log "  Log: $LOG_FILE"

    # shellcheck disable=SC2086
    nohup $BINARY swe mine \
        --output "$OUTPUT_DIR" \
        --difficulty-targets "$DIFFICULTY_TARGETS" \
        --cache-db "$CACHE_DB" \
        --pr-file "$PR_FILE" \
        --validate-workspace \
        -j \
        $EXTRA_ARGS \
        > "$LOG_FILE" 2>&1 &

    local pid=$!
    echo "$pid" > "$PID_FILE"
    log "Pipeline started with PID $pid"
}

# --- Main ---
log "=== SWE-Forge Production Runner ==="
log "Configuration:"
log "  DIFFICULTY_TARGETS=$DIFFICULTY_TARGETS"
log "  OUTPUT_DIR=$OUTPUT_DIR"
log "  MONITOR_INTERVAL=${MONITOR_INTERVAL}s"
log "  STALL_TIMEOUT=${STALL_TIMEOUT}s"
log "  MAX_RESTARTS=$MAX_RESTARTS"

start_pipeline

# --- Monitoring loop ---
LAST_EXPORTED=0
LAST_CHANGE_TIME=$(date +%s)

while true; do
    sleep "$MONITOR_INTERVAL"

    # Check for shutdown
    if [ "$SHUTDOWN_REQUESTED" -eq 1 ]; then
        break
    fi

    # Rotate logs if needed
    rotate_log

    # Check if process is still running
    if ! is_running; then
        log "Pipeline process is no longer running!"

        # Check if it completed successfully
        if [ -f "$LOG_FILE" ] && grep -q "Pipeline.*completed\|SWE mine completed\|All difficulty targets met" "$LOG_FILE" 2>/dev/null; then
            log "Pipeline completed successfully."
            write_summary
            break
        fi

        # Crash detected
        RESTART_COUNT=$((RESTART_COUNT + 1))
        if [ "$RESTART_COUNT" -gt "$MAX_RESTARTS" ]; then
            log "ERROR: Max restarts ($MAX_RESTARTS) exceeded. Giving up."
            write_summary
            exit 1
        fi

        log "Crash detected (restart $RESTART_COUNT/$MAX_RESTARTS). Restarting in 10s..."
        sleep 10
        start_pipeline
        LAST_CHANGE_TIME=$(date +%s)
        continue
    fi

    # Progress report
    current_exported=$(count_exported)
    easy_count=$(count_accepted "easy")
    medium_count=$(count_accepted "medium")
    hard_count=$(count_accepted "hard")

    log "Progress: exported=$current_exported easy=$easy_count medium=$medium_count hard=$hard_count"

    # Stall detection
    if [ "$current_exported" -ne "$LAST_EXPORTED" ]; then
        LAST_EXPORTED=$current_exported
        LAST_CHANGE_TIME=$(date +%s)
    else
        now=$(date +%s)
        stall_duration=$((now - LAST_CHANGE_TIME))
        if [ "$stall_duration" -gt "$STALL_TIMEOUT" ]; then
            log "WARNING: Pipeline stalled for ${stall_duration}s (no new exports)"
            # Show last few log lines for debugging
            tail -5 "$LOG_FILE" 2>/dev/null | while IFS= read -r line; do
                log "  | $line"
            done
        fi
    fi

    # Throughput estimation
    if [ "$current_exported" -gt 0 ]; then
        if [ -f "$PID_FILE" ]; then
            pid_time=$(stat -c%Y "$PID_FILE" 2>/dev/null || stat -f%m "$PID_FILE" 2>/dev/null || date +%s)
            now=$(date +%s)
            elapsed_since_start=$((now - pid_time))
            if [ "$elapsed_since_start" -gt 0 ]; then
                rate=$(echo "scale=2; $current_exported / $elapsed_since_start * 3600" | bc 2>/dev/null || echo "?")
                # Parse total target from DIFFICULTY_TARGETS
                total_target=0
                IFS=',' read -ra PAIRS <<< "$DIFFICULTY_TARGETS"
                for pair in "${PAIRS[@]}"; do
                    count=$(echo "$pair" | cut -d: -f2 | tr -d ' ')
                    total_target=$((total_target + count))
                done
                remaining_tasks=$((total_target - current_exported))
                if [ "$remaining_tasks" -gt 0 ] && [ "$current_exported" -gt 0 ]; then
                    eta_secs=$(echo "scale=0; $remaining_tasks * $elapsed_since_start / $current_exported" | bc 2>/dev/null || echo "?")
                    log "  Throughput: ~${rate} tasks/hour | ETA: ~${eta_secs}s remaining"
                fi
            fi
        fi
    fi
done

write_summary
log "=== Runner finished ==="
