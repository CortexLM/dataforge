//! Verification system for checking agent outputs against hidden criteria.
//!
//! The verifier loads task.yaml (hidden from agent) and checks the agent's
//! output against the success criteria defined there.

use std::fs;
use std::path::Path;
use std::process::Command;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Result of verifying an agent's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Task identifier.
    pub task_id: String,
    /// Overall score (0.0 to 1.0).
    pub score: f64,
    /// Whether all required checks passed.
    pub passed: bool,
    /// Individual check results.
    pub checks: Vec<CheckResult>,
    /// Partial credit awarded.
    pub partial_credit: Vec<PartialCredit>,
    /// Total points earned.
    pub points_earned: f64,
    /// Maximum possible points.
    pub max_points: f64,
    /// Summary of the verification.
    pub summary: String,
}

impl VerificationResult {
    /// Creates a new verification result.
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            score: 0.0,
            passed: false,
            checks: Vec::new(),
            partial_credit: Vec::new(),
            points_earned: 0.0,
            max_points: 0.0,
            summary: String::new(),
        }
    }

    /// Calculates the final score from checks and partial credit.
    pub fn calculate_score(&mut self) {
        let passed_checks = self.checks.iter().filter(|c| c.passed).count();
        let total_checks = self.checks.len();

        if total_checks > 0 {
            // Base score from checks
            let check_score = passed_checks as f64 / total_checks as f64;
            
            // Add partial credit
            let partial_score: f64 = self.partial_credit.iter().map(|p| p.points).sum();
            
            self.points_earned = check_score * 0.7 + partial_score * 0.3;
            self.max_points = 1.0;
            self.score = self.points_earned.min(1.0);
        }

        self.passed = self.checks.iter().all(|c| c.passed || !c.required);

        self.summary = format!(
            "{}/{} checks passed, score: {:.1}%",
            passed_checks,
            total_checks,
            self.score * 100.0
        );
    }
}

/// Result of a single check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Unique identifier for this check.
    pub check_id: String,
    /// Type of check performed.
    pub check_type: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Whether this check is required for overall pass.
    pub required: bool,
    /// Expected value/pattern.
    pub expected: String,
    /// Actual value found.
    pub actual: String,
    /// Human-readable description.
    pub description: String,
    /// Error message if check failed.
    pub error: Option<String>,
}

impl CheckResult {
    /// Creates a passed check result.
    pub fn pass(
        check_id: impl Into<String>,
        check_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            check_id: check_id.into(),
            check_type: check_type.into(),
            passed: true,
            required: true,
            expected: String::new(),
            actual: String::new(),
            description: description.into(),
            error: None,
        }
    }

    /// Creates a failed check result.
    pub fn fail(
        check_id: impl Into<String>,
        check_type: impl Into<String>,
        description: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            check_id: check_id.into(),
            check_type: check_type.into(),
            passed: false,
            required: true,
            expected: String::new(),
            actual: String::new(),
            description: description.into(),
            error: Some(error.into()),
        }
    }

    /// Sets the expected and actual values.
    pub fn with_values(mut self, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        self.expected = expected.into();
        self.actual = actual.into();
        self
    }

    /// Sets whether this check is required.
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
}

/// Partial credit awarded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialCredit {
    /// Description of what was achieved.
    pub criterion: String,
    /// Points awarded (0.0 to 1.0).
    pub points: f64,
    /// Reasoning for the partial credit.
    pub reasoning: String,
}

/// Check specification from task.yaml.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckSpec {
    /// Unique identifier.
    #[serde(default)]
    pub check_id: String,
    /// Type of check.
    pub check_type: String,
    /// Target (file path, command, etc.).
    pub target: String,
    /// Expected value.
    #[serde(default)]
    pub expected: String,
    /// Description of what this checks.
    #[serde(default)]
    pub description: String,
    /// Whether this check is required.
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Partial credit specification.
#[derive(Debug, Clone, Deserialize)]
pub struct PartialCreditSpec {
    /// Criterion description.
    pub criterion: String,
    /// Points to award if met.
    pub points: f64,
    /// Optional pattern to check.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Optional file to check.
    #[serde(default)]
    pub file: Option<String>,
}

/// The verifier that checks agent outputs.
pub struct Verifier {
    /// Checks to perform.
    checks: Vec<CheckSpec>,
    /// Partial credit criteria.
    partial_credit_specs: Vec<PartialCreditSpec>,
}

impl Verifier {
    /// Creates a new verifier from a task.yaml file.
    pub fn from_task_yaml(task_yaml_path: &Path) -> Result<Self, VerifierError> {
        let content = fs::read_to_string(task_yaml_path).map_err(|e| {
            VerifierError::LoadError(format!("Failed to read task.yaml: {}", e))
        })?;

        let yaml: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| {
            VerifierError::ParseError(format!("Failed to parse task.yaml: {}", e))
        })?;

        // Extract automated checks
        let checks = if let Some(verification) = yaml.get("verification") {
            if let Some(automated) = verification.get("automated_checks") {
                serde_yaml::from_value(automated.clone()).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Extract partial credit specs
        let partial_credit_specs = if let Some(verification) = yaml.get("verification") {
            if let Some(partial) = verification.get("partial_credit_criteria") {
                serde_yaml::from_value(partial.clone()).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Self {
            checks,
            partial_credit_specs,
        })
    }

    /// Creates a verifier with custom checks.
    pub fn with_checks(checks: Vec<CheckSpec>) -> Self {
        Self {
            checks,
            partial_credit_specs: Vec::new(),
        }
    }

    /// Verifies the agent output directory.
    pub fn verify(&self, output_dir: &Path, task_id: &str) -> VerificationResult {
        let mut result = VerificationResult::new(task_id);

        info!("Verifying output in {}", output_dir.display());

        // Run all checks
        for (idx, check) in self.checks.iter().enumerate() {
            let check_id = if check.check_id.is_empty() {
                format!("check-{}", idx + 1)
            } else {
                check.check_id.clone()
            };

            let check_result = self.run_check(&check_id, check, output_dir);
            debug!(
                "Check {}: {} - {}",
                check_id,
                check.check_type,
                if check_result.passed { "PASS" } else { "FAIL" }
            );
            result.checks.push(check_result);
        }

        // Evaluate partial credit
        for spec in &self.partial_credit_specs {
            if let Some(credit) = self.evaluate_partial_credit(spec, output_dir) {
                result.partial_credit.push(credit);
            }
        }

        result.calculate_score();

        info!(
            "Verification complete: {} - score {:.1}%",
            if result.passed { "PASSED" } else { "FAILED" },
            result.score * 100.0
        );

        result
    }

    /// Runs a single check.
    fn run_check(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        match check.check_type.as_str() {
            "file_exists" => self.check_file_exists(check_id, check, output_dir),
            "file_contains" => self.check_file_contains(check_id, check, output_dir),
            "file_not_contains" => self.check_file_not_contains(check_id, check, output_dir),
            "output_contains" => self.check_command_output(check_id, check, output_dir),
            "command_succeeds" => self.check_command_succeeds(check_id, check, output_dir),
            "json_valid" => self.check_json_valid(check_id, check, output_dir),
            "regex_match" => self.check_regex_match(check_id, check, output_dir),
            "line_count" => self.check_line_count(check_id, check, output_dir),
            other => CheckResult::fail(
                check_id,
                other,
                &check.description,
                format!("Unknown check type: {}", other),
            ),
        }
    }

    /// Checks if a file exists.
    fn check_file_exists(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);
        let exists = file_path.exists();

        let expected_exists = check.expected.is_empty() || check.expected == "true";

        if exists == expected_exists {
            CheckResult::pass(check_id, "file_exists", &check.description)
                .with_values(
                    format!("exists={}", expected_exists),
                    format!("exists={}", exists),
                )
                .with_required(check.required)
        } else {
            CheckResult::fail(
                check_id,
                "file_exists",
                &check.description,
                if expected_exists {
                    format!("File not found: {}", check.target)
                } else {
                    format!("File should not exist: {}", check.target)
                },
            )
            .with_values(
                format!("exists={}", expected_exists),
                format!("exists={}", exists),
            )
            .with_required(check.required)
        }
    }

    /// Checks if a file contains a pattern.
    fn check_file_contains(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);

        match fs::read_to_string(&file_path) {
            Ok(content) => {
                if content.contains(&check.expected) {
                    CheckResult::pass(check_id, "file_contains", &check.description)
                        .with_values(&check.expected, "[found]")
                        .with_required(check.required)
                } else {
                    CheckResult::fail(
                        check_id,
                        "file_contains",
                        &check.description,
                        format!("Pattern '{}' not found in {}", check.expected, check.target),
                    )
                    .with_values(&check.expected, "[not found]")
                    .with_required(check.required)
                }
            }
            Err(e) => CheckResult::fail(
                check_id,
                "file_contains",
                &check.description,
                format!("Failed to read {}: {}", check.target, e),
            )
            .with_required(check.required),
        }
    }

    /// Checks that a file does NOT contain a pattern.
    fn check_file_not_contains(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);

        match fs::read_to_string(&file_path) {
            Ok(content) => {
                if !content.contains(&check.expected) {
                    CheckResult::pass(check_id, "file_not_contains", &check.description)
                        .with_values(format!("not '{}'", check.expected), "[not found - good]")
                        .with_required(check.required)
                } else {
                    CheckResult::fail(
                        check_id,
                        "file_not_contains",
                        &check.description,
                        format!("Forbidden pattern '{}' found in {}", check.expected, check.target),
                    )
                    .with_values(format!("not '{}'", check.expected), "[found - bad]")
                    .with_required(check.required)
                }
            }
            Err(_) => {
                // File doesn't exist - pattern not found, so pass
                CheckResult::pass(check_id, "file_not_contains", &check.description)
                    .with_required(check.required)
            }
        }
    }

    /// Checks command output contains expected text.
    fn check_command_output(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let output = Command::new("sh")
            .arg("-c")
            .arg(&check.target)
            .current_dir(output_dir)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let combined = format!("{}{}", stdout, stderr);

                if combined.contains(&check.expected) {
                    CheckResult::pass(check_id, "output_contains", &check.description)
                        .with_values(&check.expected, "[found]")
                        .with_required(check.required)
                } else {
                    CheckResult::fail(
                        check_id,
                        "output_contains",
                        &check.description,
                        format!("Expected '{}' in command output", check.expected),
                    )
                    .with_values(&check.expected, truncate(&combined, 200))
                    .with_required(check.required)
                }
            }
            Err(e) => CheckResult::fail(
                check_id,
                "output_contains",
                &check.description,
                format!("Command failed: {}", e),
            )
            .with_required(check.required),
        }
    }

    /// Checks if a command succeeds (exit code 0).
    fn check_command_succeeds(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let output = Command::new("sh")
            .arg("-c")
            .arg(&check.target)
            .current_dir(output_dir)
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    CheckResult::pass(check_id, "command_succeeds", &check.description)
                        .with_values("exit 0", "exit 0")
                        .with_required(check.required)
                } else {
                    let code = out.status.code().unwrap_or(-1);
                    CheckResult::fail(
                        check_id,
                        "command_succeeds",
                        &check.description,
                        format!("Command exited with code {}", code),
                    )
                    .with_values("exit 0", format!("exit {}", code))
                    .with_required(check.required)
                }
            }
            Err(e) => CheckResult::fail(
                check_id,
                "command_succeeds",
                &check.description,
                format!("Failed to run command: {}", e),
            )
            .with_required(check.required),
        }
    }

    /// Checks if a file is valid JSON.
    fn check_json_valid(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);

        match fs::read_to_string(&file_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(_) => CheckResult::pass(check_id, "json_valid", &check.description)
                    .with_required(check.required),
                Err(e) => CheckResult::fail(
                    check_id,
                    "json_valid",
                    &check.description,
                    format!("Invalid JSON: {}", e),
                )
                .with_required(check.required),
            },
            Err(e) => CheckResult::fail(
                check_id,
                "json_valid",
                &check.description,
                format!("Failed to read file: {}", e),
            )
            .with_required(check.required),
        }
    }

    /// Checks if file content matches a regex.
    fn check_regex_match(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);

        match fs::read_to_string(&file_path) {
            Ok(content) => match Regex::new(&check.expected) {
                Ok(re) => {
                    if re.is_match(&content) {
                        CheckResult::pass(check_id, "regex_match", &check.description)
                            .with_values(&check.expected, "[matches]")
                            .with_required(check.required)
                    } else {
                        CheckResult::fail(
                            check_id,
                            "regex_match",
                            &check.description,
                            "Pattern did not match",
                        )
                        .with_values(&check.expected, "[no match]")
                        .with_required(check.required)
                    }
                }
                Err(e) => CheckResult::fail(
                    check_id,
                    "regex_match",
                    &check.description,
                    format!("Invalid regex: {}", e),
                )
                .with_required(check.required),
            },
            Err(e) => CheckResult::fail(
                check_id,
                "regex_match",
                &check.description,
                format!("Failed to read file: {}", e),
            )
            .with_required(check.required),
        }
    }

    /// Checks line count of a file.
    fn check_line_count(&self, check_id: &str, check: &CheckSpec, output_dir: &Path) -> CheckResult {
        let file_path = output_dir.join(&check.target);

        match fs::read_to_string(&file_path) {
            Ok(content) => {
                let count = content.lines().count();
                let expected: usize = check.expected.parse().unwrap_or(0);

                if count >= expected {
                    CheckResult::pass(check_id, "line_count", &check.description)
                        .with_values(format!(">= {}", expected), count.to_string())
                        .with_required(check.required)
                } else {
                    CheckResult::fail(
                        check_id,
                        "line_count",
                        &check.description,
                        format!("Expected at least {} lines, found {}", expected, count),
                    )
                    .with_values(format!(">= {}", expected), count.to_string())
                    .with_required(check.required)
                }
            }
            Err(e) => CheckResult::fail(
                check_id,
                "line_count",
                &check.description,
                format!("Failed to read file: {}", e),
            )
            .with_required(check.required),
        }
    }

    /// Evaluates partial credit criteria.
    fn evaluate_partial_credit(
        &self,
        spec: &PartialCreditSpec,
        output_dir: &Path,
    ) -> Option<PartialCredit> {
        // Check file pattern if specified
        if let Some(ref file) = spec.file {
            let file_path = output_dir.join(file);
            if !file_path.exists() {
                return None;
            }

            if let Some(ref pattern) = spec.pattern {
                let content = fs::read_to_string(&file_path).ok()?;
                if !content.contains(pattern) {
                    return None;
                }
            }
        }

        Some(PartialCredit {
            criterion: spec.criterion.clone(),
            points: spec.points,
            reasoning: format!("Criterion met: {}", spec.criterion),
        })
    }
}

/// Truncates a string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Errors from the verifier.
#[derive(Debug, thiserror::Error)]
pub enum VerifierError {
    #[error("Failed to load task: {0}")]
    LoadError(String),

    #[error("Failed to parse task: {0}")]
    ParseError(String),

    #[error("Check error: {0}")]
    CheckError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_output(dir: &Path) {
        fs::write(dir.join("output.txt"), "hello world\ntest line").unwrap();
        fs::write(dir.join("data.json"), r#"{"key": "value"}"#).unwrap();
    }

    #[test]
    fn test_file_exists_check() {
        let temp = TempDir::new().unwrap();
        create_test_output(temp.path());

        let verifier = Verifier::with_checks(vec![
            CheckSpec {
                check_id: "exists-1".to_string(),
                check_type: "file_exists".to_string(),
                target: "output.txt".to_string(),
                expected: "true".to_string(),
                description: "Output file exists".to_string(),
                required: true,
            },
            CheckSpec {
                check_id: "exists-2".to_string(),
                check_type: "file_exists".to_string(),
                target: "missing.txt".to_string(),
                expected: "true".to_string(),
                description: "Missing file".to_string(),
                required: true,
            },
        ]);

        let result = verifier.verify(temp.path(), "test-task");

        assert_eq!(result.checks.len(), 2);
        assert!(result.checks[0].passed);
        assert!(!result.checks[1].passed);
    }

    #[test]
    fn test_file_contains_check() {
        let temp = TempDir::new().unwrap();
        create_test_output(temp.path());

        let verifier = Verifier::with_checks(vec![CheckSpec {
            check_id: "contains-1".to_string(),
            check_type: "file_contains".to_string(),
            target: "output.txt".to_string(),
            expected: "hello".to_string(),
            description: "Contains hello".to_string(),
            required: true,
        }]);

        let result = verifier.verify(temp.path(), "test-task");

        assert!(result.checks[0].passed);
    }

    #[test]
    fn test_json_valid_check() {
        let temp = TempDir::new().unwrap();
        create_test_output(temp.path());

        let verifier = Verifier::with_checks(vec![CheckSpec {
            check_id: "json-1".to_string(),
            check_type: "json_valid".to_string(),
            target: "data.json".to_string(),
            expected: String::new(),
            description: "Valid JSON".to_string(),
            required: true,
        }]);

        let result = verifier.verify(temp.path(), "test-task");

        assert!(result.checks[0].passed);
    }

    #[test]
    fn test_score_calculation() {
        let mut result = VerificationResult::new("test");
        result.checks.push(CheckResult::pass("c1", "test", "desc1"));
        result.checks.push(CheckResult::pass("c2", "test", "desc2"));
        result.checks.push(CheckResult::fail("c3", "test", "desc3", "error"));

        result.calculate_score();

        assert!(!result.passed); // One check failed
        assert!(result.score > 0.0 && result.score < 1.0);
    }
}
