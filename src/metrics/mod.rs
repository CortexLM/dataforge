//! Metrics module for Prometheus-based monitoring.
//!
//! This module provides comprehensive metrics collection and export for
//! dataforge operations, including task execution, LLM usage, and quality scores.
//!
//! # Example
//!
//! ```ignore
//! use dataforge::metrics::{init_metrics, export_metrics, MetricsCollector};
//!
//! // Initialize metrics on startup
//! init_metrics().expect("Failed to initialize metrics");
//!
//! // Create a collector for recording metrics
//! let collector = MetricsCollector::new();
//!
//! // Record task execution
//! collector.record_task("success", "medium", "gpt-4", 120.5);
//!
//! // Export metrics for Prometheus scraping
//! let metrics_text = export_metrics();
//! ```

pub mod collectors;
pub mod prometheus;

// Re-export key types for convenient access
pub use collectors::{MetricsCollector, TokenUsage};
pub use prometheus::{export_metrics, init_metrics, metrics_handler};

// Re-export metric constants for direct access when needed
pub use prometheus::{
    ACTIVE_WORKERS, JOBS_IN_PROGRESS, LLM_COST_CENTS, LLM_LATENCY, LLM_REQUESTS_TOTAL,
    LLM_TOKENS_TOTAL, QUALITY_FILTERED, QUALITY_SCORE, QUEUE_DEPTH, REGISTRY, TASKS_TOTAL,
    TASK_DURATION,
};
