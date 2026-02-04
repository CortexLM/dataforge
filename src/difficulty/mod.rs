//! Difficulty system for dataforge benchmarks.
//!
//! This module provides difficulty levels, resource limits, and scoring calculations
//! for benchmark tasks.

use serde::{Deserialize, Serialize};

/// The difficulty level of a benchmark task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DifficultyLevel {
    Easy,
    Medium,
    Hard,
}

impl DifficultyLevel {
    /// Returns the score range (min, max) for this difficulty level.
    pub fn score_range(&self) -> (f64, f64) {
        match self {
            DifficultyLevel::Easy => (0.0, 0.33),
            DifficultyLevel::Medium => (0.34, 0.66),
            DifficultyLevel::Hard => (0.67, 1.0),
        }
    }

    /// Returns the expected time range in seconds (min, max) for this difficulty level.
    pub fn expected_time_range(&self) -> (u32, u32) {
        match self {
            DifficultyLevel::Easy => (30, 120),    // 30 seconds to 2 minutes
            DifficultyLevel::Medium => (120, 600), // 2 to 10 minutes
            DifficultyLevel::Hard => (600, 1800),  // 10 to 30 minutes
        }
    }

    /// Returns the expected number of command steps (min, max) for this difficulty level.
    pub fn command_steps_range(&self) -> (u32, u32) {
        match self {
            DifficultyLevel::Easy => (1, 3),
            DifficultyLevel::Medium => (3, 8),
            DifficultyLevel::Hard => (8, 20),
        }
    }

    /// Returns the target success rate for this difficulty level.
    /// This represents the expected pass rate when tested by human operators.
    pub fn target_success_rate(&self) -> f64 {
        match self {
            DifficultyLevel::Easy => 0.90,   // 90% expected success
            DifficultyLevel::Medium => 0.70, // 70% expected success
            DifficultyLevel::Hard => 0.40,   // 40% expected success
        }
    }

    /// Returns the base points awarded for completing a task at this difficulty.
    pub fn base_points(&self) -> f64 {
        match self {
            DifficultyLevel::Easy => 10.0,
            DifficultyLevel::Medium => 25.0,
            DifficultyLevel::Hard => 50.0,
        }
    }

    /// Returns the maximum time bonus that can be earned for this difficulty.
    pub fn time_bonus_max(&self) -> f64 {
        match self {
            DifficultyLevel::Easy => 5.0,
            DifficultyLevel::Medium => 15.0,
            DifficultyLevel::Hard => 30.0,
        }
    }

    /// Returns the resource limits for this difficulty level.
    pub fn resource_limits(&self) -> ResourceLimits {
        match self {
            DifficultyLevel::Easy => ResourceLimits {
                cpu_limit: 1.0,
                memory_limit: "256m".to_string(),
                storage_limit: "1g".to_string(),
                network: NetworkMode::Internal,
                pids_limit: 100,
            },
            DifficultyLevel::Medium => ResourceLimits {
                cpu_limit: 2.0,
                memory_limit: "512m".to_string(),
                storage_limit: "5g".to_string(),
                network: NetworkMode::Internal,
                pids_limit: 256,
            },
            DifficultyLevel::Hard => ResourceLimits {
                cpu_limit: 4.0,
                memory_limit: "1g".to_string(),
                storage_limit: "10g".to_string(),
                network: NetworkMode::External,
                pids_limit: 512,
            },
        }
    }
}

/// Resource limits applied to a benchmark task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU limit in cores.
    pub cpu_limit: f64,
    /// Memory limit (e.g., "512m", "1g").
    pub memory_limit: String,
    /// Storage limit (e.g., "5g", "10g").
    pub storage_limit: String,
    /// Network access mode.
    pub network: NetworkMode,
    /// Maximum number of processes allowed.
    pub pids_limit: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        DifficultyLevel::Medium.resource_limits()
    }
}

/// Network access mode for a benchmark task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No network access allowed.
    None,
    /// Internal network only (isolated).
    Internal,
    /// External network access permitted.
    External,
}

/// Result of calibrating a task's difficulty through testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    /// The task ID that was calibrated.
    pub task_id: String,
    /// Number of human testers who attempted the task.
    pub num_testers: u32,
    /// Success rate among testers (0.0 to 1.0).
    pub success_rate: f64,
    /// Mean time to complete the task in seconds.
    pub mean_time: f64,
    /// Standard deviation of completion time in seconds.
    pub std_time: f64,
    /// Mean number of hints used.
    pub mean_hints: f64,
    /// Calculated difficulty score (0.0 to 1.0).
    pub difficulty_score: f64,
    /// Suggested difficulty level based on calibration.
    pub suggested_level: DifficultyLevel,
}

impl CalibrationResult {
    /// Creates a new CalibrationResult from test data.
    pub fn new(
        task_id: impl Into<String>,
        num_testers: u32,
        success_rate: f64,
        mean_time: f64,
        std_time: f64,
        mean_hints: f64,
    ) -> Self {
        let difficulty_score = calculate_difficulty_score(mean_time, success_rate, mean_hints);
        let suggested_level = Self::score_to_level(difficulty_score);

        Self {
            task_id: task_id.into(),
            num_testers,
            success_rate,
            mean_time,
            std_time,
            mean_hints,
            difficulty_score,
            suggested_level,
        }
    }

    /// Converts a difficulty score to a difficulty level.
    fn score_to_level(score: f64) -> DifficultyLevel {
        if score < 0.34 {
            DifficultyLevel::Easy
        } else if score < 0.67 {
            DifficultyLevel::Medium
        } else {
            DifficultyLevel::Hard
        }
    }
}

/// Calculates a difficulty score based on task metrics.
///
/// The score is a weighted combination of:
/// - Time component (40%): Based on mean completion time
/// - Success rate component (40%): Inverse of success rate
/// - Hints component (20%): Based on average hints used
///
/// # Arguments
/// * `mean_time` - Mean completion time in seconds
/// * `success_rate` - Success rate from 0.0 to 1.0
/// * `mean_hints` - Average number of hints used
///
/// # Returns
/// A difficulty score between 0.0 and 1.0
pub fn calculate_difficulty_score(mean_time: f64, success_rate: f64, mean_hints: f64) -> f64 {
    // Time component: normalize to 0-1 range
    // Assume max time is 1800 seconds (30 minutes) for hardest tasks
    const MAX_TIME: f64 = 1800.0;
    let time_component = (mean_time / MAX_TIME).min(1.0);

    // Success rate component: invert (lower success = higher difficulty)
    // Clamp to valid range
    let clamped_success = success_rate.clamp(0.0, 1.0);
    let success_component = 1.0 - clamped_success;

    // Hints component: normalize assuming max of 5 hints
    const MAX_HINTS: f64 = 5.0;
    let hints_component = (mean_hints / MAX_HINTS).min(1.0);

    // Weighted combination
    const TIME_WEIGHT: f64 = 0.4;
    const SUCCESS_WEIGHT: f64 = 0.4;
    const HINTS_WEIGHT: f64 = 0.2;

    let score = TIME_WEIGHT * time_component
        + SUCCESS_WEIGHT * success_component
        + HINTS_WEIGHT * hints_component;

    // Clamp final score to 0-1 range
    score.clamp(0.0, 1.0)
}

/// Calculates the final score for a task attempt.
///
/// # Arguments
/// * `difficulty` - The difficulty level of the task
/// * `success` - Whether the task was completed successfully
/// * `partial_completion` - Percentage of task completed (0.0 to 1.0)
/// * `time_taken` - Actual time taken in seconds
/// * `expected_time` - Expected/reference time for the task in seconds
/// * `process_valid` - Whether the process/approach was valid
///
/// # Returns
/// The calculated score for the attempt
pub fn calculate_task_score(
    difficulty: DifficultyLevel,
    success: bool,
    partial_completion: f64,
    time_taken: f64,
    expected_time: f64,
    process_valid: bool,
) -> f64 {
    let base = difficulty.base_points();
    let time_bonus_max = difficulty.time_bonus_max();

    // If not successful, award partial points
    if !success {
        let partial_clamped = partial_completion.clamp(0.0, 1.0);
        // Award 50% of base points scaled by completion percentage
        return base * 0.5 * partial_clamped;
    }

    // Calculate time bonus: faster completion = more bonus
    // Bonus scales from 0 (at expected_time) to max (at 50% of expected time)
    let time_bonus = if time_taken < expected_time && expected_time > 0.0 {
        let time_ratio = time_taken / expected_time;
        // Linear scale from 0 at ratio=1.0 to max at ratio=0.5
        let bonus_ratio = ((1.0 - time_ratio) / 0.5).min(1.0);
        time_bonus_max * bonus_ratio
    } else {
        0.0
    };

    // Process validity bonus: 10% extra if process is valid
    let process_bonus = if process_valid { base * 0.1 } else { 0.0 };

    base + time_bonus + process_bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_score_ranges() {
        // Easy task: fast completion, high success, few hints
        let easy_score = calculate_difficulty_score(60.0, 0.95, 0.2);
        assert!(
            easy_score < 0.34,
            "Easy task should have score < 0.34, got {}",
            easy_score
        );

        // Hard task: slow completion, low success, many hints
        let hard_score = calculate_difficulty_score(1500.0, 0.30, 4.0);
        assert!(
            hard_score > 0.66,
            "Hard task should have score > 0.66, got {}",
            hard_score
        );
    }

    #[test]
    fn test_difficulty_score_clamping() {
        // Test with extreme values
        let score = calculate_difficulty_score(10000.0, -0.5, 100.0);
        assert!(
            (0.0..=1.0).contains(&score),
            "Score should be clamped to 0-1"
        );

        let score2 = calculate_difficulty_score(0.0, 1.5, 0.0);
        assert!(
            (0.0..=1.0).contains(&score2),
            "Score should be clamped to 0-1"
        );
    }

    #[test]
    fn test_task_score_success() {
        let score = calculate_task_score(DifficultyLevel::Medium, true, 1.0, 100.0, 200.0, true);
        // Base (25) + time bonus (up to 15) + process bonus (2.5)
        assert!(score > 25.0, "Successful task should score above base");
        assert!(score <= 42.5, "Score should not exceed maximum possible");
    }

    #[test]
    fn test_task_score_partial() {
        let score = calculate_task_score(DifficultyLevel::Easy, false, 0.5, 300.0, 120.0, false);
        // 50% of base (10) * 50% completion = 2.5
        assert!(
            (score - 2.5).abs() < 0.01,
            "Partial score should be 2.5, got {}",
            score
        );
    }

    #[test]
    fn test_calibration_result() {
        let result = CalibrationResult::new("test-task-1", 10, 0.80, 300.0, 60.0, 1.5);

        assert_eq!(result.task_id, "test-task-1");
        assert_eq!(result.num_testers, 10);
        assert!(result.difficulty_score >= 0.0 && result.difficulty_score <= 1.0);
    }

    #[test]
    fn test_resource_limits() {
        let easy_limits = DifficultyLevel::Easy.resource_limits();
        let hard_limits = DifficultyLevel::Hard.resource_limits();

        assert!(easy_limits.cpu_limit < hard_limits.cpu_limit);
        assert!(easy_limits.pids_limit < hard_limits.pids_limit);
    }

    #[test]
    fn test_difficulty_level_methods() {
        for level in [
            DifficultyLevel::Easy,
            DifficultyLevel::Medium,
            DifficultyLevel::Hard,
        ] {
            let (min_score, max_score) = level.score_range();
            assert!(min_score < max_score);

            let (min_time, max_time) = level.expected_time_range();
            assert!(min_time < max_time);

            let (min_steps, max_steps) = level.command_steps_range();
            assert!(min_steps < max_steps);

            assert!(level.target_success_rate() > 0.0);
            assert!(level.target_success_rate() <= 1.0);

            assert!(level.base_points() > 0.0);
            assert!(level.time_bonus_max() > 0.0);
        }
    }
}
