//! Pipeline orchestration for synthetic data generation.
//!
//! This module provides the main pipeline infrastructure for running tasks,
//! collecting trajectories, filtering quality, and storing results.
//!
//! # Architecture
//!
//! The pipeline consists of several components:
//!
//! - **Orchestrator**: The main coordinator that manages the entire pipeline
//! - **Runner**: Executes individual tasks in Docker containers
//! - **Config**: Configuration for all pipeline components
//!
//! # Pipeline Flow
//!
//! 1. **Task Selection**: Tasks are selected from a registry or provided directly
//! 2. **Container Creation**: A Docker container is created for task execution
//! 3. **Scaffold Initialization**: The agent scaffold (e.g., SWE-Agent) is initialized
//! 4. **Agent Loop Execution**: The agent executes steps until completion or limit
//! 5. **Trajectory Collection**: Each step is recorded as part of a trajectory
//! 6. **Quality Filtering**: Trajectories are evaluated for quality
//! 7. **Storage**: Passing trajectories are stored in the database and filesystem
//!
//! # Example
//!
//! ```rust,ignore
//! use dataforge::pipeline::{PipelineOrchestrator, PipelineConfig, TaskSpec};
//! use std::time::Duration;
//!
//! // Create configuration
//! let config = PipelineConfig::new()
//!     .with_max_concurrent_tasks(4)
//!     .with_task_timeout(Duration::from_secs(1800))
//!     .with_database_url("postgres://localhost/dataforge");
//!
//! // Create orchestrator
//! let orchestrator = PipelineOrchestrator::new(config).await?;
//!
//! // Create a task
//! let task = TaskSpec::new("task-001", "Create a Python script that prints 'Hello, World!'")
//!     .with_category("code_generation")
//!     .with_difficulty("easy")
//!     .with_verification_script("python main.py | grep -q 'Hello, World!'");
//!
//! // Run the task
//! let result = orchestrator.run_task(task).await?;
//!
//! println!("Task {} completed with status: {}", result.task_id, result.status);
//!
//! // Get statistics
//! let stats = orchestrator.stats().await;
//! println!("Total executed: {}, Successful: {}", stats.total_executed, stats.successful);
//! ```
//!
//! # Batch Execution
//!
//! The orchestrator supports concurrent execution of multiple tasks:
//!
//! ```rust,ignore
//! let tasks = vec![
//!     TaskSpec::new("task-1", "Task 1 instruction"),
//!     TaskSpec::new("task-2", "Task 2 instruction"),
//!     TaskSpec::new("task-3", "Task 3 instruction"),
//! ];
//!
//! let results = orchestrator.run_batch(tasks).await;
//!
//! for result in results {
//!     match result.status {
//!         ExecutionStatus::Completed => println!("{}: Success!", result.task_id),
//!         ExecutionStatus::Failed => println!("{}: Failed - {:?}", result.task_id, result.error),
//!         ExecutionStatus::QualityFiltered => println!("{}: Filtered out", result.task_id),
//!         _ => {}
//!     }
//! }
//! ```
//!
//! # Configuration
//!
//! The pipeline can be configured via the `PipelineConfig` struct or environment variables:
//!
//! ```rust,ignore
//! // Via builder pattern
//! let config = PipelineConfig::new()
//!     .with_max_concurrent_tasks(8)
//!     .with_min_quality_score(0.7)
//!     .with_daily_budget(100.0);
//!
//! // Via environment variables
//! let config = PipelineConfig::from_env()?;
//! ```
//!
//! # Quality Filtering
//!
//! Trajectories are evaluated based on:
//!
//! - **Correctness**: Did the task complete successfully?
//! - **Coherence**: Are the actions meaningful and related?
//! - **Completeness**: Is the trajectory fully formed?
//!
//! Trajectories that don't meet the minimum quality threshold are filtered out
//! but still stored for analysis.

pub mod config;
pub mod orchestrator;
pub mod runner;

// Re-export main types for convenience
pub use config::{ConfigError, PipelineConfig};
pub use orchestrator::{
    ExecutionStatus, PipelineError, PipelineOrchestrator, PipelineStats, QualityFilterPipeline,
    TaskExecution,
};
pub use runner::{RunError, RunResult, TaskRunner, TaskSpec};
