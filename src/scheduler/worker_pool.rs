//! Worker pool for processing jobs from Redis queue.
//!
//! This module provides a pool of workers that process jobs from a shared
//! Redis queue. Each worker runs as an independent async task and pulls
//! jobs from the queue.
//!
//! # Features
//!
//! - Configurable number of workers
//! - Graceful shutdown with broadcast channel
//! - Automatic job retry on failure
//! - Dead letter queue for failed jobs
//! - Pool statistics tracking

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::pipeline::PipelineOrchestrator;

use super::job::{Job, JobResult, JobStatus};
use super::queue::{JobQueue, QueueError};

/// Errors that can occur in the worker pool.
#[derive(Debug, Error)]
pub enum PoolError {
    /// Failed to connect to the job queue.
    #[error("Queue connection failed: {0}")]
    QueueConnection(#[from] QueueError),

    /// Worker initialization failed.
    #[error("Worker initialization failed: {0}")]
    WorkerInitFailed(String),

    /// Pool is already running.
    #[error("Pool is already running")]
    AlreadyRunning,

    /// Pool is not running.
    #[error("Pool is not running")]
    NotRunning,

    /// Shutdown timed out.
    #[error("Shutdown timed out after {0:?}")]
    ShutdownTimeout(Duration),

    /// Pipeline error during job processing.
    #[error("Pipeline error: {0}")]
    PipelineError(String),
}

/// Configuration for the worker pool.
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    /// Number of worker tasks to spawn.
    pub num_workers: usize,
    /// Redis connection URL.
    pub redis_url: String,
    /// Name of the job queue.
    pub queue_name: String,
    /// How often to poll for new jobs when the queue is empty.
    pub poll_interval: Duration,
    /// Maximum time allowed for processing a single job.
    pub job_timeout: Duration,
    /// Timeout for graceful shutdown.
    pub shutdown_timeout: Duration,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            num_workers: 4,
            redis_url: "redis://localhost:6379".to_string(),
            queue_name: "tasks".to_string(),
            poll_interval: Duration::from_secs(1),
            job_timeout: Duration::from_secs(1800), // 30 minutes
            shutdown_timeout: Duration::from_secs(60),
        }
    }
}

impl WorkerPoolConfig {
    /// Creates a new configuration with the specified number of workers.
    pub fn new(num_workers: usize) -> Self {
        Self {
            num_workers,
            ..Default::default()
        }
    }

    /// Sets the Redis URL.
    pub fn with_redis_url(mut self, url: impl Into<String>) -> Self {
        self.redis_url = url.into();
        self
    }

    /// Sets the queue name.
    pub fn with_queue_name(mut self, name: impl Into<String>) -> Self {
        self.queue_name = name.into();
        self
    }

    /// Sets the poll interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Sets the job timeout.
    pub fn with_job_timeout(mut self, timeout: Duration) -> Self {
        self.job_timeout = timeout;
        self
    }

    /// Sets the shutdown timeout.
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }
}

/// Statistics about the worker pool.
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total number of workers in the pool.
    pub num_workers: usize,
    /// Number of workers currently processing jobs.
    pub active_workers: usize,
    /// Total number of jobs completed successfully.
    pub jobs_completed: u64,
    /// Total number of jobs that failed.
    pub jobs_failed: u64,
    /// Average job processing duration.
    pub average_job_duration: Duration,
}

impl PoolStats {
    /// Returns the total number of jobs processed (completed + failed).
    pub fn total_processed(&self) -> u64 {
        self.jobs_completed + self.jobs_failed
    }

    /// Returns the success rate as a percentage.
    pub fn success_rate(&self) -> f64 {
        let total = self.total_processed();
        if total == 0 {
            return 0.0;
        }
        (self.jobs_completed as f64 / total as f64) * 100.0
    }
}

/// Shared state for tracking pool statistics.
struct SharedPoolStats {
    jobs_completed: AtomicU64,
    jobs_failed: AtomicU64,
    total_duration_ms: AtomicU64,
    active_workers: AtomicU64,
}

impl SharedPoolStats {
    fn new() -> Self {
        Self {
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            total_duration_ms: AtomicU64::new(0),
            active_workers: AtomicU64::new(0),
        }
    }

    fn record_completion(&self, duration: Duration) {
        self.jobs_completed.fetch_add(1, Ordering::SeqCst);
        self.total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
    }

    fn record_failure(&self, duration: Duration) {
        self.jobs_failed.fetch_add(1, Ordering::SeqCst);
        self.total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
    }

    fn increment_active(&self) {
        self.active_workers.fetch_add(1, Ordering::SeqCst);
    }

    fn decrement_active(&self) {
        self.active_workers.fetch_sub(1, Ordering::SeqCst);
    }

    fn to_pool_stats(&self, num_workers: usize) -> PoolStats {
        let completed = self.jobs_completed.load(Ordering::SeqCst);
        let failed = self.jobs_failed.load(Ordering::SeqCst);
        let total_duration_ms = self.total_duration_ms.load(Ordering::SeqCst);
        let active = self.active_workers.load(Ordering::SeqCst);

        let total_jobs = completed + failed;
        let average_duration = if total_jobs > 0 {
            Duration::from_millis(total_duration_ms / total_jobs)
        } else {
            Duration::ZERO
        };

        PoolStats {
            num_workers,
            active_workers: active as usize,
            jobs_completed: completed,
            jobs_failed: failed,
            average_job_duration: average_duration,
        }
    }
}

/// Worker pool that manages multiple workers processing jobs from a queue.
pub struct WorkerPool {
    config: WorkerPoolConfig,
    queue: Arc<JobQueue>,
    orchestrator: Arc<PipelineOrchestrator>,
    shutdown_tx: broadcast::Sender<()>,
    worker_handles: Vec<JoinHandle<()>>,
    stats: Arc<SharedPoolStats>,
    is_running: AtomicBool,
}

impl WorkerPool {
    /// Creates a new worker pool.
    ///
    /// # Arguments
    ///
    /// * `config` - Pool configuration
    /// * `orchestrator` - Pipeline orchestrator for executing tasks
    ///
    /// # Errors
    ///
    /// Returns `PoolError` if queue connection fails.
    pub async fn new(
        config: WorkerPoolConfig,
        orchestrator: Arc<PipelineOrchestrator>,
    ) -> Result<Self, PoolError> {
        let queue = JobQueue::connect(&config.redis_url, &config.queue_name).await?;

        // Create broadcast channel for shutdown signal
        // Buffer size of 1 is sufficient since we only send once
        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            config,
            queue: Arc::new(queue),
            orchestrator,
            shutdown_tx,
            worker_handles: Vec::new(),
            stats: Arc::new(SharedPoolStats::new()),
            is_running: AtomicBool::new(false),
        })
    }

    /// Creates a worker pool with an existing queue connection.
    ///
    /// Useful when the queue is shared with other components.
    pub fn with_queue(
        config: WorkerPoolConfig,
        queue: Arc<JobQueue>,
        orchestrator: Arc<PipelineOrchestrator>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config,
            queue,
            orchestrator,
            shutdown_tx,
            worker_handles: Vec::new(),
            stats: Arc::new(SharedPoolStats::new()),
            is_running: AtomicBool::new(false),
        }
    }

    /// Starts all workers in the pool.
    ///
    /// Workers will begin polling the queue for jobs immediately.
    ///
    /// # Errors
    ///
    /// Returns `PoolError::AlreadyRunning` if the pool is already running.
    pub async fn start(&mut self) -> Result<(), PoolError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Err(PoolError::AlreadyRunning);
        }

        // Recover any jobs stuck in the processing queue from previous runs
        match self.queue.recover_processing_jobs().await {
            Ok(recovered) => {
                if recovered > 0 {
                    info!(
                        recovered = recovered,
                        "Recovered jobs from processing queue"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to recover processing jobs");
            }
        }

        // Spawn workers
        for i in 0..self.config.num_workers {
            let worker = Worker::new(
                format!("worker-{}", i),
                Arc::clone(&self.queue),
                Arc::clone(&self.orchestrator),
                self.shutdown_tx.subscribe(),
                self.config.poll_interval,
                self.config.job_timeout,
                Arc::clone(&self.stats),
            );

            let handle = tokio::spawn(async move {
                worker.run().await;
            });

            self.worker_handles.push(handle);
        }

        self.is_running.store(true, Ordering::SeqCst);
        info!(num_workers = self.config.num_workers, "Worker pool started");

        Ok(())
    }

    /// Gracefully shuts down all workers.
    ///
    /// Sends a shutdown signal to all workers and waits for them to finish
    /// their current jobs.
    ///
    /// # Errors
    ///
    /// Returns `PoolError::ShutdownTimeout` if workers don't stop within
    /// the configured timeout.
    pub async fn shutdown(&mut self) -> Result<(), PoolError> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(PoolError::NotRunning);
        }

        info!("Initiating worker pool shutdown");

        // Send shutdown signal to all workers
        // Ignore send error - workers may have already stopped
        let _ = self.shutdown_tx.send(());

        // Wait for all workers to finish with timeout
        let shutdown_future = async {
            for handle in self.worker_handles.drain(..) {
                if let Err(e) = handle.await {
                    error!(error = %e, "Worker task panicked during shutdown");
                }
            }
        };

        match tokio::time::timeout(self.config.shutdown_timeout, shutdown_future).await {
            Ok(()) => {
                self.is_running.store(false, Ordering::SeqCst);
                info!("Worker pool shutdown complete");
                Ok(())
            }
            Err(_) => {
                self.is_running.store(false, Ordering::SeqCst);
                Err(PoolError::ShutdownTimeout(self.config.shutdown_timeout))
            }
        }
    }

    /// Returns current pool statistics.
    pub fn stats(&self) -> PoolStats {
        self.stats.to_pool_stats(self.config.num_workers)
    }

    /// Scales the pool to a new number of workers.
    ///
    /// If scaling up, new workers are added. If scaling down, excess workers
    /// are gracefully stopped.
    ///
    /// # Arguments
    ///
    /// * `num_workers` - The target number of workers
    ///
    /// # Note
    ///
    /// Scaling down currently stops and restarts all workers. A more
    /// sophisticated implementation would selectively stop workers.
    pub async fn scale(&mut self, num_workers: usize) -> Result<(), PoolError> {
        if !self.is_running.load(Ordering::SeqCst) {
            // Not running, just update config
            self.config.num_workers = num_workers;
            return Ok(());
        }

        if num_workers == self.config.num_workers {
            return Ok(());
        }

        info!(
            current = self.config.num_workers,
            target = num_workers,
            "Scaling worker pool"
        );

        // For simplicity, shutdown and restart with new count
        // A more sophisticated implementation would add/remove individual workers
        self.shutdown().await?;
        self.config.num_workers = num_workers;
        self.start().await?;

        Ok(())
    }

    /// Returns whether the pool is currently running.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Returns the number of workers in the pool.
    pub fn num_workers(&self) -> usize {
        self.config.num_workers
    }

    /// Returns a reference to the job queue.
    pub fn queue(&self) -> &Arc<JobQueue> {
        &self.queue
    }
}

/// A single worker that processes jobs from the queue.
pub struct Worker {
    /// Unique identifier for this worker.
    id: String,
    /// Reference to the job queue.
    queue: Arc<JobQueue>,
    /// Reference to the pipeline orchestrator.
    orchestrator: Arc<PipelineOrchestrator>,
    /// Receiver for shutdown signal.
    shutdown_rx: broadcast::Receiver<()>,
    /// Interval between poll attempts when queue is empty.
    poll_interval: Duration,
    /// Maximum time for processing a single job.
    job_timeout: Duration,
    /// Shared statistics.
    stats: Arc<SharedPoolStats>,
}

impl Worker {
    /// Creates a new worker.
    fn new(
        id: String,
        queue: Arc<JobQueue>,
        orchestrator: Arc<PipelineOrchestrator>,
        shutdown_rx: broadcast::Receiver<()>,
        poll_interval: Duration,
        job_timeout: Duration,
        stats: Arc<SharedPoolStats>,
    ) -> Self {
        Self {
            id,
            queue,
            orchestrator,
            shutdown_rx,
            poll_interval,
            job_timeout,
            stats,
        }
    }

    /// Main worker loop.
    ///
    /// Continuously polls for jobs and processes them until a shutdown
    /// signal is received.
    async fn run(mut self) {
        info!(worker_id = %self.id, "Worker started");

        loop {
            // Check for shutdown signal (non-blocking)
            match self.shutdown_rx.try_recv() {
                Ok(()) | Err(broadcast::error::TryRecvError::Closed) => {
                    info!(worker_id = %self.id, "Worker received shutdown signal");
                    break;
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    // We missed some signals, but since it's shutdown, just check again
                    continue;
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    // No shutdown signal, continue processing
                }
            }

            // Try to dequeue a job
            match self.queue.dequeue(self.poll_interval).await {
                Ok(Some(job)) => {
                    self.process_job(job).await;
                }
                Ok(None) => {
                    // No job available, the dequeue already waited poll_interval
                    debug!(worker_id = %self.id, "No jobs available");
                }
                Err(e) => {
                    error!(worker_id = %self.id, error = %e, "Failed to dequeue job");
                    // Wait before retrying on error
                    tokio::time::sleep(self.poll_interval).await;
                }
            }
        }

        info!(worker_id = %self.id, "Worker stopped");
    }

    /// Processes a single job.
    async fn process_job(&self, mut job: Job) {
        let job_id = job.id;
        let start_time = Instant::now();

        info!(
            worker_id = %self.id,
            job_id = %job_id,
            task_id = %job.task_spec.id,
            attempt = job.attempts + 1,
            "Processing job"
        );

        self.stats.increment_active();
        job.increment_attempts();

        // Execute the job with timeout
        let result = self.execute_job_with_timeout(&job).await;
        let duration = start_time.elapsed();

        self.stats.decrement_active();

        match result {
            Ok(job_result) => {
                // Job completed (success or expected failure)
                if let Err(e) = self.queue.complete(job_id, job_result.clone()).await {
                    error!(
                        worker_id = %self.id,
                        job_id = %job_id,
                        error = %e,
                        "Failed to mark job complete"
                    );
                }

                if job_result.is_success() {
                    self.stats.record_completion(duration);
                    info!(
                        worker_id = %self.id,
                        job_id = %job_id,
                        duration_ms = duration.as_millis(),
                        "Job completed successfully"
                    );
                } else {
                    self.stats.record_failure(duration);
                    warn!(
                        worker_id = %self.id,
                        job_id = %job_id,
                        status = %job_result.status,
                        error = ?job_result.error,
                        "Job completed with failure status"
                    );
                }
            }
            Err(e) => {
                // Job execution failed unexpectedly
                self.stats.record_failure(duration);

                if job.should_retry() {
                    warn!(
                        worker_id = %self.id,
                        job_id = %job_id,
                        error = %e,
                        remaining_attempts = job.remaining_attempts(),
                        "Job failed, requeueing for retry"
                    );

                    if let Err(requeue_err) = self.queue.requeue(job).await {
                        error!(
                            worker_id = %self.id,
                            job_id = %job_id,
                            error = %requeue_err,
                            "Failed to requeue job"
                        );
                    }
                } else {
                    error!(
                        worker_id = %self.id,
                        job_id = %job_id,
                        error = %e,
                        "Job failed, moving to dead letter queue"
                    );

                    if let Err(dlq_err) = self.queue.dead_letter(job, &e.to_string()).await {
                        error!(
                            worker_id = %self.id,
                            job_id = %job_id,
                            error = %dlq_err,
                            "Failed to move job to dead letter queue"
                        );
                    }
                }
            }
        }
    }

    /// Executes a job with the configured timeout.
    async fn execute_job_with_timeout(&self, job: &Job) -> Result<JobResult, PoolError> {
        let job_id = job.id;
        let worker_id = self.id.clone();
        let start_time = Instant::now();

        // Convert task spec to pipeline format
        let pipeline_task = job.task_spec.to_pipeline_task();

        // Execute with timeout
        let execution_future = self.orchestrator.run_task(pipeline_task);

        match tokio::time::timeout(self.job_timeout, execution_future).await {
            Ok(Ok(execution)) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;

                let result = match execution.status {
                    crate::pipeline::ExecutionStatus::Completed => JobResult::success(
                        job_id,
                        worker_id,
                        execution.trajectory_id.unwrap_or_else(Uuid::new_v4),
                        duration_ms,
                    ),
                    crate::pipeline::ExecutionStatus::Failed => JobResult::failure(
                        job_id,
                        worker_id,
                        execution
                            .error
                            .unwrap_or_else(|| "Unknown error".to_string()),
                        duration_ms,
                    ),
                    crate::pipeline::ExecutionStatus::QualityFiltered => {
                        // Quality filtered is still considered a successful execution
                        let mut result = JobResult::success(
                            job_id,
                            &self.id,
                            execution.trajectory_id.unwrap_or_else(Uuid::new_v4),
                            duration_ms,
                        );
                        result.status = JobStatus::Completed;
                        result
                    }
                    _ => JobResult::failure(
                        job_id,
                        worker_id,
                        format!("Unexpected status: {:?}", execution.status),
                        duration_ms,
                    ),
                };

                Ok(result)
            }
            Ok(Err(e)) => {
                // Pipeline error
                Err(PoolError::PipelineError(e.to_string()))
            }
            Err(_) => {
                // Timeout
                let duration_ms = start_time.elapsed().as_millis() as u64;
                Ok(JobResult::timeout(job_id, worker_id, duration_ms))
            }
        }
    }

    /// Returns the worker's ID.
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_pool_config_default() {
        let config = WorkerPoolConfig::default();

        assert_eq!(config.num_workers, 4);
        assert_eq!(config.redis_url, "redis://localhost:6379");
        assert_eq!(config.queue_name, "tasks");
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.job_timeout, Duration::from_secs(1800));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_worker_pool_config_builder() {
        let config = WorkerPoolConfig::new(8)
            .with_redis_url("redis://custom:6380")
            .with_queue_name("my_queue")
            .with_poll_interval(Duration::from_secs(5))
            .with_job_timeout(Duration::from_secs(3600))
            .with_shutdown_timeout(Duration::from_secs(120));

        assert_eq!(config.num_workers, 8);
        assert_eq!(config.redis_url, "redis://custom:6380");
        assert_eq!(config.queue_name, "my_queue");
        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert_eq!(config.job_timeout, Duration::from_secs(3600));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_pool_stats_default() {
        let stats = PoolStats::default();

        assert_eq!(stats.num_workers, 0);
        assert_eq!(stats.active_workers, 0);
        assert_eq!(stats.jobs_completed, 0);
        assert_eq!(stats.jobs_failed, 0);
        assert_eq!(stats.average_job_duration, Duration::ZERO);
        assert_eq!(stats.total_processed(), 0);
        assert!((stats.success_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pool_stats_calculations() {
        let stats = PoolStats {
            num_workers: 4,
            active_workers: 2,
            jobs_completed: 80,
            jobs_failed: 20,
            average_job_duration: Duration::from_secs(60),
        };

        assert_eq!(stats.total_processed(), 100);
        assert!((stats.success_rate() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_shared_pool_stats() {
        let stats = SharedPoolStats::new();

        stats.record_completion(Duration::from_secs(10));
        stats.record_completion(Duration::from_secs(20));
        stats.record_failure(Duration::from_secs(5));

        let pool_stats = stats.to_pool_stats(4);

        assert_eq!(pool_stats.num_workers, 4);
        assert_eq!(pool_stats.jobs_completed, 2);
        assert_eq!(pool_stats.jobs_failed, 1);
        // Average: (10000 + 20000 + 5000) / 3 = 11666 ms
        assert!(pool_stats.average_job_duration.as_millis() > 11000);
        assert!(pool_stats.average_job_duration.as_millis() < 12000);
    }

    #[test]
    fn test_shared_pool_stats_active_workers() {
        let stats = SharedPoolStats::new();

        assert_eq!(stats.active_workers.load(Ordering::SeqCst), 0);

        stats.increment_active();
        stats.increment_active();
        assert_eq!(stats.active_workers.load(Ordering::SeqCst), 2);

        stats.decrement_active();
        assert_eq!(stats.active_workers.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_pool_error_display() {
        let err = PoolError::AlreadyRunning;
        assert!(err.to_string().contains("already running"));

        let err = PoolError::NotRunning;
        assert!(err.to_string().contains("not running"));

        let err = PoolError::ShutdownTimeout(Duration::from_secs(60));
        assert!(err.to_string().contains("60"));

        let err = PoolError::PipelineError("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }
}
