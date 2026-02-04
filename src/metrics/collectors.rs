//! Custom metric collectors for dataforge operations.
//!
//! This module provides a high-level interface for recording various metrics
//! throughout the application. The `MetricsCollector` struct wraps the raw
//! Prometheus metrics and provides convenient methods for common operations.

use super::prometheus::{
    ACTIVE_WORKERS, JOBS_IN_PROGRESS, LLM_COST_CENTS, LLM_LATENCY, LLM_REQUESTS_TOTAL,
    LLM_TOKENS_TOTAL, QUALITY_FILTERED, QUALITY_SCORE, QUEUE_DEPTH, TASKS_TOTAL, TASK_DURATION,
};

/// Token usage information for LLM requests.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    /// Number of input/prompt tokens.
    pub input_tokens: u64,
    /// Number of output/completion tokens.
    pub output_tokens: u64,
}

impl TokenUsage {
    /// Create a new TokenUsage instance.
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
        }
    }

    /// Get the total number of tokens (input + output).
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Metrics collector for recording dataforge operational metrics.
///
/// This struct provides a convenient interface for recording metrics throughout
/// the application. It wraps the underlying Prometheus metrics and ensures
/// consistent labeling and error handling.
///
/// # Example
///
/// ```ignore
/// use dataforge::metrics::{MetricsCollector, TokenUsage, init_metrics};
///
/// init_metrics().expect("Failed to init metrics");
/// let collector = MetricsCollector::new();
///
/// // Record a successful task
/// collector.record_task("success", "medium", "gpt-4", 120.5);
///
/// // Record LLM usage
/// let tokens = TokenUsage::new(1000, 500);
/// collector.record_llm_request("gpt-4", true, 2.5, tokens, 15);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MetricsCollector;

impl MetricsCollector {
    /// Create a new MetricsCollector instance.
    ///
    /// Note: Metrics must be initialized with `init_metrics()` before
    /// calling any recording methods.
    pub fn new() -> Self {
        Self
    }

    /// Record a task execution.
    ///
    /// # Arguments
    ///
    /// * `status` - Task completion status (e.g., "success", "failure", "timeout")
    /// * `difficulty` - Task difficulty level (e.g., "easy", "medium", "hard")
    /// * `model` - LLM model used for the task
    /// * `duration_secs` - Task execution duration in seconds
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.record_task("success", "medium", "gpt-4", 120.5);
    /// ```
    pub fn record_task(&self, status: &str, difficulty: &str, model: &str, duration_secs: f64) {
        if let Some(tasks_total) = TASKS_TOTAL.get() {
            tasks_total
                .with_label_values(&[status, difficulty, model])
                .inc();
        }

        if let Some(task_duration) = TASK_DURATION.get() {
            task_duration
                .with_label_values(&[difficulty])
                .observe(duration_secs);
        }

        tracing::trace!(
            status = status,
            difficulty = difficulty,
            model = model,
            duration_secs = duration_secs,
            "Recorded task metric"
        );
    }

    /// Record an LLM API request.
    ///
    /// # Arguments
    ///
    /// * `model` - LLM model identifier
    /// * `success` - Whether the request succeeded
    /// * `latency_secs` - Request latency in seconds
    /// * `tokens` - Token usage for the request
    /// * `cost_cents` - Cost of the request in cents
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// let tokens = TokenUsage::new(1000, 500);
    /// collector.record_llm_request("gpt-4", true, 2.5, tokens, 15);
    /// ```
    pub fn record_llm_request(
        &self,
        model: &str,
        success: bool,
        latency_secs: f64,
        tokens: TokenUsage,
        cost_cents: u64,
    ) {
        let status = if success { "success" } else { "failure" };

        if let Some(llm_requests) = LLM_REQUESTS_TOTAL.get() {
            llm_requests.with_label_values(&[model, status]).inc();
        }

        if let Some(llm_latency) = LLM_LATENCY.get() {
            llm_latency
                .with_label_values(&[model])
                .observe(latency_secs);
        }

        if let Some(llm_tokens) = LLM_TOKENS_TOTAL.get() {
            llm_tokens
                .with_label_values(&[model, "input"])
                .inc_by(tokens.input_tokens as f64);
            llm_tokens
                .with_label_values(&[model, "output"])
                .inc_by(tokens.output_tokens as f64);
        }

        if let Some(llm_cost) = LLM_COST_CENTS.get() {
            llm_cost
                .with_label_values(&[model])
                .inc_by(cost_cents as f64);
        }

        tracing::trace!(
            model = model,
            status = status,
            latency_secs = latency_secs,
            input_tokens = tokens.input_tokens,
            output_tokens = tokens.output_tokens,
            cost_cents = cost_cents,
            "Recorded LLM request metric"
        );
    }

    /// Record a quality score measurement.
    ///
    /// # Arguments
    ///
    /// * `score` - Quality score between 0.0 and 1.0
    /// * `passed` - Whether the trajectory passed quality filtering
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.record_quality(0.85, true);
    /// collector.record_quality(0.3, false);
    /// ```
    pub fn record_quality(&self, score: f64, passed: bool) {
        if let Some(quality_score) = QUALITY_SCORE.get() {
            quality_score.observe(score);
        }

        if !passed {
            if let Some(quality_filtered) = QUALITY_FILTERED.get() {
                quality_filtered.inc();
            }
        }

        tracing::trace!(score = score, passed = passed, "Recorded quality metric");
    }

    /// Update the queue depth for a specific queue.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - Name of the queue
    /// * `depth` - Current number of items in the queue
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.update_queue_depth("pending_tasks", 42);
    /// collector.update_queue_depth("completed_tasks", 100);
    /// ```
    pub fn update_queue_depth(&self, queue_name: &str, depth: usize) {
        if let Some(queue_depth) = QUEUE_DEPTH.get() {
            queue_depth
                .with_label_values(&[queue_name])
                .set(depth as f64);
        }

        tracing::trace!(
            queue_name = queue_name,
            depth = depth,
            "Updated queue depth metric"
        );
    }

    /// Update the count of active workers.
    ///
    /// # Arguments
    ///
    /// * `count` - Current number of active workers
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.update_workers(8);
    /// ```
    pub fn update_workers(&self, count: usize) {
        if let Some(active_workers) = ACTIVE_WORKERS.get() {
            active_workers.set(count as f64);
        }

        tracing::trace!(count = count, "Updated active workers metric");
    }

    /// Update the count of jobs currently in progress.
    ///
    /// # Arguments
    ///
    /// * `count` - Current number of jobs being processed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.update_jobs_in_progress(5);
    /// ```
    pub fn update_jobs_in_progress(&self, count: usize) {
        if let Some(jobs_in_progress) = JOBS_IN_PROGRESS.get() {
            jobs_in_progress.set(count as f64);
        }

        tracing::trace!(count = count, "Updated jobs in progress metric");
    }

    /// Increment the count of jobs in progress by 1.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.inc_jobs_in_progress();
    /// ```
    pub fn inc_jobs_in_progress(&self) {
        if let Some(jobs_in_progress) = JOBS_IN_PROGRESS.get() {
            jobs_in_progress.inc();
        }
    }

    /// Decrement the count of jobs in progress by 1.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let collector = MetricsCollector::new();
    /// collector.dec_jobs_in_progress();
    /// ```
    pub fn dec_jobs_in_progress(&self) {
        if let Some(jobs_in_progress) = JOBS_IN_PROGRESS.get() {
            jobs_in_progress.dec();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::init_metrics;

    fn ensure_metrics_init() {
        // Initialize metrics if not already done
        let _ = init_metrics();
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::new(1000, 500);
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.total(), 1500);
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total(), 0);
    }

    #[test]
    fn test_metrics_collector_new() {
        let collector = MetricsCollector::new();
        // Just verify it can be created
        assert!(std::mem::size_of_val(&collector) == 0);
    }

    #[test]
    fn test_record_task() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        // Should not panic even if metrics aren't fully initialized
        collector.record_task("success", "medium", "gpt-4", 120.5);
        collector.record_task("failure", "hard", "claude-3", 60.0);
        collector.record_task("timeout", "easy", "gpt-3.5", 300.0);
    }

    #[test]
    fn test_record_llm_request() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        let tokens = TokenUsage::new(1000, 500);
        collector.record_llm_request("gpt-4", true, 2.5, tokens, 15);

        let tokens = TokenUsage::new(2000, 1000);
        collector.record_llm_request("claude-3", false, 5.0, tokens, 30);
    }

    #[test]
    fn test_record_quality() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        collector.record_quality(0.85, true);
        collector.record_quality(0.3, false);
        collector.record_quality(0.99, true);
    }

    #[test]
    fn test_update_queue_depth() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        collector.update_queue_depth("pending_tasks", 42);
        collector.update_queue_depth("completed_tasks", 100);
        collector.update_queue_depth("pending_tasks", 40);
    }

    #[test]
    fn test_update_workers() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        collector.update_workers(8);
        collector.update_workers(10);
        collector.update_workers(4);
    }

    #[test]
    fn test_update_jobs_in_progress() {
        ensure_metrics_init();
        let collector = MetricsCollector::new();

        collector.update_jobs_in_progress(5);
        collector.inc_jobs_in_progress();
        collector.dec_jobs_in_progress();
    }
}
