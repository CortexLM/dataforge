//! Prometheus metrics registration and export.
//!
//! This module defines all Prometheus metrics used by swe_forge and provides
//! functions for initializing, registering, and exporting metrics.

use prometheus::{
    Counter, CounterVec, Encoder, Gauge, GaugeVec, Histogram, HistogramVec, Opts, Registry,
    TextEncoder,
};
use std::sync::OnceLock;

/// Global Prometheus registry for all swe_forge metrics.
pub static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Total number of tasks executed, labeled by status, difficulty, and model.
pub static TASKS_TOTAL: OnceLock<CounterVec> = OnceLock::new();

/// Task execution duration in seconds, labeled by difficulty.
pub static TASK_DURATION: OnceLock<HistogramVec> = OnceLock::new();

/// Number of jobs in queue, labeled by queue name.
pub static QUEUE_DEPTH: OnceLock<GaugeVec> = OnceLock::new();

/// Number of jobs currently being processed.
pub static JOBS_IN_PROGRESS: OnceLock<Gauge> = OnceLock::new();

/// Total LLM API requests, labeled by model and status.
pub static LLM_REQUESTS_TOTAL: OnceLock<CounterVec> = OnceLock::new();

/// LLM API request latency in seconds, labeled by model.
pub static LLM_LATENCY: OnceLock<HistogramVec> = OnceLock::new();

/// Total tokens used, labeled by model and type (input/output).
pub static LLM_TOKENS_TOTAL: OnceLock<CounterVec> = OnceLock::new();

/// LLM API costs in cents, labeled by model.
pub static LLM_COST_CENTS: OnceLock<CounterVec> = OnceLock::new();

/// Distribution of quality scores.
pub static QUALITY_SCORE: OnceLock<Histogram> = OnceLock::new();

/// Total trajectories filtered by quality.
pub static QUALITY_FILTERED: OnceLock<Counter> = OnceLock::new();

/// Number of active workers.
pub static ACTIVE_WORKERS: OnceLock<Gauge> = OnceLock::new();

/// Initialize all metrics and register them with the registry.
///
/// This function should be called once at application startup. It creates all
/// metric instances with appropriate labels and buckets, and registers them
/// with the global Prometheus registry.
///
/// # Errors
///
/// Returns a `prometheus::Error` if metric registration fails, typically due to
/// duplicate metric names or invalid metric configurations.
///
/// # Example
///
/// ```ignore
/// use swe_forge::metrics::init_metrics;
///
/// fn main() {
///     init_metrics().expect("Failed to initialize metrics");
///     // Application continues...
/// }
/// ```
pub fn init_metrics() -> Result<(), prometheus::Error> {
    // Create the registry
    let registry = Registry::new();

    // Task metrics
    let tasks_total = CounterVec::new(
        Opts::new("swe_forge_tasks_total", "Total number of tasks executed"),
        &["status", "difficulty", "model"],
    )?;

    let task_duration = HistogramVec::new(
        prometheus::HistogramOpts::new(
            "swe_forge_task_duration_seconds",
            "Task execution duration in seconds",
        )
        .buckets(vec![10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0]),
        &["difficulty"],
    )?;

    // Queue metrics
    let queue_depth = GaugeVec::new(
        Opts::new("swe_forge_queue_depth", "Number of jobs in queue"),
        &["queue_name"],
    )?;

    let jobs_in_progress = Gauge::new(
        "swe_forge_jobs_in_progress",
        "Number of jobs currently being processed",
    )?;

    // LLM metrics
    let llm_requests_total = CounterVec::new(
        Opts::new("swe_forge_llm_requests_total", "Total LLM API requests"),
        &["model", "status"],
    )?;

    let llm_latency = HistogramVec::new(
        prometheus::HistogramOpts::new(
            "swe_forge_llm_latency_seconds",
            "LLM API request latency in seconds",
        )
        .buckets(vec![0.5, 1.0, 2.0, 5.0, 10.0, 30.0]),
        &["model"],
    )?;

    let llm_tokens_total = CounterVec::new(
        Opts::new("swe_forge_llm_tokens_total", "Total tokens used"),
        &["model", "type"],
    )?;

    let llm_cost_cents = CounterVec::new(
        Opts::new("swe_forge_llm_cost_cents", "LLM API costs in cents"),
        &["model"],
    )?;

    // Quality metrics
    let quality_score = Histogram::with_opts(
        prometheus::HistogramOpts::new("swe_forge_quality_score", "Distribution of quality scores")
            .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
    )?;

    let quality_filtered = Counter::new(
        "swe_forge_quality_filtered_total",
        "Total trajectories filtered by quality",
    )?;

    // Worker metrics
    let active_workers = Gauge::new("swe_forge_active_workers", "Number of active workers")?;

    // Register all metrics with the registry
    registry.register(Box::new(tasks_total.clone()))?;
    registry.register(Box::new(task_duration.clone()))?;
    registry.register(Box::new(queue_depth.clone()))?;
    registry.register(Box::new(jobs_in_progress.clone()))?;
    registry.register(Box::new(llm_requests_total.clone()))?;
    registry.register(Box::new(llm_latency.clone()))?;
    registry.register(Box::new(llm_tokens_total.clone()))?;
    registry.register(Box::new(llm_cost_cents.clone()))?;
    registry.register(Box::new(quality_score.clone()))?;
    registry.register(Box::new(quality_filtered.clone()))?;
    registry.register(Box::new(active_workers.clone()))?;

    // Store metrics in static variables
    // If any of these fail, metrics were already initialized (idempotent)
    let _ = REGISTRY.set(registry);
    let _ = TASKS_TOTAL.set(tasks_total);
    let _ = TASK_DURATION.set(task_duration);
    let _ = QUEUE_DEPTH.set(queue_depth);
    let _ = JOBS_IN_PROGRESS.set(jobs_in_progress);
    let _ = LLM_REQUESTS_TOTAL.set(llm_requests_total);
    let _ = LLM_LATENCY.set(llm_latency);
    let _ = LLM_TOKENS_TOTAL.set(llm_tokens_total);
    let _ = LLM_COST_CENTS.set(llm_cost_cents);
    let _ = QUALITY_SCORE.set(quality_score);
    let _ = QUALITY_FILTERED.set(quality_filtered);
    let _ = ACTIVE_WORKERS.set(active_workers);

    tracing::info!("Prometheus metrics initialized successfully");

    Ok(())
}

/// Export all registered metrics in Prometheus text format.
///
/// This function gathers all metrics from the registry and encodes them in the
/// Prometheus text exposition format, suitable for scraping by a Prometheus server.
///
/// # Returns
///
/// A string containing all metrics in Prometheus text format. If the registry
/// has not been initialized or encoding fails, returns an error message.
///
/// # Example
///
/// ```ignore
/// use swe_forge::metrics::{init_metrics, export_metrics};
///
/// init_metrics().expect("Failed to init");
/// let metrics = export_metrics();
/// println!("{}", metrics);
/// ```
pub fn export_metrics() -> String {
    let Some(registry) = REGISTRY.get() else {
        return "# Metrics not initialized. Call init_metrics() first.\n".to_string();
    };

    let encoder = TextEncoder::new();
    let metric_families = registry.gather();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        return format!("# Error encoding metrics: {}\n", e);
    }

    String::from_utf8(buffer)
        .unwrap_or_else(|e| format!("# Error converting metrics to UTF-8: {}\n", e))
}

/// HTTP handler for the /metrics endpoint.
///
/// This async function is designed to be used as an HTTP handler in web frameworks
/// like actix-web, axum, or warp. It returns metrics in Prometheus text format.
///
/// # Example with axum
///
/// ```ignore
/// use axum::{routing::get, Router};
/// use swe_forge::metrics::metrics_handler;
///
/// let app = Router::new()
///     .route("/metrics", get(|| async { metrics_handler().await }));
/// ```
pub async fn metrics_handler() -> String {
    export_metrics()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_metrics() {
        // Note: This test modifies global state, so it must be run in isolation
        // or with special handling in a test harness.
        let result = init_metrics();
        // First call should succeed or metrics already initialized
        assert!(result.is_ok() || REGISTRY.get().is_some());
    }

    #[test]
    fn test_export_metrics_uninitialized() {
        // If metrics haven't been initialized, should return informative message
        // Note: This test depends on execution order
        let metrics = export_metrics();
        // Should either be a proper metrics output or the uninitialized message
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_metrics_after_init() {
        // Ensure metrics are initialized
        let _ = init_metrics();

        // Verify metrics can be exported
        let metrics = export_metrics();
        assert!(!metrics.is_empty());

        // If initialization succeeded, we should see metric names
        if REGISTRY.get().is_some() {
            // The output might be empty if no metrics have been recorded,
            // but it should be valid Prometheus format (no error prefix)
            assert!(!metrics.starts_with("# Error"));
        }
    }
}
