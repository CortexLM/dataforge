//! Horizontal scaling system with worker pools and Redis queues.
//!
//! This module provides infrastructure for distributed task execution:
//!
//! - **JobQueue**: Redis-based job queue with reliable dequeue using BRPOPLPUSH
//! - **WorkerPool**: Pool of workers that process jobs concurrently
//! - **Job**: Job definitions with retry logic and dead letter support
//!
//! # Architecture
//!
//! ```text
//!                      ┌──────────────┐
//!                      │   Producer   │
//!                      │  (API/CLI)   │
//!                      └──────┬───────┘
//!                             │
//!                      ┌──────▼───────┐
//!                      │    Redis     │
//!                      │    Queue     │
//!                      └──────┬───────┘
//!                             │
//!         ┌───────────────────┼───────────────────┐
//!         │                   │                   │
//!         ▼                   ▼                   ▼
//!    ┌─────────┐         ┌─────────┐         ┌─────────┐
//!    │ Worker 1│         │ Worker 2│         │ Worker N│
//!    └─────────┘         └─────────┘         └─────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use swe_forge::scheduler::{WorkerPool, WorkerPoolConfig, JobQueue, Job, TaskSpec};
//! use swe_forge::pipeline::PipelineOrchestrator;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! // Connect to Redis queue
//! let queue = JobQueue::connect("redis://localhost:6379", "tasks").await?;
//!
//! // Enqueue some jobs
//! let task = TaskSpec::new("task-001", "code_gen", "easy", "Write a hello world program");
//! let job = Job::new(task);
//! queue.enqueue(job).await?;
//!
//! // Create worker pool
//! let orchestrator = Arc::new(PipelineOrchestrator::new(config).await?);
//! let pool_config = WorkerPoolConfig {
//!     num_workers: 4,
//!     redis_url: "redis://localhost:6379".to_string(),
//!     queue_name: "tasks".to_string(),
//!     poll_interval: Duration::from_secs(1),
//!     job_timeout: Duration::from_secs(1800),
//! };
//!
//! let mut pool = WorkerPool::new(pool_config, orchestrator).await?;
//! pool.start().await?;
//!
//! // Graceful shutdown
//! pool.shutdown().await?;
//! ```
//!
//! # Reliability Features
//!
//! - **Atomic dequeue**: Uses BRPOPLPUSH to atomically move jobs to processing queue
//! - **Crash recovery**: Jobs in processing queue are automatically requeued on worker restart
//! - **Dead letter queue**: Failed jobs after max attempts are moved to DLQ for analysis
//! - **Graceful shutdown**: Workers finish current jobs before stopping

pub mod job;
pub mod queue;
pub mod worker_pool;

// Re-export main types for convenience
pub use job::{Job, JobResult, JobStatus, TaskSpec};
pub use queue::{JobQueue, QueueError};
pub use worker_pool::{PoolError, PoolStats, Worker, WorkerPool, WorkerPoolConfig};
