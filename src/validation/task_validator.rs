//! Task validation for quality and completeness checks.
//!
//! This module provides validation for generated task instances,
//! ensuring they meet quality standards before being added to the registry.

use serde::{Deserialize, Serialize};

/// Result of an individual validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Name of the check that was performed.
    pub check_name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Optional message with details about the check result.
    pub message: Option<String>,
}

impl CheckResult {
    /// Create a passing check result.
    pub fn pass(name: impl Into<String>) -> Self {
        Self {
            check_name: name.into(),
            passed: true,
            message: None,
        }
    }

    /// Create a passing check result with a message.
    pub fn pass_with_message(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            check_name: name.into(),
            passed: true,
            message: Some(message.into()),
        }
    }

    /// Create a failing check result with a reason.
    pub fn fail(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            check_name: name.into(),
            passed: false,
            message: Some(reason.into()),
        }
    }
}

/// Result of task validation containing all check results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskValidationResult {
    /// Whether all validation checks passed.
    pub valid: bool,
    /// Individual check results.
    pub checks: Vec<CheckResult>,
    /// Overall validation score (0.0 to 1.0).
    pub score: f64,
    /// Summary message.
    pub summary: String,
}

impl TaskValidationResult {
    /// Create a new validation result from check results.
    pub fn new(checks: Vec<CheckResult>) -> Self {
        let passed = checks.iter().filter(|c| c.passed).count();
        let total = checks.len();
        let valid = checks.iter().all(|c| c.passed);
        let score = if total > 0 {
            passed as f64 / total as f64
        } else {
            1.0
        };

        let summary = if valid {
            format!("All {} checks passed", total)
        } else {
            let failed = total - passed;
            format!("{} of {} checks failed", failed, total)
        };

        Self {
            valid,
            checks,
            score,
            summary,
        }
    }

    /// Create a successful validation result.
    pub fn success() -> Self {
        Self {
            valid: true,
            checks: Vec::new(),
            score: 1.0,
            summary: "Validation successful".to_string(),
        }
    }
}

/// Validator for task instances.
///
/// Performs various checks to ensure task quality and completeness.
pub struct TaskValidator {
    /// Whether to require a solution file.
    require_solution: bool,
    /// Whether to require test files.
    require_tests: bool,
    /// Minimum description length.
    min_description_length: usize,
}

impl Default for TaskValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskValidator {
    /// Create a new task validator with default settings.
    pub fn new() -> Self {
        Self {
            require_solution: true,
            require_tests: false,
            min_description_length: 50,
        }
    }

    /// Set whether to require a solution file.
    pub fn with_require_solution(mut self, require: bool) -> Self {
        self.require_solution = require;
        self
    }

    /// Set whether to require test files.
    pub fn with_require_tests(mut self, require: bool) -> Self {
        self.require_tests = require;
        self
    }

    /// Set the minimum description length.
    pub fn with_min_description_length(mut self, length: usize) -> Self {
        self.min_description_length = length;
        self
    }

    /// Validate a task directory.
    ///
    /// # Arguments
    /// * `task_dir` - Path to the task directory to validate
    ///
    /// # Returns
    /// A `TaskValidationResult` with all check results
    pub fn validate(&self, task_dir: &std::path::Path) -> TaskValidationResult {
        let mut checks = Vec::new();

        // Check task directory exists
        if !task_dir.exists() {
            checks.push(CheckResult::fail(
                "directory_exists",
                format!("Task directory does not exist: {}", task_dir.display()),
            ));
            return TaskValidationResult::new(checks);
        }

        checks.push(CheckResult::pass("directory_exists"));

        // Check for task.json or task.yaml
        let task_json = task_dir.join("task.json");
        let task_yaml = task_dir.join("task.yaml");

        if task_json.exists() || task_yaml.exists() {
            checks.push(CheckResult::pass("task_metadata"));
        } else {
            checks.push(CheckResult::fail(
                "task_metadata",
                "No task.json or task.yaml found in task directory",
            ));
        }

        // Check for description file
        let description = task_dir.join("DESCRIPTION.md");
        if description.exists() {
            match std::fs::read_to_string(&description) {
                Ok(content) => {
                    if content.len() >= self.min_description_length {
                        checks.push(CheckResult::pass("description"));
                    } else {
                        checks.push(CheckResult::fail(
                            "description",
                            format!(
                                "Description too short: {} chars (minimum: {})",
                                content.len(),
                                self.min_description_length
                            ),
                        ));
                    }
                }
                Err(e) => {
                    checks.push(CheckResult::fail(
                        "description",
                        format!("Failed to read description: {}", e),
                    ));
                }
            }
        } else {
            checks.push(CheckResult::fail("description", "DESCRIPTION.md not found"));
        }

        // Check for solution if required
        if self.require_solution {
            let solution_dir = task_dir.join("solution");
            if solution_dir.exists() && solution_dir.is_dir() {
                checks.push(CheckResult::pass("solution"));
            } else {
                checks.push(CheckResult::fail(
                    "solution",
                    "Solution directory not found",
                ));
            }
        }

        // Check for tests if required
        if self.require_tests {
            let tests_dir = task_dir.join("tests");
            if tests_dir.exists() && tests_dir.is_dir() {
                checks.push(CheckResult::pass("tests"));
            } else {
                checks.push(CheckResult::fail("tests", "Tests directory not found"));
            }
        }

        TaskValidationResult::new(checks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_check_result_pass() {
        let result = CheckResult::pass("test_check");
        assert!(result.passed);
        assert_eq!(result.check_name, "test_check");
        assert!(result.message.is_none());
    }

    #[test]
    fn test_check_result_fail() {
        let result = CheckResult::fail("test_check", "Something went wrong");
        assert!(!result.passed);
        assert_eq!(result.check_name, "test_check");
        assert_eq!(result.message, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_validation_result_new() {
        let checks = vec![
            CheckResult::pass("check1"),
            CheckResult::fail("check2", "Failed"),
        ];
        let result = TaskValidationResult::new(checks);

        assert!(!result.valid);
        assert_eq!(result.checks.len(), 2);
        assert_eq!(result.score, 0.5);
    }

    #[test]
    fn test_validator_missing_directory() {
        let validator = TaskValidator::new();
        let result = validator.validate(std::path::Path::new("/nonexistent/path"));

        assert!(!result.valid);
        assert!(result
            .checks
            .iter()
            .any(|c| c.check_name == "directory_exists" && !c.passed));
    }

    #[test]
    fn test_validator_valid_task() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let task_dir = temp_dir.path();

        // Create task.json
        fs::write(task_dir.join("task.json"), "{}").expect("Failed to write task.json");

        // Create DESCRIPTION.md with enough content
        let description = "# Task Description\n\nThis is a detailed task description that meets the minimum length requirement for validation.";
        fs::write(task_dir.join("DESCRIPTION.md"), description)
            .expect("Failed to write description");

        // Create solution directory
        fs::create_dir(task_dir.join("solution")).expect("Failed to create solution dir");

        let validator = TaskValidator::new();
        let result = validator.validate(task_dir);

        assert!(result.valid, "Expected valid task, got: {:?}", result);
    }
}
