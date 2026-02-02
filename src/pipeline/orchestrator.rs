//! Pipeline orchestrator for coordinating data generation.
//!
//! This module provides the main `PipelineOrchestrator` that coordinates:
//! - Task selection and execution
//! - Docker container management
//! - Scaffold-based agent loops
//! - Trajectory collection
//! - Quality filtering
//! - Persistent storage

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::execution::DockerClient;
use crate::llm::router::{MultiModelRouter, RoutingStrategy};
use crate::llm::CostTracker;
use crate::storage::{Database, QualityScore};
use crate::trajectory::{TaskResult, Trajectory, TrajectoryStorage};

use super::config::PipelineConfig;
use super::runner::{RunError, RunResult, TaskSpec};

/// Errors that can occur during pipeline operations.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] super::config::ConfigError),

    /// Docker-related error.
    #[error("Docker error: {0}")]
    Docker(#[from] crate::error::DockerError),

    /// Database error.
    #[error("Database error: {0}")]
    Database(#[from] crate::storage::DatabaseError),

    /// Storage error.
    #[error("Storage error: {0}")]
    Storage(#[from] crate::trajectory::StorageError),

    /// Task execution error.
    #[error("Execution error: {0}")]
    Execution(#[from] RunError),

    /// Budget exceeded.
    #[error("Budget exceeded: daily={daily:.2}, monthly={monthly:.2}")]
    BudgetExceeded { daily: f64, monthly: f64 },

    /// No tasks to execute.
    #[error("No tasks to execute")]
    NoTasks,

    /// Initialization failed.
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),
}

/// Status of a task execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Task is pending execution.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed with an error.
    Failed,
    /// Task was filtered out due to quality.
    QualityFiltered,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionStatus::Pending => write!(f, "pending"),
            ExecutionStatus::Running => write!(f, "running"),
            ExecutionStatus::Completed => write!(f, "completed"),
            ExecutionStatus::Failed => write!(f, "failed"),
            ExecutionStatus::QualityFiltered => write!(f, "quality_filtered"),
        }
    }
}

/// Result of executing a single task.
#[derive(Debug)]
pub struct TaskExecution {
    /// ID of the executed task.
    pub task_id: String,
    /// ID of the collected trajectory (if any).
    pub trajectory_id: Option<Uuid>,
    /// Final execution status.
    pub status: ExecutionStatus,
    /// Duration of the execution.
    pub duration: Duration,
    /// Error message if the task failed.
    pub error: Option<String>,
}

impl TaskExecution {
    /// Creates a new pending task execution.
    fn pending(task_id: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            trajectory_id: None,
            status: ExecutionStatus::Pending,
            duration: Duration::ZERO,
            error: None,
        }
    }

    /// Marks the execution as completed.
    fn completed(mut self, trajectory_id: Uuid, duration: Duration) -> Self {
        self.trajectory_id = Some(trajectory_id);
        self.status = ExecutionStatus::Completed;
        self.duration = duration;
        self
    }

    /// Marks the execution as failed.
    fn failed(mut self, error: impl Into<String>, duration: Duration) -> Self {
        self.status = ExecutionStatus::Failed;
        self.error = Some(error.into());
        self.duration = duration;
        self
    }

    /// Marks the execution as quality filtered.
    fn quality_filtered(mut self, trajectory_id: Uuid, duration: Duration) -> Self {
        self.trajectory_id = Some(trajectory_id);
        self.status = ExecutionStatus::QualityFiltered;
        self.duration = duration;
        self
    }
}

/// Statistics about pipeline execution.
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    /// Total number of tasks executed.
    pub total_executed: u64,
    /// Number of successful executions.
    pub successful: u64,
    /// Number of failed executions.
    pub failed: u64,
    /// Number of quality-filtered trajectories.
    pub quality_filtered: u64,
    /// Average execution duration.
    pub average_duration: Duration,
    /// Total cost incurred.
    pub total_cost: f64,
}

impl PipelineStats {
    /// Creates new empty stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a successful execution.
    fn record_success(&mut self, duration: Duration) {
        self.total_executed += 1;
        self.successful += 1;
        self.update_average_duration(duration);
    }

    /// Records a failed execution.
    fn record_failure(&mut self, duration: Duration) {
        self.total_executed += 1;
        self.failed += 1;
        self.update_average_duration(duration);
    }

    /// Records a quality-filtered execution.
    fn record_quality_filtered(&mut self, duration: Duration) {
        self.total_executed += 1;
        self.quality_filtered += 1;
        self.update_average_duration(duration);
    }

    /// Updates the running average duration.
    fn update_average_duration(&mut self, duration: Duration) {
        if self.total_executed == 1 {
            self.average_duration = duration;
        } else {
            // Incremental average: avg = avg + (new - avg) / n
            let n = self.total_executed as f64;
            let old_avg = self.average_duration.as_secs_f64();
            let new_val = duration.as_secs_f64();
            let new_avg = old_avg + (new_val - old_avg) / n;
            self.average_duration = Duration::from_secs_f64(new_avg);
        }
    }

    /// Adds cost to the total.
    /// Adds cost to the total (used for external cost recording).
    #[allow(dead_code)]
    fn add_cost(&mut self, cost: f64) {
        self.total_cost += cost;
    }
}

/// Quality filter pipeline for trajectories.
pub struct QualityFilterPipeline {
    /// Minimum quality score to pass.
    min_score: f64,
    /// Whether deduplication is enabled.
    enable_dedup: bool,
    /// Similarity threshold for deduplication.
    similarity_threshold: f64,
}

impl QualityFilterPipeline {
    /// Creates a new quality filter pipeline.
    pub fn new(min_score: f64, enable_dedup: bool, similarity_threshold: f64) -> Self {
        Self {
            min_score,
            enable_dedup,
            similarity_threshold,
        }
    }

    /// Evaluates a trajectory and returns a quality score.
    ///
    /// Returns `Some(score)` if the trajectory passes quality checks,
    /// `None` if it should be filtered out.
    pub fn evaluate(&self, trajectory: &Trajectory) -> Option<QualityScore> {
        // Calculate component scores
        let correctness_score = self.evaluate_correctness(trajectory);
        let coherence_score = self.evaluate_coherence(trajectory);
        let completeness_score = self.evaluate_completeness(trajectory);

        // Calculate overall score (weighted average)
        let overall_score =
            0.4 * correctness_score + 0.3 * coherence_score + 0.3 * completeness_score;

        // Check if passes minimum threshold
        if overall_score < self.min_score {
            return None;
        }

        Some(
            QualityScore::new(trajectory.id, overall_score)
                .with_component_scores(correctness_score, coherence_score, completeness_score)
                .passed()
                .with_reviewer("quality_filter_pipeline"),
        )
    }

    /// Evaluates correctness of the trajectory.
    fn evaluate_correctness(&self, trajectory: &Trajectory) -> f64 {
        match &trajectory.final_result {
            TaskResult::Success { score } => *score,
            TaskResult::Failure { .. } => 0.2,
            TaskResult::Timeout => 0.1,
            TaskResult::Error { .. } => 0.0,
        }
    }

    /// Evaluates coherence of the trajectory.
    fn evaluate_coherence(&self, trajectory: &Trajectory) -> f64 {
        if trajectory.steps.is_empty() {
            return 0.0;
        }

        // Check that steps have meaningful actions
        let meaningful_steps = trajectory
            .steps
            .iter()
            .filter(|s| !s.action.tool_name.is_empty())
            .count();

        let meaningful_ratio = meaningful_steps as f64 / trajectory.steps.len() as f64;

        // Check that observations are present
        let has_observations = trajectory
            .steps
            .iter()
            .filter(|s| !s.observation.output.is_empty())
            .count();

        let observation_ratio = has_observations as f64 / trajectory.steps.len() as f64;

        // Average the two ratios
        (meaningful_ratio + observation_ratio) / 2.0
    }

    /// Evaluates completeness of the trajectory.
    fn evaluate_completeness(&self, trajectory: &Trajectory) -> f64 {
        // Check if trajectory has a terminal state
        let has_terminal = trajectory.steps.iter().any(|s| s.done);

        // Check if there's a final result
        let has_result = !matches!(trajectory.final_result, TaskResult::Error { .. });

        // Check if there's reasonable token usage
        let has_tokens = trajectory.token_usage.total_tokens > 0;

        // Calculate completeness
        let completeness = [has_terminal, has_result, has_tokens]
            .iter()
            .filter(|&&b| b)
            .count() as f64
            / 3.0;

        completeness
    }

    /// Checks if two trajectories are too similar (for deduplication).
    pub fn is_duplicate(&self, t1: &Trajectory, t2: &Trajectory) -> bool {
        if !self.enable_dedup {
            return false;
        }

        // Same task is a prerequisite for being a duplicate
        if t1.task_id != t2.task_id {
            return false;
        }

        // Compare action sequences
        let similarity = self.compute_similarity(t1, t2);
        similarity >= self.similarity_threshold
    }

    /// Computes similarity between two trajectories.
    fn compute_similarity(&self, t1: &Trajectory, t2: &Trajectory) -> f64 {
        if t1.steps.is_empty() || t2.steps.is_empty() {
            return 0.0;
        }

        // Compare action sequences using Jaccard similarity
        let actions1: Vec<&str> = t1
            .steps
            .iter()
            .map(|s| s.action.tool_name.as_str())
            .collect();
        let actions2: Vec<&str> = t2
            .steps
            .iter()
            .map(|s| s.action.tool_name.as_str())
            .collect();

        let set1: std::collections::HashSet<&str> = actions1.iter().copied().collect();
        let set2: std::collections::HashSet<&str> = actions2.iter().copied().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 {
            return 0.0;
        }

        intersection as f64 / union as f64
    }
}

/// Main pipeline orchestrator that coordinates all components.
pub struct PipelineOrchestrator {
    config: PipelineConfig,
    docker_client: Arc<DockerClient>,
    llm_router: Arc<MultiModelRouter>,
    database: Arc<Database>,
    quality_filter: QualityFilterPipeline,
    trajectory_storage: TrajectoryStorage,
    concurrency_limiter: Arc<Semaphore>,
    stats: Arc<tokio::sync::RwLock<PipelineStats>>,
    // Atomic counters for thread-safe stats
    total_executed: AtomicU64,
}

impl PipelineOrchestrator {
    /// Creates a new pipeline orchestrator with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Pipeline configuration
    ///
    /// # Errors
    ///
    /// Returns `PipelineError` if initialization fails.
    pub async fn new(config: PipelineConfig) -> Result<Self, PipelineError> {
        // Validate configuration
        config.validate()?;

        // Initialize Docker client
        let docker_client = DockerClient::new()?;

        // Initialize LLM router with cost tracking
        let cost_tracker = Arc::new(CostTracker::new(config.daily_budget, config.monthly_budget));
        let mut router =
            MultiModelRouter::with_cost_tracker(RoutingStrategy::CostOptimized, cost_tracker);

        // Set up fallback chain
        let mut fallback_chain = vec![config.default_model.clone()];
        fallback_chain.extend(config.fallback_models.clone());
        router.set_fallback_chain(fallback_chain);

        // Connect to database
        let database = Database::connect(&config.database_url).await?;

        // Run migrations
        database.run_migrations().await?;

        // Initialize quality filter
        let quality_filter = QualityFilterPipeline::new(
            config.min_quality_score,
            config.enable_deduplication,
            config.similarity_threshold,
        );

        // Initialize trajectory storage
        let trajectory_storage = TrajectoryStorage::new(&config.trajectory_path);

        // Create concurrency limiter
        let concurrency_limiter = Arc::new(Semaphore::new(config.max_concurrent_tasks));

        Ok(Self {
            config,
            docker_client: Arc::new(docker_client),
            llm_router: Arc::new(router),
            database: Arc::new(database),
            quality_filter,
            trajectory_storage,
            concurrency_limiter,
            stats: Arc::new(tokio::sync::RwLock::new(PipelineStats::new())),
            total_executed: AtomicU64::new(0),
        })
    }

    /// Runs a single task through the pipeline.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification to execute
    ///
    /// # Returns
    ///
    /// `TaskExecution` containing the execution results.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError` if execution fails.
    pub async fn run_task(&self, task: TaskSpec) -> Result<TaskExecution, PipelineError> {
        let start_time = Instant::now();
        let mut execution = TaskExecution::pending(&task.id);

        // Check budget before execution
        if self.llm_router.cost_tracker().is_over_budget() {
            let report = self.llm_router.cost_tracker().get_cost_report();
            return Err(PipelineError::BudgetExceeded {
                daily: report.daily_spent,
                monthly: report.monthly_spent,
            });
        }

        // Acquire concurrency permit
        let _permit = self.concurrency_limiter.acquire().await.map_err(|e| {
            PipelineError::InitializationFailed(format!("Failed to acquire permit: {}", e))
        })?;

        execution.status = ExecutionStatus::Running;

        // Execute the task
        let result = self.execute_task(&task).await;
        let duration = start_time.elapsed();

        match result {
            Ok(run_result) => {
                // Evaluate quality
                if let Some(quality_score) = self.quality_filter.evaluate(&run_result.trajectory) {
                    // Trajectory passed quality filter
                    // Save to database
                    self.database
                        .save_trajectory(&run_result.trajectory)
                        .await?;
                    self.database.save_quality_score(&quality_score).await?;

                    // Save to file storage
                    self.trajectory_storage.save(&run_result.trajectory).await?;

                    // Update stats
                    {
                        let mut stats = self.stats.write().await;
                        stats.record_success(duration);
                    }

                    execution = execution.completed(run_result.trajectory.id, duration);
                } else {
                    // Trajectory filtered out
                    // Still save the trajectory for analysis, but mark as filtered
                    self.database
                        .save_trajectory(&run_result.trajectory)
                        .await?;

                    let filtered_score = QualityScore::new(run_result.trajectory.id, 0.0)
                        .with_reviewer("quality_filter_pipeline");
                    self.database.save_quality_score(&filtered_score).await?;

                    // Update stats
                    {
                        let mut stats = self.stats.write().await;
                        stats.record_quality_filtered(duration);
                    }

                    execution = execution.quality_filtered(run_result.trajectory.id, duration);
                }
            }
            Err(e) => {
                // Execution failed
                {
                    let mut stats = self.stats.write().await;
                    stats.record_failure(duration);
                }

                execution = execution.failed(e.to_string(), duration);
            }
        }

        // Update total count
        self.total_executed.fetch_add(1, Ordering::SeqCst);

        Ok(execution)
    }

    /// Runs multiple tasks concurrently.
    ///
    /// # Arguments
    ///
    /// * `tasks` - Vector of task specifications to execute
    ///
    /// # Returns
    ///
    /// Vector of `TaskExecution` results for each task.
    pub async fn run_batch(&self, tasks: Vec<TaskSpec>) -> Vec<TaskExecution> {
        if tasks.is_empty() {
            return Vec::new();
        }

        // Create futures for all tasks
        let futures: Vec<_> = tasks
            .into_iter()
            .map(|task| {
                let task_id = task.id.clone();
                async move {
                    match self.run_task(task).await {
                        Ok(execution) => execution,
                        Err(e) => {
                            TaskExecution::pending(&task_id).failed(e.to_string(), Duration::ZERO)
                        }
                    }
                }
            })
            .collect();

        // Execute all tasks concurrently
        futures::future::join_all(futures).await
    }

    /// Gets the current pipeline statistics.
    pub async fn stats(&self) -> PipelineStats {
        let stats = self.stats.read().await;
        let mut result = stats.clone();

        // Add cost from the cost tracker
        let report = self.llm_router.cost_tracker().get_cost_report();
        result.total_cost = report.monthly_spent;

        result
    }

    /// Gets the current configuration.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Gets a reference to the Docker client.
    pub fn docker_client(&self) -> &Arc<DockerClient> {
        &self.docker_client
    }

    /// Gets a reference to the LLM router.
    pub fn llm_router(&self) -> &Arc<MultiModelRouter> {
        &self.llm_router
    }

    /// Gets a reference to the database.
    pub fn database(&self) -> &Arc<Database> {
        &self.database
    }

    /// Checks if the budget has been exceeded.
    pub fn is_over_budget(&self) -> bool {
        self.llm_router.cost_tracker().is_over_budget()
    }

    /// Executes a single task (internal implementation).
    async fn execute_task(&self, task: &TaskSpec) -> Result<RunResult, RunError> {
        // Create scaffold for the task
        let scaffold = self.create_scaffold(task)?;

        // Create and run the task runner
        let mut runner = super::runner::TaskRunner::new(Arc::clone(&self.docker_client), scaffold);

        runner.run(task, &self.config.default_model).await
    }

    /// Creates a scaffold for a task.
    fn create_scaffold(
        &self,
        task: &TaskSpec,
    ) -> Result<Box<dyn crate::scaffold::Scaffold>, RunError> {
        use crate::scaffold::{SweAgentConfig, SweAgentScaffold};

        let config = SweAgentConfig::default()
            .with_model(&self.config.default_model)
            .with_timeout(task.timeout)
            .with_max_steps(task.max_steps as u32);

        Ok(Box::new(SweAgentScaffold::new(config)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::{
        AgentAction, EnvironmentState, Observation, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;

    fn create_test_trajectory(success: bool) -> Trajectory {
        Trajectory {
            id: Uuid::new_v4(),
            task_id: "test-task".to_string(),
            model: "gpt-4".to_string(),
            scaffold_type: "test".to_string(),
            steps: vec![TrajectoryStep {
                step_number: 0,
                state: EnvironmentState::default(),
                action: AgentAction {
                    tool_name: "bash".to_string(),
                    tool_args: serde_json::json!({"cmd": "ls"}),
                    raw_llm_output: "test".to_string(),
                    thinking: None,
                },
                observation: Observation {
                    success: true,
                    output: "file.txt".to_string(),
                    error: None,
                    state_changes: Vec::new(),
                },
                reward: 0.5,
                done: true,
                timestamp: Utc::now(),
            }],
            final_result: if success {
                TaskResult::Success { score: 1.0 }
            } else {
                TaskResult::Failure {
                    reason: "test".to_string(),
                }
            },
            total_reward: 0.5,
            created_at: Utc::now(),
            duration_seconds: 60,
            token_usage: TokenUsage::new(100, 50),
        }
    }

    #[test]
    fn test_execution_status_display() {
        assert_eq!(format!("{}", ExecutionStatus::Pending), "pending");
        assert_eq!(format!("{}", ExecutionStatus::Running), "running");
        assert_eq!(format!("{}", ExecutionStatus::Completed), "completed");
        assert_eq!(format!("{}", ExecutionStatus::Failed), "failed");
        assert_eq!(
            format!("{}", ExecutionStatus::QualityFiltered),
            "quality_filtered"
        );
    }

    #[test]
    fn test_task_execution_lifecycle() {
        let execution = TaskExecution::pending("task-1");
        assert_eq!(execution.status, ExecutionStatus::Pending);
        assert!(execution.trajectory_id.is_none());
        assert!(execution.error.is_none());

        let trajectory_id = Uuid::new_v4();
        let duration = Duration::from_secs(60);

        let completed = execution.completed(trajectory_id, duration);
        assert_eq!(completed.status, ExecutionStatus::Completed);
        assert_eq!(completed.trajectory_id, Some(trajectory_id));
        assert_eq!(completed.duration, duration);
    }

    #[test]
    fn test_task_execution_failed() {
        let execution = TaskExecution::pending("task-2");
        let duration = Duration::from_secs(30);

        let failed = execution.failed("Test error", duration);
        assert_eq!(failed.status, ExecutionStatus::Failed);
        assert_eq!(failed.error, Some("Test error".to_string()));
        assert_eq!(failed.duration, duration);
    }

    #[test]
    fn test_task_execution_quality_filtered() {
        let execution = TaskExecution::pending("task-3");
        let trajectory_id = Uuid::new_v4();
        let duration = Duration::from_secs(45);

        let filtered = execution.quality_filtered(trajectory_id, duration);
        assert_eq!(filtered.status, ExecutionStatus::QualityFiltered);
        assert_eq!(filtered.trajectory_id, Some(trajectory_id));
    }

    #[test]
    fn test_pipeline_stats() {
        let mut stats = PipelineStats::new();
        assert_eq!(stats.total_executed, 0);
        assert_eq!(stats.successful, 0);
        assert_eq!(stats.failed, 0);

        stats.record_success(Duration::from_secs(60));
        assert_eq!(stats.total_executed, 1);
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.average_duration.as_secs(), 60);

        stats.record_failure(Duration::from_secs(30));
        assert_eq!(stats.total_executed, 2);
        assert_eq!(stats.failed, 1);
        // Average should be (60 + 30) / 2 = 45
        assert_eq!(stats.average_duration.as_secs(), 45);

        stats.record_quality_filtered(Duration::from_secs(90));
        assert_eq!(stats.total_executed, 3);
        assert_eq!(stats.quality_filtered, 1);
    }

    #[test]
    fn test_quality_filter_evaluate_success() {
        let filter = QualityFilterPipeline::new(0.5, false, 0.85);
        let trajectory = create_test_trajectory(true);

        let result = filter.evaluate(&trajectory);
        assert!(result.is_some());

        let score = result.unwrap();
        assert!(score.passed_filter);
        assert!(score.overall_score >= 0.5);
    }

    #[test]
    fn test_quality_filter_evaluate_failure() {
        let filter = QualityFilterPipeline::new(0.9, false, 0.85);
        let trajectory = create_test_trajectory(false);

        // With high threshold and failed task, should be filtered
        let result = filter.evaluate(&trajectory);
        assert!(result.is_none());
    }

    #[test]
    fn test_quality_filter_deduplication() {
        let filter = QualityFilterPipeline::new(0.5, true, 0.85);

        let t1 = create_test_trajectory(true);
        let mut t2 = create_test_trajectory(true);
        t2.task_id = t1.task_id.clone();

        // Same task, same actions - should be duplicate
        assert!(filter.is_duplicate(&t1, &t2));

        // Different task - not duplicate
        t2.task_id = "different-task".to_string();
        assert!(!filter.is_duplicate(&t1, &t2));
    }

    #[test]
    fn test_quality_filter_deduplication_disabled() {
        let filter = QualityFilterPipeline::new(0.5, false, 0.85);

        let t1 = create_test_trajectory(true);
        let t2 = create_test_trajectory(true);

        // Deduplication disabled - never duplicate
        assert!(!filter.is_duplicate(&t1, &t2));
    }

    #[test]
    fn test_pipeline_error_display() {
        let err = PipelineError::BudgetExceeded {
            daily: 100.0,
            monthly: 500.0,
        };
        assert!(err.to_string().contains("Budget exceeded"));
        assert!(err.to_string().contains("100.00"));
        assert!(err.to_string().contains("500.00"));

        let err = PipelineError::NoTasks;
        assert!(err.to_string().contains("No tasks"));

        let err = PipelineError::InitializationFailed("test".to_string());
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_quality_filter_coherence_empty_trajectory() {
        let filter = QualityFilterPipeline::new(0.5, false, 0.85);

        let mut trajectory = create_test_trajectory(true);
        trajectory.steps.clear();

        // Empty trajectory should score 0 for coherence
        let coherence = filter.evaluate_coherence(&trajectory);
        assert!((coherence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_filter_completeness() {
        let filter = QualityFilterPipeline::new(0.5, false, 0.85);

        let trajectory = create_test_trajectory(true);
        let completeness = filter.evaluate_completeness(&trajectory);

        // Should be complete (has terminal state, result, and tokens)
        assert!((completeness - 1.0).abs() < f64::EPSILON);
    }
}
