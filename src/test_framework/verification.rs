// Verification helpers for output validation
// Provides utilities to verify actual outputs against expected specifications

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::test_framework::pytest_generator::{ContentCheck, ExpectedOutput};

/// Result of verifying a single output
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Name/identifier of the verified item
    pub name: String,
    /// Whether the verification passed
    pub passed: bool,
    /// Expected value (if applicable)
    pub expected: Option<String>,
    /// Actual value found
    pub actual: Option<String>,
    /// Human-readable explanation of the result
    pub reason: String,
}

impl VerificationResult {
    /// Create a new passing verification result
    pub fn pass(name: String, reason: String) -> Self {
        Self {
            name,
            passed: true,
            expected: None,
            actual: None,
            reason,
        }
    }

    /// Create a new failing verification result
    pub fn fail(name: String, reason: String) -> Self {
        Self {
            name,
            passed: false,
            expected: None,
            actual: None,
            reason,
        }
    }

    /// Create a result with expected/actual comparison
    pub fn with_comparison(
        name: String,
        passed: bool,
        expected: String,
        actual: String,
        reason: String,
    ) -> Self {
        Self {
            name,
            passed,
            expected: Some(expected),
            actual: Some(actual),
            reason,
        }
    }
}

/// Verifies actual outputs against expected specifications
pub struct OutputVerifier {
    /// Map of output name to expected output specification
    expected_outputs: HashMap<String, ExpectedOutput>,
}

impl OutputVerifier {
    /// Create a new output verifier
    pub fn new(expected_outputs: HashMap<String, ExpectedOutput>) -> Self {
        Self { expected_outputs }
    }

    /// Verify all expected outputs against the actual directory
    pub fn verify(&self, actual_dir: &Path) -> Vec<VerificationResult> {
        let mut results = Vec::new();

        for (name, expected) in &self.expected_outputs {
            let file_path = actual_dir.join(&expected.path);
            let file_results = self.verify_single_output(name, expected, &file_path);
            results.extend(file_results);
        }

        results
    }

    /// Verify a single output file
    fn verify_single_output(
        &self,
        name: &str,
        expected: &ExpectedOutput,
        file_path: &Path,
    ) -> Vec<VerificationResult> {
        let mut results = Vec::new();

        // Check if file physically exists
        let file_physically_exists = file_path.exists();

        // Check file existence (will pass for optional missing files)
        let existence_result = self.verify_file_existence(name, expected, file_path);
        results.push(existence_result);

        // If file doesn't physically exist, skip content checks
        // (whether it passed or failed depends on required flag)
        if !file_physically_exists {
            return results;
        }

        // Read file content
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                results.push(VerificationResult::fail(
                    format!("{}_read", name),
                    format!("Failed to read file: {}", e),
                ));
                return results;
            }
        };

        // Verify format
        results.push(self.verify_format(name, expected, &content));

        // Verify content checks
        for check in &expected.content_checks {
            results.push(self.verify_content_check(name, check, &content, &expected.format));
        }

        results
    }

    /// Verify file existence
    fn verify_file_existence(
        &self,
        name: &str,
        expected: &ExpectedOutput,
        file_path: &Path,
    ) -> VerificationResult {
        let exists = file_path.exists();

        if exists {
            VerificationResult::pass(
                format!("{}_exists", name),
                format!("File exists at {}", expected.path),
            )
        } else if expected.required {
            VerificationResult::fail(
                format!("{}_exists", name),
                format!("Required file not found: {}", expected.path),
            )
        } else {
            VerificationResult::pass(
                format!("{}_exists", name),
                format!("Optional file not present: {}", expected.path),
            )
        }
    }

    /// Verify file format
    fn verify_format(
        &self,
        name: &str,
        expected: &ExpectedOutput,
        content: &str,
    ) -> VerificationResult {
        match expected.format.as_str() {
            "json" => match serde_json::from_str::<serde_json::Value>(content) {
                Ok(_) => VerificationResult::pass(
                    format!("{}_format", name),
                    "Valid JSON format".to_string(),
                ),
                Err(e) => VerificationResult::fail(
                    format!("{}_format", name),
                    format!("Invalid JSON: {}", e),
                ),
            },
            "txt" | "text" => {
                // Plain text is always valid
                VerificationResult::pass(
                    format!("{}_format", name),
                    "Valid text format".to_string(),
                )
            }
            "csv" => {
                // Basic CSV validation - check for consistent column count
                let lines: Vec<&str> = content.lines().collect();
                if lines.is_empty() {
                    return VerificationResult::pass(
                        format!("{}_format", name),
                        "Empty CSV file".to_string(),
                    );
                }

                let expected_cols = lines[0].split(',').count();
                for (i, line) in lines.iter().enumerate() {
                    let cols = line.split(',').count();
                    if cols != expected_cols {
                        return VerificationResult::fail(
                            format!("{}_format", name),
                            format!(
                                "CSV column count mismatch at line {}: expected {}, got {}",
                                i + 1,
                                expected_cols,
                                cols
                            ),
                        );
                    }
                }

                VerificationResult::pass(
                    format!("{}_format", name),
                    format!("Valid CSV with {} columns", expected_cols),
                )
            }
            _ => {
                // Unknown format - pass but note it
                VerificationResult::pass(
                    format!("{}_format", name),
                    format!("Unknown format '{}', skipping validation", expected.format),
                )
            }
        }
    }

    /// Verify a single content check
    fn verify_content_check(
        &self,
        name: &str,
        check: &ContentCheck,
        content: &str,
        format: &str,
    ) -> VerificationResult {
        let check_name = format!("{}_{}", name, check.check_type);

        match check.check_type.as_str() {
            "contains" => {
                if content.contains(&check.value) {
                    VerificationResult::pass(check_name, check.description.clone())
                } else {
                    VerificationResult::with_comparison(
                        check_name,
                        false,
                        format!("Contains: {}", check.value),
                        "Not found".to_string(),
                        check.description.clone(),
                    )
                }
            }
            "equals" => {
                let trimmed_content = content.trim();
                let trimmed_expected = check.value.trim();
                if trimmed_content == trimmed_expected {
                    VerificationResult::pass(check_name, check.description.clone())
                } else {
                    VerificationResult::with_comparison(
                        check_name,
                        false,
                        trimmed_expected.to_string(),
                        Self::truncate_string(trimmed_content, 100),
                        check.description.clone(),
                    )
                }
            }
            "matches_regex" => match regex::Regex::new(&check.value) {
                Ok(re) => {
                    if re.is_match(content) {
                        VerificationResult::pass(check_name, check.description.clone())
                    } else {
                        VerificationResult::with_comparison(
                            check_name,
                            false,
                            format!("Regex: {}", check.value),
                            "No match".to_string(),
                            check.description.clone(),
                        )
                    }
                }
                Err(e) => VerificationResult::fail(
                    check_name,
                    format!("Invalid regex '{}': {}", check.value, e),
                ),
            },
            "json_path" => {
                if format != "json" {
                    return VerificationResult::fail(
                        check_name,
                        format!("json_path check requires JSON format, got '{}'", format),
                    );
                }

                match serde_json::from_str::<serde_json::Value>(content) {
                    Ok(json) => {
                        if self.check_json_path(&json, &check.value) {
                            VerificationResult::pass(check_name, check.description.clone())
                        } else {
                            VerificationResult::with_comparison(
                                check_name,
                                false,
                                format!("JSON path: {}", check.value),
                                "Path not found or null".to_string(),
                                check.description.clone(),
                            )
                        }
                    }
                    Err(e) => {
                        VerificationResult::fail(check_name, format!("Failed to parse JSON: {}", e))
                    }
                }
            }
            _ => VerificationResult::fail(
                check_name,
                format!("Unknown check type: {}", check.check_type),
            ),
        }
    }

    /// Check if a JSON path exists and has a non-null value
    fn check_json_path(&self, json: &serde_json::Value, path: &str) -> bool {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            match current {
                serde_json::Value::Object(map) => match map.get(part) {
                    Some(v) => current = v,
                    None => return false,
                },
                serde_json::Value::Array(arr) => match part.parse::<usize>() {
                    Ok(idx) => match arr.get(idx) {
                        Some(v) => current = v,
                        None => return false,
                    },
                    Err(_) => return false,
                },
                _ => return false,
            }
        }

        !current.is_null()
    }

    /// Truncate a string for display purposes
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }
}

/// Summary of verification results
#[derive(Debug)]
pub struct VerificationSummary {
    /// Total number of checks
    pub total: usize,
    /// Number of passed checks
    pub passed: usize,
    /// Number of failed checks
    pub failed: usize,
    /// All individual results
    pub results: Vec<VerificationResult>,
}

impl VerificationSummary {
    /// Create a summary from a list of results
    pub fn from_results(results: Vec<VerificationResult>) -> Self {
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;

        Self {
            total: results.len(),
            passed,
            failed,
            results,
        }
    }

    /// Check if all verifications passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get pass rate as percentage
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }

    /// Get only the failed results
    pub fn failures(&self) -> Vec<&VerificationResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }
        let mut file = fs::File::create(&path).expect("Failed to create file");
        file.write_all(content.as_bytes())
            .expect("Failed to write content");
    }

    #[test]
    fn test_verification_result_constructors() {
        let pass = VerificationResult::pass("test".to_string(), "Passed".to_string());
        assert!(pass.passed);
        assert_eq!(pass.name, "test");

        let fail = VerificationResult::fail("test2".to_string(), "Failed".to_string());
        assert!(!fail.passed);
        assert_eq!(fail.name, "test2");

        let comparison = VerificationResult::with_comparison(
            "test3".to_string(),
            true,
            "expected".to_string(),
            "actual".to_string(),
            "Compare".to_string(),
        );
        assert!(comparison.passed);
        assert_eq!(comparison.expected, Some("expected".to_string()));
        assert_eq!(comparison.actual, Some("actual".to_string()));
    }

    #[test]
    fn test_verify_json_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        create_test_file(
            temp_dir.path(),
            "output.json",
            r#"{"status": "success", "value": 42}"#,
        );

        let mut expected = HashMap::new();
        expected.insert(
            "result".to_string(),
            ExpectedOutput {
                path: "output.json".to_string(),
                format: "json".to_string(),
                content_checks: vec![
                    ContentCheck {
                        check_type: "json_path".to_string(),
                        value: "status".to_string(),
                        description: "Status exists".to_string(),
                    },
                    ContentCheck {
                        check_type: "contains".to_string(),
                        value: "success".to_string(),
                        description: "Contains success".to_string(),
                    },
                ],
                required: true,
            },
        );

        let verifier = OutputVerifier::new(expected);
        let results = verifier.verify(temp_dir.path());
        let summary = VerificationSummary::from_results(results);

        assert!(summary.all_passed(), "Expected all checks to pass");
    }

    #[test]
    fn test_verify_missing_required_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let mut expected = HashMap::new();
        expected.insert(
            "missing".to_string(),
            ExpectedOutput {
                path: "does_not_exist.txt".to_string(),
                format: "txt".to_string(),
                content_checks: Vec::new(),
                required: true,
            },
        );

        let verifier = OutputVerifier::new(expected);
        let results = verifier.verify(temp_dir.path());
        let summary = VerificationSummary::from_results(results);

        assert!(!summary.all_passed(), "Expected failure for missing file");
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn test_verify_optional_missing_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let mut expected = HashMap::new();
        expected.insert(
            "optional".to_string(),
            ExpectedOutput {
                path: "optional.txt".to_string(),
                format: "txt".to_string(),
                content_checks: Vec::new(),
                required: false,
            },
        );

        let verifier = OutputVerifier::new(expected);
        let results = verifier.verify(temp_dir.path());
        let summary = VerificationSummary::from_results(results);

        assert!(
            summary.all_passed(),
            "Optional missing file should not fail"
        );
    }

    #[test]
    fn test_verify_content_contains() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        create_test_file(temp_dir.path(), "log.txt", "Hello World\nFoo Bar\n");

        let mut expected = HashMap::new();
        expected.insert(
            "log".to_string(),
            ExpectedOutput {
                path: "log.txt".to_string(),
                format: "txt".to_string(),
                content_checks: vec![
                    ContentCheck {
                        check_type: "contains".to_string(),
                        value: "Hello".to_string(),
                        description: "Contains Hello".to_string(),
                    },
                    ContentCheck {
                        check_type: "contains".to_string(),
                        value: "NotPresent".to_string(),
                        description: "Contains NotPresent".to_string(),
                    },
                ],
                required: true,
            },
        );

        let verifier = OutputVerifier::new(expected);
        let results = verifier.verify(temp_dir.path());
        let summary = VerificationSummary::from_results(results);

        assert_eq!(summary.failed, 1, "One content check should fail");
    }

    #[test]
    fn test_verify_csv_format() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        create_test_file(temp_dir.path(), "data.csv", "a,b,c\n1,2,3\n4,5,6\n");

        let mut expected = HashMap::new();
        expected.insert(
            "data".to_string(),
            ExpectedOutput {
                path: "data.csv".to_string(),
                format: "csv".to_string(),
                content_checks: Vec::new(),
                required: true,
            },
        );

        let verifier = OutputVerifier::new(expected);
        let results = verifier.verify(temp_dir.path());
        let summary = VerificationSummary::from_results(results);

        assert!(summary.all_passed(), "Valid CSV should pass");
    }

    #[test]
    fn test_verification_summary() {
        let results = vec![
            VerificationResult::pass("test1".to_string(), "OK".to_string()),
            VerificationResult::fail("test2".to_string(), "Failed".to_string()),
            VerificationResult::pass("test3".to_string(), "OK".to_string()),
        ];

        let summary = VerificationSummary::from_results(results);

        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert!(!summary.all_passed());
        assert!((summary.pass_rate() - 66.67).abs() < 1.0);
        assert_eq!(summary.failures().len(), 1);
    }
}
