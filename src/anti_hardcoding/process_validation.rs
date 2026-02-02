//! Process validation for tracking and verifying command execution.
//!
//! This module provides functionality to trace and validate the commands
//! executed during benchmark evaluation. It can enforce required commands,
//! detect forbidden patterns, and compute validation scores.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Record of a single command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecution {
    /// The command that was executed
    pub command: String,
    /// Unix timestamp when execution started
    pub timestamp: f64,
    /// Exit code returned by the command
    pub exit_code: i32,
    /// Standard output captured from the command
    pub stdout: String,
    /// Standard error captured from the command
    pub stderr: String,
    /// Duration of execution in seconds
    pub duration: f64,
}

impl CommandExecution {
    /// Create a new command execution record.
    ///
    /// # Arguments
    /// * `command` - The command string that was executed
    /// * `timestamp` - Unix timestamp of execution start
    /// * `exit_code` - The exit code from the command
    /// * `stdout` - Captured standard output
    /// * `stderr` - Captured standard error
    /// * `duration` - Execution duration in seconds
    pub fn new(
        command: String,
        timestamp: f64,
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration: f64,
    ) -> Self {
        Self {
            command,
            timestamp,
            exit_code,
            stdout,
            stderr,
            duration,
        }
    }

    /// Check if the command execution was successful (exit code 0).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Configuration for process validation rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessValidationConfig {
    /// Regex patterns for required commands (at least one must match)
    pub required_patterns: Vec<String>,
    /// Regex patterns for forbidden commands (none should match)
    pub forbidden_patterns: Vec<String>,
    /// Minimum number of commands expected
    pub min_commands: usize,
    /// Maximum allowed execution time for any single command (seconds)
    pub max_single_command_duration: Option<f64>,
    /// Maximum total execution time (seconds)
    pub max_total_duration: Option<f64>,
    /// Whether all commands must succeed (exit code 0)
    pub require_all_success: bool,
}

impl ProcessValidationConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a required command pattern.
    ///
    /// # Arguments
    /// * `pattern` - Regex pattern that must match at least one executed command
    pub fn with_required_pattern(mut self, pattern: &str) -> Self {
        self.required_patterns.push(pattern.to_string());
        self
    }

    /// Add a forbidden command pattern.
    ///
    /// # Arguments
    /// * `pattern` - Regex pattern that must not match any executed command
    pub fn with_forbidden_pattern(mut self, pattern: &str) -> Self {
        self.forbidden_patterns.push(pattern.to_string());
        self
    }

    /// Set minimum number of commands.
    pub fn with_min_commands(mut self, min: usize) -> Self {
        self.min_commands = min;
        self
    }

    /// Set requirement for all commands to succeed.
    pub fn with_require_all_success(mut self) -> Self {
        self.require_all_success = true;
        self
    }

    /// Set maximum duration for any single command.
    pub fn with_max_single_duration(mut self, seconds: f64) -> Self {
        self.max_single_command_duration = Some(seconds);
        self
    }

    /// Set maximum total execution duration.
    pub fn with_max_total_duration(mut self, seconds: f64) -> Self {
        self.max_total_duration = Some(seconds);
        self
    }
}

/// Tracer for recording and validating command executions.
pub struct ProcessTracer {
    config: ProcessValidationConfig,
    executions: Vec<CommandExecution>,
}

impl ProcessTracer {
    /// Create a new process tracer with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Validation configuration to use
    pub fn new(config: ProcessValidationConfig) -> Self {
        Self {
            config,
            executions: Vec::new(),
        }
    }

    /// Record a command execution.
    ///
    /// # Arguments
    /// * `execution` - The command execution record to add
    pub fn record(&mut self, execution: CommandExecution) {
        self.executions.push(execution);
    }

    /// Get all recorded executions.
    pub fn executions(&self) -> &[CommandExecution] {
        &self.executions
    }

    /// Get the total number of recorded executions.
    pub fn execution_count(&self) -> usize {
        self.executions.len()
    }

    /// Clear all recorded executions.
    pub fn clear(&mut self) {
        self.executions.clear();
    }

    /// Validate all recorded executions against the configuration.
    ///
    /// # Returns
    /// A `ProcessValidationResult` containing validation status and details
    pub fn validate(&self) -> ProcessValidationResult {
        let mut issues = Vec::new();
        let mut required_found: HashMap<String, bool> = HashMap::new();
        let mut forbidden_used: HashMap<String, bool> = HashMap::new();

        // Initialize tracking maps
        for pattern in &self.config.required_patterns {
            required_found.insert(pattern.clone(), false);
        }
        for pattern in &self.config.forbidden_patterns {
            forbidden_used.insert(pattern.clone(), false);
        }

        // Check minimum commands
        if self.executions.len() < self.config.min_commands {
            issues.push(format!(
                "Insufficient commands: {} executed, {} required",
                self.executions.len(),
                self.config.min_commands
            ));
        }

        // Calculate total duration
        let total_duration: f64 = self.executions.iter().map(|e| e.duration).sum();

        // Check total duration limit
        if let Some(max_total) = self.config.max_total_duration {
            if total_duration > max_total {
                issues.push(format!(
                    "Total execution time {:.2}s exceeds limit {:.2}s",
                    total_duration, max_total
                ));
            }
        }

        // Check each execution
        for execution in &self.executions {
            // Check single command duration
            if let Some(max_single) = self.config.max_single_command_duration {
                if execution.duration > max_single {
                    issues.push(format!(
                        "Command '{}' took {:.2}s, exceeds limit {:.2}s",
                        truncate_command(&execution.command, 50),
                        execution.duration,
                        max_single
                    ));
                }
            }

            // Check success requirement
            if self.config.require_all_success && !execution.is_success() {
                issues.push(format!(
                    "Command '{}' failed with exit code {}",
                    truncate_command(&execution.command, 50),
                    execution.exit_code
                ));
            }

            // Check required patterns
            for pattern in &self.config.required_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    if re.is_match(&execution.command) {
                        required_found.insert(pattern.clone(), true);
                    }
                }
            }

            // Check forbidden patterns
            for pattern in &self.config.forbidden_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    if re.is_match(&execution.command) {
                        forbidden_used.insert(pattern.clone(), true);
                        issues.push(format!(
                            "Forbidden command pattern '{}' matched: '{}'",
                            pattern,
                            truncate_command(&execution.command, 50)
                        ));
                    }
                }
            }
        }

        // Check all required patterns were found
        for (pattern, found) in &required_found {
            if !found {
                issues.push(format!("Required command pattern '{}' not found", pattern));
            }
        }

        // Calculate validation score
        let score = calculate_validation_score(&required_found, &forbidden_used, &issues);

        // Determine overall validity
        let valid = issues.is_empty();

        ProcessValidationResult {
            valid,
            score,
            issues,
            required_found,
            forbidden_used,
        }
    }
}

/// Result of process validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessValidationResult {
    /// Whether all validation rules passed
    pub valid: bool,
    /// Validation score from 0.0 (complete failure) to 1.0 (perfect)
    pub score: f64,
    /// List of validation issues found
    pub issues: Vec<String>,
    /// Map of required patterns to whether they were found
    pub required_found: HashMap<String, bool>,
    /// Map of forbidden patterns to whether they were used
    pub forbidden_used: HashMap<String, bool>,
}

impl ProcessValidationResult {
    /// Create a validation result indicating complete success.
    pub fn success() -> Self {
        Self {
            valid: true,
            score: 1.0,
            issues: Vec::new(),
            required_found: HashMap::new(),
            forbidden_used: HashMap::new(),
        }
    }

    /// Create a validation result indicating failure with an issue.
    pub fn failure(issue: &str) -> Self {
        Self {
            valid: false,
            score: 0.0,
            issues: vec![issue.to_string()],
            required_found: HashMap::new(),
            forbidden_used: HashMap::new(),
        }
    }
}

/// Calculate validation score based on requirements met and issues found.
fn calculate_validation_score(
    required_found: &HashMap<String, bool>,
    forbidden_used: &HashMap<String, bool>,
    issues: &[String],
) -> f64 {
    let mut score = 1.0;

    // Deduct for missing required patterns
    if !required_found.is_empty() {
        let required_count = required_found.len() as f64;
        let found_count = required_found.values().filter(|&&v| v).count() as f64;
        let required_score = found_count / required_count;
        score *= required_score;
    }

    // Deduct heavily for forbidden patterns used
    let forbidden_count = forbidden_used.values().filter(|&&v| v).count();
    if forbidden_count > 0 {
        score *= 0.5_f64.powi(forbidden_count as i32);
    }

    // Deduct for other issues (diminishing impact)
    let other_issues = issues.len().saturating_sub(
        required_found.values().filter(|&&v| !v).count()
            + forbidden_used.values().filter(|&&v| v).count(),
    );
    if other_issues > 0 {
        score *= 0.9_f64.powi(other_issues as i32);
    }

    score.clamp(0.0, 1.0)
}

/// Truncate a command string for display purposes.
fn truncate_command(command: &str, max_len: usize) -> String {
    if command.len() <= max_len {
        command.to_string()
    } else {
        format!("{}...", &command[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_execution(command: &str, exit_code: i32, duration: f64) -> CommandExecution {
        CommandExecution::new(
            command.to_string(),
            1000.0,
            exit_code,
            String::new(),
            String::new(),
            duration,
        )
    }

    #[test]
    fn test_empty_tracer_validates() {
        let config = ProcessValidationConfig::default();
        let tracer = ProcessTracer::new(config);
        let result = tracer.validate();

        assert!(result.valid);
        assert_eq!(result.score, 1.0);
    }

    #[test]
    fn test_required_pattern_found() {
        let config = ProcessValidationConfig::new().with_required_pattern(r"^git\s+");

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("git status", 0, 0.5));
        tracer.record(create_test_execution("git commit -m 'test'", 0, 1.0));

        let result = tracer.validate();

        assert!(result.valid);
        assert!(*result.required_found.get(r"^git\s+").unwrap());
    }

    #[test]
    fn test_required_pattern_not_found() {
        let config = ProcessValidationConfig::new().with_required_pattern(r"^cargo\s+test");

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("cargo build", 0, 5.0));

        let result = tracer.validate();

        assert!(!result.valid);
        assert!(!(*result.required_found.get(r"^cargo\s+test").unwrap()));
        assert!(result.issues.iter().any(|i| i.contains("not found")));
    }

    #[test]
    fn test_forbidden_pattern_detected() {
        let config = ProcessValidationConfig::new().with_forbidden_pattern(r"rm\s+-rf");

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("rm -rf /tmp/test", 0, 0.1));

        let result = tracer.validate();

        assert!(!result.valid);
        assert!(*result.forbidden_used.get(r"rm\s+-rf").unwrap());
    }

    #[test]
    fn test_min_commands_validation() {
        let config = ProcessValidationConfig::new().with_min_commands(3);

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("ls", 0, 0.1));
        tracer.record(create_test_execution("pwd", 0, 0.1));

        let result = tracer.validate();

        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("Insufficient")));
    }

    #[test]
    fn test_require_all_success() {
        let config = ProcessValidationConfig::new().with_require_all_success();

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("ls", 0, 0.1));
        tracer.record(create_test_execution("false", 1, 0.1));

        let result = tracer.validate();

        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("failed")));
    }

    #[test]
    fn test_max_duration_exceeded() {
        let config = ProcessValidationConfig::new().with_max_single_duration(1.0);

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("slow_command", 0, 2.0));

        let result = tracer.validate();

        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("exceeds limit")));
    }

    #[test]
    fn test_score_calculation() {
        let config = ProcessValidationConfig::new()
            .with_required_pattern(r"pattern1")
            .with_required_pattern(r"pattern2")
            .with_required_pattern(r"pattern3");

        let mut tracer = ProcessTracer::new(config);
        tracer.record(create_test_execution("pattern1 command", 0, 0.1));

        let result = tracer.validate();

        // Only 1 of 3 required patterns found, so score should be ~0.33
        assert!(!result.valid);
        assert!(result.score > 0.3 && result.score < 0.4);
    }

    #[test]
    fn test_command_execution_is_success() {
        let success = create_test_execution("ls", 0, 0.1);
        let failure = create_test_execution("false", 1, 0.1);

        assert!(success.is_success());
        assert!(!failure.is_success());
    }

    #[test]
    fn test_truncate_command() {
        let short = "ls -la";
        let long = "cargo build --release --features all --target x86_64-unknown-linux-gnu";

        assert_eq!(truncate_command(short, 50), short);
        assert!(truncate_command(long, 20).ends_with("..."));
        assert!(truncate_command(long, 20).len() <= 20);
    }

    #[test]
    fn test_config_builder_pattern() {
        let config = ProcessValidationConfig::new()
            .with_required_pattern(r"^test")
            .with_forbidden_pattern(r"danger")
            .with_min_commands(5)
            .with_require_all_success()
            .with_max_single_duration(10.0)
            .with_max_total_duration(60.0);

        assert_eq!(config.required_patterns.len(), 1);
        assert_eq!(config.forbidden_patterns.len(), 1);
        assert_eq!(config.min_commands, 5);
        assert!(config.require_all_success);
        assert_eq!(config.max_single_command_duration, Some(10.0));
        assert_eq!(config.max_total_duration, Some(60.0));
    }
}
