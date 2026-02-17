//! Background progress monitor for the SWE mining pipeline.
//!
//! Periodically logs pipeline statistics (candidates filtered, tasks extracted,
//! quality scored, tasks accepted) so operators can track long-running mining
//! runs without parsing individual log lines.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

/// Snapshot of pipeline progress counters at a point in time.
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    /// Number of candidates that passed through the filter stage.
    pub filtered: usize,
    /// Number of tasks that passed extraction + test generation.
    pub extracted: usize,
    /// Number of tasks that were quality-scored.
    pub scored: usize,
    /// Number of tasks accepted into the final output.
    pub accepted: usize,
    /// Wall-clock elapsed time since the monitor started.
    pub elapsed: Duration,
}

/// Shared atomic counters for pipeline progress tracking.
///
/// Cloned into pipeline worker tasks and incremented via `fetch_add`.
/// The background monitor reads these periodically to emit progress logs.
#[derive(Debug, Clone)]
pub struct ProgressCounters {
    /// Candidates evaluated by the filter stage.
    pub filtered: Arc<AtomicUsize>,
    /// Tasks that completed extraction + test generation.
    pub extracted: Arc<AtomicUsize>,
    /// Tasks that were quality-scored.
    pub scored: Arc<AtomicUsize>,
    /// Tasks accepted into the final output.
    pub accepted: Arc<AtomicUsize>,
}

impl Default for ProgressCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressCounters {
    /// Create a new set of zeroed progress counters.
    pub fn new() -> Self {
        Self {
            filtered: Arc::new(AtomicUsize::new(0)),
            extracted: Arc::new(AtomicUsize::new(0)),
            scored: Arc::new(AtomicUsize::new(0)),
            accepted: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Take a snapshot of the current counter values.
    pub fn snapshot(&self, start: Instant) -> ProgressSnapshot {
        ProgressSnapshot {
            filtered: self.filtered.load(Ordering::Relaxed),
            extracted: self.extracted.load(Ordering::Relaxed),
            scored: self.scored.load(Ordering::Relaxed),
            accepted: self.accepted.load(Ordering::Relaxed),
            elapsed: start.elapsed(),
        }
    }
}

/// A background task that periodically logs pipeline progress.
///
/// Spawns a tokio task that wakes every `interval` and logs a summary
/// of the pipeline counters. Call [`ProgressMonitor::stop`] to cancel.
pub struct ProgressMonitor {
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ProgressMonitor {
    /// Start a background progress monitor that logs every `interval`.
    ///
    /// # Arguments
    ///
    /// * `counters` - Shared atomic counters incremented by pipeline workers
    /// * `max_tasks` - Target number of tasks (used for progress percentage)
    /// * `interval` - How often to emit progress logs
    pub fn start(counters: ProgressCounters, max_tasks: usize, interval: Duration) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag = stop_flag.clone();
        let start = Instant::now();

        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            tick.tick().await; // skip the immediate first tick

            loop {
                tick.tick().await;
                if flag.load(Ordering::Relaxed) {
                    break;
                }

                let snap = counters.snapshot(start);
                let pct = if max_tasks > 0 {
                    (snap.accepted as f64 / max_tasks as f64 * 100.0).min(100.0)
                } else {
                    0.0
                };

                tracing::info!(
                    filtered = snap.filtered,
                    extracted = snap.extracted,
                    scored = snap.scored,
                    accepted = snap.accepted,
                    max_tasks = max_tasks,
                    progress_pct = format!("{:.1}%", pct),
                    elapsed_secs = snap.elapsed.as_secs(),
                    "Pipeline progress"
                );
            }
        });

        Self {
            stop_flag,
            handle: Some(handle),
        }
    }

    /// Signal the background monitor to stop and wait for it to finish.
    pub async fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for ProgressMonitor {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_counters_default() {
        let counters = ProgressCounters::new();
        let snap = counters.snapshot(Instant::now());
        assert_eq!(snap.filtered, 0);
        assert_eq!(snap.extracted, 0);
        assert_eq!(snap.scored, 0);
        assert_eq!(snap.accepted, 0);
    }

    #[test]
    fn test_progress_counters_increment() {
        let counters = ProgressCounters::new();
        counters.filtered.fetch_add(10, Ordering::Relaxed);
        counters.extracted.fetch_add(5, Ordering::Relaxed);
        counters.scored.fetch_add(3, Ordering::Relaxed);
        counters.accepted.fetch_add(1, Ordering::Relaxed);

        let snap = counters.snapshot(Instant::now());
        assert_eq!(snap.filtered, 10);
        assert_eq!(snap.extracted, 5);
        assert_eq!(snap.scored, 3);
        assert_eq!(snap.accepted, 1);
    }

    #[test]
    fn test_progress_counters_clone_shares_state() {
        let counters = ProgressCounters::new();
        let clone = counters.clone();

        counters.accepted.fetch_add(1, Ordering::Relaxed);
        assert_eq!(clone.accepted.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_progress_monitor_start_stop() {
        let counters = ProgressCounters::new();
        counters.accepted.fetch_add(3, Ordering::Relaxed);

        let monitor = ProgressMonitor::start(counters, 10, Duration::from_millis(50));

        tokio::time::sleep(Duration::from_millis(120)).await;
        monitor.stop().await;
    }
}
