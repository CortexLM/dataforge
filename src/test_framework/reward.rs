// Reward calculation for task verification
// Calculates scores based on test results and provides detailed breakdowns

use serde::{Deserialize, Serialize};

/// Score breakdown for a verification category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScore {
    /// Score achieved in this category
    pub score: f64,
    /// Maximum possible score
    pub max_score: f64,
    /// Number of tests passed
    pub tests_passed: u32,
    /// Total number of tests
    pub tests_total: u32,
}

impl CategoryScore {
    /// Create a new category score
    pub fn new(score: f64, max_score: f64, tests_passed: u32, tests_total: u32) -> Self {
        Self {
            score,
            max_score,
            tests_passed,
            tests_total,
        }
    }

    /// Calculate the percentage of tests passed
    pub fn pass_rate(&self) -> f64 {
        if self.tests_total == 0 {
            0.0
        } else {
            (self.tests_passed as f64 / self.tests_total as f64) * 100.0
        }
    }

    /// Calculate the score percentage
    pub fn score_percentage(&self) -> f64 {
        if self.max_score == 0.0 {
            0.0
        } else {
            (self.score / self.max_score) * 100.0
        }
    }
}

impl Default for CategoryScore {
    fn default() -> Self {
        Self {
            score: 0.0,
            max_score: 0.0,
            tests_passed: 0,
            tests_total: 0,
        }
    }
}

/// Breakdown of reward scores by category
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RewardBreakdown {
    /// Score for output verification
    pub output_verification: CategoryScore,
    /// Score for state verification
    pub state_verification: CategoryScore,
    /// Score for process verification
    pub process_verification: CategoryScore,
}

impl RewardBreakdown {
    /// Calculate total score across all categories
    pub fn total_score(&self) -> f64 {
        self.output_verification.score
            + self.state_verification.score
            + self.process_verification.score
    }

    /// Calculate maximum possible score
    pub fn max_score(&self) -> f64 {
        self.output_verification.max_score
            + self.state_verification.max_score
            + self.process_verification.max_score
    }

    /// Calculate total tests passed
    pub fn total_tests_passed(&self) -> u32 {
        self.output_verification.tests_passed
            + self.state_verification.tests_passed
            + self.process_verification.tests_passed
    }

    /// Calculate total number of tests
    pub fn total_tests(&self) -> u32 {
        self.output_verification.tests_total
            + self.state_verification.tests_total
            + self.process_verification.tests_total
    }
}

/// Level of partial credit achieved
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CreditLevel {
    /// No credit (0%)
    #[default]
    None,
    /// Minimal progress (25%)
    Minimal,
    /// Partial completion (50%)
    Partial,
    /// Substantial completion (75%)
    Substantial,
    /// Full completion (100%)
    Full,
}

impl CreditLevel {
    /// Get the numeric value (0.0 to 1.0) for this credit level
    pub fn value(&self) -> f64 {
        match self {
            CreditLevel::None => 0.0,
            CreditLevel::Minimal => 0.25,
            CreditLevel::Partial => 0.50,
            CreditLevel::Substantial => 0.75,
            CreditLevel::Full => 1.0,
        }
    }

    /// Get a human-readable description of this credit level
    pub fn description(&self) -> &'static str {
        match self {
            CreditLevel::None => "No progress",
            CreditLevel::Minimal => "Minimal progress",
            CreditLevel::Partial => "Partial completion",
            CreditLevel::Substantial => "Substantial completion",
            CreditLevel::Full => "Full completion",
        }
    }

    /// Create a credit level from a percentage (0-100)
    pub fn from_percentage(percentage: f64) -> Self {
        if percentage >= 100.0 {
            CreditLevel::Full
        } else if percentage >= 75.0 {
            CreditLevel::Substantial
        } else if percentage >= 50.0 {
            CreditLevel::Partial
        } else if percentage >= 25.0 {
            CreditLevel::Minimal
        } else {
            CreditLevel::None
        }
    }
}

/// Partial credit for a specific criterion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialCredit {
    /// Name of the criterion being evaluated
    pub criterion: String,
    /// Level of credit achieved
    pub achieved: CreditLevel,
    /// Score for this criterion
    pub score: f64,
    /// Maximum possible score
    pub max_score: f64,
    /// Explanation of why this score was given
    pub reason: String,
}

impl PartialCredit {
    /// Create a new partial credit entry
    pub fn new(criterion: String, achieved: CreditLevel, max_score: f64, reason: String) -> Self {
        let score = max_score * achieved.value();
        Self {
            criterion,
            achieved,
            score,
            max_score,
            reason,
        }
    }

    /// Calculate the percentage of credit achieved
    pub fn percentage(&self) -> f64 {
        self.achieved.value() * 100.0
    }
}

/// Metadata about the reward calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardMetadata {
    /// Task identifier
    pub task_id: String,
    /// Difficulty level (e.g., "easy", "medium", "hard")
    pub difficulty: String,
    /// Category of the task
    pub category: String,
    /// Execution time in seconds
    pub execution_time_seconds: f64,
    /// Timestamp when the test was run
    pub test_timestamp: String,
}

impl RewardMetadata {
    /// Create new reward metadata
    pub fn new(
        task_id: String,
        difficulty: String,
        category: String,
        execution_time_seconds: f64,
    ) -> Self {
        let test_timestamp = chrono::Utc::now().to_rfc3339();
        Self {
            task_id,
            difficulty,
            category,
            execution_time_seconds,
            test_timestamp,
        }
    }
}

/// Complete reward calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    /// Total score achieved
    pub score: f64,
    /// Maximum possible score
    pub max_score: f64,
    /// Score as a percentage
    pub percentage: f64,
    /// Whether the task passed the minimum threshold
    pub passed: bool,
    /// Detailed breakdown by category
    pub breakdown: RewardBreakdown,
    /// Partial credit entries
    pub partial_credit: Vec<PartialCredit>,
    /// Issues or failures encountered
    pub issues: Vec<String>,
    /// Metadata about the evaluation
    pub metadata: RewardMetadata,
}

/// Passing threshold percentage
const PASS_THRESHOLD: f64 = 70.0;

/// Default weights for each category
const OUTPUT_WEIGHT: f64 = 0.5;
const STATE_WEIGHT: f64 = 0.25;
const PROCESS_WEIGHT: f64 = 0.25;

impl Reward {
    /// Calculate reward from test results
    pub fn calculate(
        task_config: &TaskConfig,
        test_results: &TestResults,
        partial_credit: Vec<PartialCredit>,
        execution_time: f64,
    ) -> Self {
        // Apply weights to category scores
        let output_score = Self::calculate_category_score(&test_results.output, OUTPUT_WEIGHT);
        let state_score = Self::calculate_category_score(&test_results.state, STATE_WEIGHT);
        let process_score = Self::calculate_category_score(&test_results.process, PROCESS_WEIGHT);

        // Add partial credit bonus
        let partial_credit_bonus: f64 = partial_credit.iter().map(|pc| pc.score).sum();

        let breakdown = RewardBreakdown {
            output_verification: CategoryScore {
                score: output_score,
                max_score: OUTPUT_WEIGHT,
                tests_passed: test_results.output.tests_passed,
                tests_total: test_results.output.tests_total,
            },
            state_verification: CategoryScore {
                score: state_score,
                max_score: STATE_WEIGHT,
                tests_passed: test_results.state.tests_passed,
                tests_total: test_results.state.tests_total,
            },
            process_verification: CategoryScore {
                score: process_score,
                max_score: PROCESS_WEIGHT,
                tests_passed: test_results.process.tests_passed,
                tests_total: test_results.process.tests_total,
            },
        };

        let total_score = output_score + state_score + process_score + partial_credit_bonus;
        let max_score = OUTPUT_WEIGHT + STATE_WEIGHT + PROCESS_WEIGHT;

        // Clamp score to max_score (partial credit can't exceed maximum)
        let final_score = total_score.min(max_score);

        let percentage = if max_score > 0.0 {
            (final_score / max_score) * 100.0
        } else {
            0.0
        };

        let passed = percentage >= PASS_THRESHOLD;

        let metadata = RewardMetadata::new(
            task_config.id.clone(),
            task_config.difficulty.clone(),
            task_config.category.clone(),
            execution_time,
        );

        Self {
            score: (final_score * 10000.0).round() / 10000.0, // Round to 4 decimal places
            max_score,
            percentage: (percentage * 100.0).round() / 100.0, // Round to 2 decimal places
            passed,
            breakdown,
            partial_credit,
            issues: test_results.issues.clone(),
            metadata,
        }
    }

    /// Calculate weighted score for a category
    fn calculate_category_score(category: &CategoryScore, weight: f64) -> f64 {
        if category.tests_total == 0 {
            0.0
        } else {
            (category.tests_passed as f64 / category.tests_total as f64) * weight
        }
    }

    /// Convert reward to a human-readable text report
    pub fn to_txt(&self) -> String {
        let mut lines = Vec::new();

        lines.push("═══════════════════════════════════════════════════════════".to_string());
        lines.push("                    REWARD CALCULATION REPORT              ".to_string());
        lines.push("═══════════════════════════════════════════════════════════".to_string());
        lines.push(String::new());

        // Metadata
        lines.push(format!("Task ID:     {}", self.metadata.task_id));
        lines.push(format!("Category:    {}", self.metadata.category));
        lines.push(format!("Difficulty:  {}", self.metadata.difficulty));
        lines.push(format!("Executed:    {}", self.metadata.test_timestamp));
        lines.push(format!(
            "Duration:    {:.2}s",
            self.metadata.execution_time_seconds
        ));
        lines.push(String::new());

        // Overall result
        lines.push("───────────────────────────────────────────────────────────".to_string());
        lines.push("                       OVERALL RESULT                      ".to_string());
        lines.push("───────────────────────────────────────────────────────────".to_string());
        lines.push(format!(
            "Score:       {:.4} / {:.2} ({:.2}%)",
            self.score, self.max_score, self.percentage
        ));
        lines.push(format!(
            "Status:      {}",
            if self.passed {
                "PASSED ✓"
            } else {
                "FAILED ✗"
            }
        ));
        lines.push(String::new());

        // Category breakdown
        lines.push("───────────────────────────────────────────────────────────".to_string());
        lines.push("                    CATEGORY BREAKDOWN                     ".to_string());
        lines.push("───────────────────────────────────────────────────────────".to_string());
        lines.push(String::new());

        lines.push(format!(
            "Output Verification:  {:.4} / {:.2}  ({}/{} tests)",
            self.breakdown.output_verification.score,
            self.breakdown.output_verification.max_score,
            self.breakdown.output_verification.tests_passed,
            self.breakdown.output_verification.tests_total
        ));

        lines.push(format!(
            "State Verification:   {:.4} / {:.2}  ({}/{} tests)",
            self.breakdown.state_verification.score,
            self.breakdown.state_verification.max_score,
            self.breakdown.state_verification.tests_passed,
            self.breakdown.state_verification.tests_total
        ));

        lines.push(format!(
            "Process Verification: {:.4} / {:.2}  ({}/{} tests)",
            self.breakdown.process_verification.score,
            self.breakdown.process_verification.max_score,
            self.breakdown.process_verification.tests_passed,
            self.breakdown.process_verification.tests_total
        ));
        lines.push(String::new());

        // Partial credit
        if !self.partial_credit.is_empty() {
            lines.push("───────────────────────────────────────────────────────────".to_string());
            lines.push("                     PARTIAL CREDIT                        ".to_string());
            lines.push("───────────────────────────────────────────────────────────".to_string());
            lines.push(String::new());

            for pc in &self.partial_credit {
                lines.push(format!("• {} ({:?})", pc.criterion, pc.achieved));
                lines.push(format!(
                    "  Score: {:.4} / {:.2} ({:.0}%)",
                    pc.score,
                    pc.max_score,
                    pc.percentage()
                ));
                lines.push(format!("  Reason: {}", pc.reason));
                lines.push(String::new());
            }
        }

        // Issues
        if !self.issues.is_empty() {
            lines.push("───────────────────────────────────────────────────────────".to_string());
            lines.push("                        ISSUES                             ".to_string());
            lines.push("───────────────────────────────────────────────────────────".to_string());
            lines.push(String::new());

            for (i, issue) in self.issues.iter().enumerate() {
                lines.push(format!("{}. {}", i + 1, issue));
            }
            lines.push(String::new());
        }

        lines.push("═══════════════════════════════════════════════════════════".to_string());

        lines.join("\n")
    }

    /// Convert reward to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize reward: {}\"}}", e))
    }
}

/// Task configuration for reward calculation
#[derive(Debug, Clone)]
pub struct TaskConfig {
    /// Unique task identifier
    pub id: String,
    /// Difficulty level
    pub difficulty: String,
    /// Task category
    pub category: String,
}

impl TaskConfig {
    /// Create a new task configuration
    pub fn new(id: String, difficulty: String, category: String) -> Self {
        Self {
            id,
            difficulty,
            category,
        }
    }
}

/// Test results from running verification
#[derive(Debug, Clone)]
pub struct TestResults {
    /// Output verification results
    pub output: CategoryScore,
    /// State verification results
    pub state: CategoryScore,
    /// Process verification results
    pub process: CategoryScore,
    /// Issues encountered during testing
    pub issues: Vec<String>,
}

impl TestResults {
    /// Create new test results
    pub fn new(
        output: CategoryScore,
        state: CategoryScore,
        process: CategoryScore,
        issues: Vec<String>,
    ) -> Self {
        Self {
            output,
            state,
            process,
            issues,
        }
    }

    /// Create empty test results (all zeros)
    pub fn empty() -> Self {
        Self {
            output: CategoryScore::default(),
            state: CategoryScore::default(),
            process: CategoryScore::default(),
            issues: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_level_values() {
        assert_eq!(CreditLevel::None.value(), 0.0);
        assert_eq!(CreditLevel::Minimal.value(), 0.25);
        assert_eq!(CreditLevel::Partial.value(), 0.50);
        assert_eq!(CreditLevel::Substantial.value(), 0.75);
        assert_eq!(CreditLevel::Full.value(), 1.0);
    }

    #[test]
    fn test_credit_level_from_percentage() {
        assert_eq!(CreditLevel::from_percentage(0.0), CreditLevel::None);
        assert_eq!(CreditLevel::from_percentage(24.9), CreditLevel::None);
        assert_eq!(CreditLevel::from_percentage(25.0), CreditLevel::Minimal);
        assert_eq!(CreditLevel::from_percentage(50.0), CreditLevel::Partial);
        assert_eq!(CreditLevel::from_percentage(75.0), CreditLevel::Substantial);
        assert_eq!(CreditLevel::from_percentage(100.0), CreditLevel::Full);
    }

    #[test]
    fn test_category_score_pass_rate() {
        let score = CategoryScore::new(0.3, 0.5, 3, 5);
        assert_eq!(score.pass_rate(), 60.0);
    }

    #[test]
    fn test_category_score_empty() {
        let score = CategoryScore::default();
        assert_eq!(score.pass_rate(), 0.0);
        assert_eq!(score.score_percentage(), 0.0);
    }

    #[test]
    fn test_partial_credit_calculation() {
        let pc = PartialCredit::new(
            "Test criterion".to_string(),
            CreditLevel::Partial,
            1.0,
            "Partially completed".to_string(),
        );
        assert_eq!(pc.score, 0.5);
        assert_eq!(pc.percentage(), 50.0);
    }

    #[test]
    fn test_reward_calculation_full_pass() {
        let task_config = TaskConfig::new(
            "test-001".to_string(),
            "medium".to_string(),
            "code_generation".to_string(),
        );

        let test_results = TestResults::new(
            CategoryScore::new(0.0, 0.0, 4, 4), // All passed
            CategoryScore::new(0.0, 0.0, 3, 3), // All passed
            CategoryScore::new(0.0, 0.0, 2, 2), // All passed
            Vec::new(),
        );

        let reward = Reward::calculate(&task_config, &test_results, Vec::new(), 10.0);

        assert_eq!(reward.percentage, 100.0);
        assert!(reward.passed);
    }

    #[test]
    fn test_reward_calculation_partial() {
        let task_config = TaskConfig::new(
            "test-002".to_string(),
            "hard".to_string(),
            "debugging".to_string(),
        );

        let test_results = TestResults::new(
            CategoryScore::new(0.0, 0.0, 2, 4), // 50% passed
            CategoryScore::new(0.0, 0.0, 2, 4), // 50% passed
            CategoryScore::new(0.0, 0.0, 2, 4), // 50% passed
            vec!["Some tests failed".to_string()],
        );

        let reward = Reward::calculate(&task_config, &test_results, Vec::new(), 20.0);

        assert_eq!(reward.percentage, 50.0);
        assert!(!reward.passed);
        assert_eq!(reward.issues.len(), 1);
    }

    #[test]
    fn test_reward_to_json() {
        let task_config = TaskConfig::new(
            "test-003".to_string(),
            "easy".to_string(),
            "testing".to_string(),
        );

        let test_results = TestResults::empty();
        let reward = Reward::calculate(&task_config, &test_results, Vec::new(), 1.0);

        let json = reward.to_json();
        assert!(json.contains("\"task_id\": \"test-003\""));
        assert!(json.contains("\"passed\": false"));
    }

    #[test]
    fn test_reward_breakdown_totals() {
        let breakdown = RewardBreakdown {
            output_verification: CategoryScore::new(0.25, 0.5, 2, 4),
            state_verification: CategoryScore::new(0.125, 0.25, 2, 4),
            process_verification: CategoryScore::new(0.125, 0.25, 2, 4),
        };

        assert_eq!(breakdown.total_score(), 0.5);
        assert_eq!(breakdown.max_score(), 1.0);
        assert_eq!(breakdown.total_tests_passed(), 6);
        assert_eq!(breakdown.total_tests(), 12);
    }
}
