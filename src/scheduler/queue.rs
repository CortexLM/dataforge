//! Redis-based job queue with reliable dequeue.
//!
//! This module provides a distributed job queue backed by Redis that supports:
//!
//! - Atomic dequeue using BRPOPLPUSH
//! - Automatic retry with configurable attempts
//! - Dead letter queue for failed jobs
//! - Batch operations for efficiency
//!
//! # Queue Structure
//!
//! The queue uses three Redis lists:
//!
//! - `{queue_name}`: Main queue where jobs are enqueued
//! - `{queue_name}:processing`: Jobs being processed (for crash recovery)
//! - `{queue_name}:dead_letter`: Jobs that failed after max attempts
//!
//! # Reliability
//!
//! Jobs are atomically moved from the main queue to the processing queue when
//! dequeued. If a worker crashes, jobs in the processing queue can be recovered
//! and requeued.

use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use thiserror::Error;
use uuid::Uuid;

use super::job::{Job, JobResult};

/// Errors that can occur during queue operations.
#[derive(Debug, Error)]
pub enum QueueError {
    /// Failed to connect to Redis.
    #[error("Redis connection failed: {0}")]
    ConnectionFailed(String),

    /// Redis operation failed.
    #[error("Redis operation failed: {0}")]
    RedisError(#[from] redis::RedisError),

    /// Failed to serialize job data.
    #[error("Serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    /// Job not found in the queue.
    #[error("Job {0} not found")]
    JobNotFound(Uuid),

    /// Queue is empty (for non-blocking operations).
    #[error("Queue is empty")]
    QueueEmpty,

    /// Operation timed out.
    #[error("Operation timed out after {0:?}")]
    Timeout(Duration),
}

/// Redis-based job queue with reliable dequeue.
///
/// The queue uses BRPOPLPUSH for atomic dequeue operations, ensuring that
/// jobs are not lost if a worker crashes during processing.
pub struct JobQueue {
    /// Redis connection manager (handles reconnection automatically).
    redis: ConnectionManager,
    /// Name of the main queue.
    queue_name: String,
    /// Name of the processing queue.
    processing_queue: String,
    /// Name of the dead letter queue.
    dead_letter_queue: String,
    /// Key for storing job results.
    results_key: String,
}

impl JobQueue {
    /// Connects to Redis and creates a new job queue.
    ///
    /// # Arguments
    ///
    /// * `redis_url` - Redis connection URL (e.g., "redis://localhost:6379")
    /// * `queue_name` - Name of the queue (used as prefix for Redis keys)
    ///
    /// # Errors
    ///
    /// Returns `QueueError::ConnectionFailed` if the connection fails.
    pub async fn connect(redis_url: &str, queue_name: &str) -> Result<Self, QueueError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| QueueError::ConnectionFailed(e.to_string()))?;

        let redis = ConnectionManager::new(client)
            .await
            .map_err(|e| QueueError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            redis,
            queue_name: queue_name.to_string(),
            processing_queue: format!("{}:processing", queue_name),
            dead_letter_queue: format!("{}:dead_letter", queue_name),
            results_key: format!("{}:results", queue_name),
        })
    }

    /// Creates a JobQueue from an existing ConnectionManager.
    ///
    /// Useful when sharing a connection pool across multiple components.
    pub fn from_connection(redis: ConnectionManager, queue_name: &str) -> Self {
        Self {
            redis,
            queue_name: queue_name.to_string(),
            processing_queue: format!("{}:processing", queue_name),
            dead_letter_queue: format!("{}:dead_letter", queue_name),
            results_key: format!("{}:results", queue_name),
        }
    }

    /// Enqueues a new job.
    ///
    /// Jobs are added to the left of the queue (LPUSH) so they can be
    /// dequeued from the right (RPOP) in FIFO order.
    ///
    /// # Arguments
    ///
    /// * `job` - The job to enqueue
    pub async fn enqueue(&self, job: Job) -> Result<(), QueueError> {
        let serialized = serde_json::to_string(&job)?;
        let mut conn = self.redis.clone();
        conn.lpush::<_, _, ()>(&self.queue_name, serialized).await?;
        Ok(())
    }

    /// Enqueues multiple jobs in a single operation.
    ///
    /// This is more efficient than enqueueing jobs one at a time.
    ///
    /// # Arguments
    ///
    /// * `jobs` - Vector of jobs to enqueue
    pub async fn enqueue_batch(&self, jobs: Vec<Job>) -> Result<(), QueueError> {
        if jobs.is_empty() {
            return Ok(());
        }

        let serialized: Result<Vec<String>, _> = jobs.iter().map(serde_json::to_string).collect();
        let serialized = serialized?;

        let mut conn = self.redis.clone();

        // Use pipeline for batch efficiency
        let mut pipe = redis::pipe();
        for job_data in &serialized {
            pipe.lpush(&self.queue_name, job_data);
        }
        pipe.query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    /// Dequeues the next job, blocking until one is available or timeout.
    ///
    /// Uses BRPOPLPUSH to atomically move the job from the main queue to
    /// the processing queue. This ensures that if a worker crashes, the
    /// job can be recovered.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for a job
    ///
    /// # Returns
    ///
    /// - `Ok(Some(job))` if a job was dequeued
    /// - `Ok(None)` if the timeout expired with no jobs available
    pub async fn dequeue(&self, timeout: Duration) -> Result<Option<Job>, QueueError> {
        let mut conn = self.redis.clone();
        let timeout_secs = timeout.as_secs().max(1) as usize;

        // BRPOPLPUSH atomically pops from source and pushes to destination
        let result: Option<String> = redis::cmd("BRPOPLPUSH")
            .arg(&self.queue_name)
            .arg(&self.processing_queue)
            .arg(timeout_secs)
            .query_async(&mut conn)
            .await?;

        match result {
            Some(data) => {
                let job: Job = serde_json::from_str(&data)?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Marks a job as completed and removes it from the processing queue.
    ///
    /// The result is stored for later retrieval.
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the completed job
    /// * `result` - The job result
    pub async fn complete(&self, job_id: Uuid, result: JobResult) -> Result<(), QueueError> {
        let mut conn = self.redis.clone();

        // Store the result
        let result_key = format!("{}:{}", self.results_key, job_id);
        let result_data = serde_json::to_string(&result)?;

        // Set result with expiration (7 days)
        conn.set_ex::<_, _, ()>(&result_key, &result_data, 604800)
            .await?;

        // Remove from processing queue by finding and removing the job
        self.remove_job_from_processing(job_id).await?;

        Ok(())
    }

    /// Returns a job to the main queue for retry.
    ///
    /// The job's attempt counter should be incremented before calling this.
    ///
    /// # Arguments
    ///
    /// * `job` - The job to requeue
    pub async fn requeue(&self, job: Job) -> Result<(), QueueError> {
        let mut conn = self.redis.clone();

        // Remove from processing queue first
        self.remove_job_from_processing(job.id).await?;

        // Re-add to the main queue (at the front for immediate retry)
        let serialized = serde_json::to_string(&job)?;
        conn.rpush::<_, _, ()>(&self.queue_name, serialized).await?;

        Ok(())
    }

    /// Moves a job to the dead letter queue after exhausting retry attempts.
    ///
    /// # Arguments
    ///
    /// * `job` - The failed job
    /// * `error` - Description of the final error
    pub async fn dead_letter(&self, job: Job, error: &str) -> Result<(), QueueError> {
        let mut conn = self.redis.clone();

        // Remove from processing queue
        self.remove_job_from_processing(job.id).await?;

        // Create a dead letter entry with error information
        let dead_letter_entry = serde_json::json!({
            "job": job,
            "error": error,
            "moved_at": chrono::Utc::now().to_rfc3339(),
        });
        let serialized = serde_json::to_string(&dead_letter_entry)?;

        // Add to dead letter queue
        conn.lpush::<_, _, ()>(&self.dead_letter_queue, serialized)
            .await?;

        Ok(())
    }

    /// Returns the number of jobs in the main queue.
    pub async fn len(&self) -> Result<usize, QueueError> {
        let mut conn = self.redis.clone();
        let len: usize = conn.llen(&self.queue_name).await?;
        Ok(len)
    }

    /// Returns the number of jobs currently being processed.
    pub async fn processing_len(&self) -> Result<usize, QueueError> {
        let mut conn = self.redis.clone();
        let len: usize = conn.llen(&self.processing_queue).await?;
        Ok(len)
    }

    /// Returns the number of jobs in the dead letter queue.
    pub async fn dead_letter_len(&self) -> Result<usize, QueueError> {
        let mut conn = self.redis.clone();
        let len: usize = conn.llen(&self.dead_letter_queue).await?;
        Ok(len)
    }

    /// Returns whether the main queue is empty.
    pub async fn is_empty(&self) -> Result<bool, QueueError> {
        Ok(self.len().await? == 0)
    }

    /// Retrieves a job result by job ID.
    ///
    /// Results are stored for 7 days after completion.
    pub async fn get_result(&self, job_id: Uuid) -> Result<Option<JobResult>, QueueError> {
        let mut conn = self.redis.clone();
        let result_key = format!("{}:{}", self.results_key, job_id);

        let data: Option<String> = conn.get(&result_key).await?;

        match data {
            Some(s) => {
                let result: JobResult = serde_json::from_str(&s)?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// Recovers jobs stuck in the processing queue.
    ///
    /// This should be called on worker startup to recover jobs from
    /// workers that crashed. Jobs are moved back to the main queue.
    ///
    /// # Returns
    ///
    /// The number of jobs recovered.
    pub async fn recover_processing_jobs(&self) -> Result<usize, QueueError> {
        let mut conn = self.redis.clone();
        let mut recovered = 0;

        // Get all jobs in the processing queue
        let jobs: Vec<String> = conn.lrange(&self.processing_queue, 0, -1).await?;

        for job_data in jobs {
            // Parse the job
            if let Ok(mut job) = serde_json::from_str::<Job>(&job_data) {
                // Increment attempts since this is effectively a retry
                job.increment_attempts();

                if job.should_retry() {
                    // Move back to main queue
                    let serialized = serde_json::to_string(&job)?;

                    // Use a transaction to atomically remove from processing and add to main
                    let mut pipe = redis::pipe();
                    pipe.atomic()
                        .lrem(&self.processing_queue, 1, &job_data)
                        .rpush(&self.queue_name, &serialized);
                    pipe.query_async::<_, ()>(&mut conn).await?;

                    recovered += 1;
                } else {
                    // Exceeded max attempts, move to dead letter
                    self.dead_letter(job, "Recovered from processing queue after max attempts")
                        .await?;
                }
            }
        }

        Ok(recovered)
    }

    /// Clears all queues (main, processing, and dead letter).
    ///
    /// **Warning**: This permanently deletes all jobs. Use with caution.
    pub async fn clear(&self) -> Result<(), QueueError> {
        let mut conn = self.redis.clone();

        let mut pipe = redis::pipe();
        pipe.del(&self.queue_name)
            .del(&self.processing_queue)
            .del(&self.dead_letter_queue);
        pipe.query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    /// Returns queue statistics.
    pub async fn stats(&self) -> Result<QueueStats, QueueError> {
        let (queue_len, processing_len, dead_letter_len) =
            tokio::try_join!(self.len(), self.processing_len(), self.dead_letter_len())?;

        Ok(QueueStats {
            queue_name: self.queue_name.clone(),
            pending_jobs: queue_len,
            processing_jobs: processing_len,
            dead_letter_jobs: dead_letter_len,
        })
    }

    /// Peeks at jobs in the dead letter queue without removing them.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of jobs to return
    pub async fn peek_dead_letter(
        &self,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, QueueError> {
        let mut conn = self.redis.clone();
        let data: Vec<String> = conn
            .lrange(&self.dead_letter_queue, 0, limit as isize - 1)
            .await?;

        let entries: Result<Vec<serde_json::Value>, _> =
            data.iter().map(|s| serde_json::from_str(s)).collect();

        Ok(entries?)
    }

    /// Helper to remove a job from the processing queue by ID.
    async fn remove_job_from_processing(&self, job_id: Uuid) -> Result<(), QueueError> {
        let mut conn = self.redis.clone();

        // Get all jobs in processing queue
        let jobs: Vec<String> = conn.lrange(&self.processing_queue, 0, -1).await?;

        // Find and remove the job with matching ID
        for job_data in jobs {
            if let Ok(job) = serde_json::from_str::<Job>(&job_data) {
                if job.id == job_id {
                    conn.lrem::<_, _, ()>(&self.processing_queue, 1, &job_data)
                        .await?;
                    return Ok(());
                }
            }
        }

        // Job not found is not an error - it might have been already removed
        Ok(())
    }

    /// Returns the queue name.
    pub fn queue_name(&self) -> &str {
        &self.queue_name
    }
}

/// Statistics about queue state.
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Name of the queue.
    pub queue_name: String,
    /// Number of jobs waiting to be processed.
    pub pending_jobs: usize,
    /// Number of jobs currently being processed.
    pub processing_jobs: usize,
    /// Number of jobs in the dead letter queue.
    pub dead_letter_jobs: usize,
}

impl QueueStats {
    /// Returns the total number of jobs in all queues.
    pub fn total_jobs(&self) -> usize {
        self.pending_jobs + self.processing_jobs + self.dead_letter_jobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::TaskSpec;

    fn create_test_task(id: &str) -> TaskSpec {
        TaskSpec::new(id, "test_category", "easy", "Test instruction")
    }

    fn create_test_job(id: &str) -> Job {
        Job::new(create_test_task(id))
    }

    #[test]
    fn test_queue_error_display() {
        let err = QueueError::ConnectionFailed("timeout".to_string());
        assert!(err.to_string().contains("timeout"));

        let err = QueueError::JobNotFound(Uuid::new_v4());
        assert!(err.to_string().contains("not found"));

        let err = QueueError::QueueEmpty;
        assert!(err.to_string().contains("empty"));

        let err = QueueError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_queue_stats() {
        let stats = QueueStats {
            queue_name: "test".to_string(),
            pending_jobs: 10,
            processing_jobs: 5,
            dead_letter_jobs: 2,
        };

        assert_eq!(stats.total_jobs(), 17);
    }

    #[test]
    fn test_job_serialization_roundtrip() {
        let job = create_test_job("test-1");
        let serialized = serde_json::to_string(&job).expect("serialization should work");
        let deserialized: Job =
            serde_json::from_str(&serialized).expect("deserialization should work");

        assert_eq!(job.id, deserialized.id);
        assert_eq!(job.task_spec.id, deserialized.task_spec.id);
    }

    #[test]
    fn test_dead_letter_entry_structure() {
        let job = create_test_job("test-1");
        let error = "Test error message";

        let entry = serde_json::json!({
            "job": job,
            "error": error,
            "moved_at": chrono::Utc::now().to_rfc3339(),
        });

        // Verify the structure is serializable
        let serialized = serde_json::to_string(&entry).expect("entry should serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("should parse back");

        assert!(parsed.get("job").is_some());
        assert!(parsed.get("error").is_some());
        assert!(parsed.get("moved_at").is_some());
    }
}
