//! Job definitions for the scheduler.
//!
//! This module defines the core job types used in the scheduling system:
//!
//! - `Job`: A unit of work to be executed by workers
//! - `TaskSpec`: Specification of the task to execute
//! - `JobResult`: Result of job execution
//! - `JobStatus`: Status of a completed job

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default maximum number of retry attempts for a job.
const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// Default priority for jobs (0 is normal priority).
const DEFAULT_PRIORITY: i32 = 0;

/// Specification for a task to be executed.
///
/// This is a serializable task description that can be stored in Redis
/// and executed by workers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSpec {
    /// Unique identifier for the task.
    pub id: String,
    /// Category of the task (e.g., "file_manipulation", "code_generation").
    pub category: String,
    /// Difficulty level (e.g., "easy", "medium", "hard").
    pub difficulty: String,
    /// The instruction/problem statement for the agent.
    pub instruction: String,
    /// Optional verification script to check task completion.
    #[serde(default)]
    pub verification_script: Option<String>,
    /// Optional expected output for validation.
    #[serde(default)]
    pub expected_output: Option<String>,
    /// Timeout in seconds for this task.
    pub timeout_seconds: u64,
    /// Maximum steps allowed for this task.
    pub max_steps: u32,
    /// Optional hint for which model to use.
    #[serde(default)]
    pub model_hint: Option<String>,
}

impl TaskSpec {
    /// Creates a new task specification with default timeout and max steps.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the task
    /// * `category` - Category of the task
    /// * `difficulty` - Difficulty level
    /// * `instruction` - The instruction for the agent
    pub fn new(
        id: impl Into<String>,
        category: impl Into<String>,
        difficulty: impl Into<String>,
        instruction: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category: category.into(),
            difficulty: difficulty.into(),
            instruction: instruction.into(),
            verification_script: None,
            expected_output: None,
            timeout_seconds: 1800, // 30 minutes default
            max_steps: 50,
            model_hint: None,
        }
    }

    /// Sets the verification script.
    pub fn with_verification_script(mut self, script: impl Into<String>) -> Self {
        self.verification_script = Some(script.into());
        self
    }

    /// Sets the expected output.
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output = Some(output.into());
        self
    }

    /// Sets the timeout in seconds.
    pub fn with_timeout_seconds(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    /// Sets the maximum steps.
    pub fn with_max_steps(mut self, steps: u32) -> Self {
        self.max_steps = steps;
        self
    }

    /// Sets the model hint.
    pub fn with_model_hint(mut self, model: impl Into<String>) -> Self {
        self.model_hint = Some(model.into());
        self
    }

    /// Converts this task spec to the pipeline's TaskSpec format.
    pub fn to_pipeline_task(&self) -> crate::pipeline::TaskSpec {
        let mut task = crate::pipeline::TaskSpec::new(&self.id, &self.instruction)
            .with_category(&self.category)
            .with_difficulty(&self.difficulty)
            .with_timeout(std::time::Duration::from_secs(self.timeout_seconds))
            .with_max_steps(self.max_steps as usize);

        if let Some(ref script) = self.verification_script {
            task = task.with_verification_script(script);
        }

        if let Some(ref output) = self.expected_output {
            task = task.with_expected_output(output);
        }

        task
    }
}

/// A job representing a unit of work to be executed.
///
/// Jobs are stored in Redis and processed by workers. They include
/// retry logic and metadata for tracking execution history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique identifier for this job.
    pub id: Uuid,
    /// The task specification to execute.
    pub task_spec: TaskSpec,
    /// Priority of the job (higher values = higher priority).
    pub priority: i32,
    /// When this job was created.
    pub created_at: DateTime<Utc>,
    /// Number of times this job has been attempted.
    pub attempts: u32,
    /// Maximum number of attempts before moving to dead letter queue.
    pub max_attempts: u32,
    /// Optional metadata for tracking and debugging.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

impl Job {
    /// Creates a new job with default settings.
    ///
    /// The job will have:
    /// - A new UUID
    /// - Default priority (0)
    /// - Current timestamp as creation time
    /// - Zero attempts
    /// - Default max attempts (3)
    pub fn new(task: TaskSpec) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_spec: task,
            priority: DEFAULT_PRIORITY,
            created_at: Utc::now(),
            attempts: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            metadata: None,
        }
    }

    /// Creates a new job with a specified priority.
    ///
    /// Higher priority values mean the job should be processed sooner.
    /// Negative priorities are valid and indicate lower-than-normal priority.
    pub fn with_priority(task: TaskSpec, priority: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_spec: task,
            priority,
            created_at: Utc::now(),
            attempts: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            metadata: None,
        }
    }

    /// Sets the maximum number of retry attempts.
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Sets optional metadata for the job.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Increments the attempt counter.
    ///
    /// This should be called before each execution attempt.
    pub fn increment_attempts(&mut self) {
        self.attempts += 1;
    }

    /// Returns whether the job should be retried after a failure.
    ///
    /// A job should be retried if it has not exceeded the maximum
    /// number of attempts.
    pub fn should_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }

    /// Returns the number of remaining retry attempts.
    pub fn remaining_attempts(&self) -> u32 {
        self.max_attempts.saturating_sub(self.attempts)
    }

    /// Returns how long ago the job was created.
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }
}

/// Status of a completed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job completed successfully.
    Completed,
    /// Job failed after exhausting all retry attempts.
    Failed,
    /// Job timed out during execution.
    Timeout,
    /// Job was cancelled before completion.
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Timeout => write!(f, "timeout"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Result of a job execution.
///
/// This is returned by workers after processing a job and contains
/// information about the execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// ID of the job that was executed.
    pub job_id: Uuid,
    /// Final status of the job.
    pub status: JobStatus,
    /// ID of the trajectory if one was generated.
    pub trajectory_id: Option<Uuid>,
    /// Error message if the job failed.
    pub error: Option<String>,
    /// When the job was completed.
    pub completed_at: DateTime<Utc>,
    /// ID of the worker that processed this job.
    pub worker_id: String,
    /// Duration of the execution in milliseconds.
    pub duration_ms: u64,
}

impl JobResult {
    /// Creates a new successful job result.
    pub fn success(
        job_id: Uuid,
        worker_id: impl Into<String>,
        trajectory_id: Uuid,
        duration_ms: u64,
    ) -> Self {
        Self {
            job_id,
            status: JobStatus::Completed,
            trajectory_id: Some(trajectory_id),
            error: None,
            completed_at: Utc::now(),
            worker_id: worker_id.into(),
            duration_ms,
        }
    }

    /// Creates a new failed job result.
    pub fn failure(
        job_id: Uuid,
        worker_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            job_id,
            status: JobStatus::Failed,
            trajectory_id: None,
            error: Some(error.into()),
            completed_at: Utc::now(),
            worker_id: worker_id.into(),
            duration_ms,
        }
    }

    /// Creates a new timeout job result.
    pub fn timeout(job_id: Uuid, worker_id: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            job_id,
            status: JobStatus::Timeout,
            trajectory_id: None,
            error: Some("Job execution timed out".to_string()),
            completed_at: Utc::now(),
            worker_id: worker_id.into(),
            duration_ms,
        }
    }

    /// Creates a new cancelled job result.
    pub fn cancelled(job_id: Uuid, worker_id: impl Into<String>) -> Self {
        Self {
            job_id,
            status: JobStatus::Cancelled,
            trajectory_id: None,
            error: Some("Job was cancelled".to_string()),
            completed_at: Utc::now(),
            worker_id: worker_id.into(),
            duration_ms: 0,
        }
    }

    /// Returns whether the job completed successfully.
    pub fn is_success(&self) -> bool {
        self.status == JobStatus::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_spec_new() {
        let spec = TaskSpec::new("task-1", "code_gen", "medium", "Write a function");

        assert_eq!(spec.id, "task-1");
        assert_eq!(spec.category, "code_gen");
        assert_eq!(spec.difficulty, "medium");
        assert_eq!(spec.instruction, "Write a function");
        assert_eq!(spec.timeout_seconds, 1800);
        assert_eq!(spec.max_steps, 50);
        assert!(spec.verification_script.is_none());
        assert!(spec.expected_output.is_none());
        assert!(spec.model_hint.is_none());
    }

    #[test]
    fn test_task_spec_builder() {
        let spec = TaskSpec::new("task-2", "file_ops", "hard", "Create files")
            .with_verification_script("test -f output.txt")
            .with_expected_output("success")
            .with_timeout_seconds(3600)
            .with_max_steps(100)
            .with_model_hint("gpt-4");

        assert_eq!(spec.id, "task-2");
        assert_eq!(spec.timeout_seconds, 3600);
        assert_eq!(spec.max_steps, 100);
        assert_eq!(
            spec.verification_script,
            Some("test -f output.txt".to_string())
        );
        assert_eq!(spec.expected_output, Some("success".to_string()));
        assert_eq!(spec.model_hint, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_job_new() {
        let task = TaskSpec::new("task-1", "code_gen", "easy", "Do something");
        let job = Job::new(task);

        assert!(!job.id.is_nil());
        assert_eq!(job.priority, 0);
        assert_eq!(job.attempts, 0);
        assert_eq!(job.max_attempts, 3);
        assert!(job.should_retry());
    }

    #[test]
    fn test_job_with_priority() {
        let task = TaskSpec::new("task-1", "code_gen", "easy", "High priority task");
        let job = Job::with_priority(task, 10);

        assert_eq!(job.priority, 10);
    }

    #[test]
    fn test_job_increment_attempts() {
        let task = TaskSpec::new("task-1", "code_gen", "easy", "Test");
        let mut job = Job::new(task).with_max_attempts(2);

        assert!(job.should_retry());
        assert_eq!(job.remaining_attempts(), 2);

        job.increment_attempts();
        assert!(job.should_retry());
        assert_eq!(job.remaining_attempts(), 1);

        job.increment_attempts();
        assert!(!job.should_retry());
        assert_eq!(job.remaining_attempts(), 0);
    }

    #[test]
    fn test_job_serialization() {
        let task = TaskSpec::new("task-1", "code_gen", "easy", "Test serialization");
        let job = Job::new(task);

        let json = serde_json::to_string(&job).expect("serialization should work");
        let parsed: Job = serde_json::from_str(&json).expect("deserialization should work");

        assert_eq!(parsed.id, job.id);
        assert_eq!(parsed.task_spec.id, job.task_spec.id);
        assert_eq!(parsed.priority, job.priority);
    }

    #[test]
    fn test_job_status_display() {
        assert_eq!(format!("{}", JobStatus::Completed), "completed");
        assert_eq!(format!("{}", JobStatus::Failed), "failed");
        assert_eq!(format!("{}", JobStatus::Timeout), "timeout");
        assert_eq!(format!("{}", JobStatus::Cancelled), "cancelled");
    }

    #[test]
    fn test_job_result_success() {
        let job_id = Uuid::new_v4();
        let trajectory_id = Uuid::new_v4();
        let result = JobResult::success(job_id, "worker-1", trajectory_id, 5000);

        assert_eq!(result.job_id, job_id);
        assert_eq!(result.status, JobStatus::Completed);
        assert_eq!(result.trajectory_id, Some(trajectory_id));
        assert!(result.error.is_none());
        assert!(result.is_success());
    }

    #[test]
    fn test_job_result_failure() {
        let job_id = Uuid::new_v4();
        let result = JobResult::failure(job_id, "worker-2", "Task failed", 3000);

        assert_eq!(result.job_id, job_id);
        assert_eq!(result.status, JobStatus::Failed);
        assert!(result.trajectory_id.is_none());
        assert_eq!(result.error, Some("Task failed".to_string()));
        assert!(!result.is_success());
    }

    #[test]
    fn test_job_result_timeout() {
        let job_id = Uuid::new_v4();
        let result = JobResult::timeout(job_id, "worker-3", 30000);

        assert_eq!(result.status, JobStatus::Timeout);
        assert!(result.error.is_some());
        assert!(!result.is_success());
    }

    #[test]
    fn test_job_result_cancelled() {
        let job_id = Uuid::new_v4();
        let result = JobResult::cancelled(job_id, "worker-4");

        assert_eq!(result.status, JobStatus::Cancelled);
        assert_eq!(result.duration_ms, 0);
        assert!(!result.is_success());
    }

    #[test]
    fn test_task_spec_equality() {
        let spec1 = TaskSpec::new("task-1", "code_gen", "easy", "Instruction");
        let spec2 = TaskSpec::new("task-1", "code_gen", "easy", "Instruction");
        let spec3 = TaskSpec::new("task-2", "code_gen", "easy", "Instruction");

        assert_eq!(spec1, spec2);
        assert_ne!(spec1, spec3);
    }

    #[test]
    fn test_job_with_metadata() {
        let task = TaskSpec::new("task-1", "code_gen", "easy", "Test");
        let metadata = serde_json::json!({
            "source": "api",
            "user_id": "user-123"
        });
        let job = Job::new(task).with_metadata(metadata.clone());

        assert_eq!(job.metadata, Some(metadata));
    }
}
