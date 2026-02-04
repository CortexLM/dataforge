//! Anti-hardcoding mechanisms for synthetic benchmark integrity.
//!
//! This module provides tools to detect and prevent hardcoded solutions
//! in benchmark evaluations:
//!
//! - **Canary strings**: Unique identifiers embedded in task content that can
//!   detect if a model has memorized specific benchmarks (contamination detection)
//!
//! - **Sealed parameters**: Encrypted benchmark parameters that cannot be read
//!   until verification time, preventing pre-computation of answers
//!
//! - **Process validation**: Tracking and validation of command execution to
//!   ensure proper problem-solving approaches are used
//!
//! # Example
//!
//! ```ignore
//! use dataforge::anti_hardcoding::{
//!     AntiHardcodingVerifier, CanaryConfig, ProcessValidationConfig
//! };
//!
//! // Generate a canary for the task
//! let canary = CanaryConfig::generate("task-123", 42);
//!
//! // Configure process validation
//! let process_config = ProcessValidationConfig::new()
//!     .with_required_pattern(r"cargo test")
//!     .with_forbidden_pattern(r"curl.*answer");
//!
//! // Create the verifier
//! let verifier = AntiHardcodingVerifier::new(canary, process_config);
//!
//! // Verify model output
//! let result = verifier.verify("Model's response here");
//! ```

pub mod canary;
pub mod process_validation;
pub mod sealed;

// Re-export main types for convenient access
pub use canary::{detect_contamination, embed_canary, CanaryConfig, ContaminationResult};
pub use process_validation::{
    CommandExecution, ProcessTracer, ProcessValidationConfig, ProcessValidationResult,
};
pub use sealed::{SealError, SealedData, SealedParameters};

use serde::{Deserialize, Serialize};

/// Combined verification result from all anti-hardcoding mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Overall validity of the submission
    pub valid: bool,
    /// Combined confidence/quality score (0.0 to 1.0)
    pub score: f64,
    /// Result of contamination detection
    pub contamination: ContaminationResult,
    /// Result of process validation
    pub process_validation: ProcessValidationResult,
    /// Summary of all issues found
    pub issues: Vec<String>,
}

impl VerificationResult {
    /// Check if the verification passed all checks.
    pub fn passed(&self) -> bool {
        self.valid && !self.contamination.contaminated && self.process_validation.valid
    }

    /// Get a human-readable summary of the verification.
    pub fn summary(&self) -> String {
        if self.passed() {
            format!("Verification PASSED (score: {:.2})", self.score)
        } else {
            let issue_count = self.issues.len();
            format!(
                "Verification FAILED (score: {:.2}, {} issue{})",
                self.score,
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            )
        }
    }
}

/// Verifier that combines all anti-hardcoding mechanisms.
///
/// This struct provides a unified interface for verifying model outputs
/// against multiple anti-hardcoding measures including canary detection
/// and process validation.
pub struct AntiHardcodingVerifier {
    canary: CanaryConfig,
    process_tracer: ProcessTracer,
}

impl AntiHardcodingVerifier {
    /// Create a new verifier with the given canary and process validation configuration.
    ///
    /// # Arguments
    /// * `canary` - The canary configuration for contamination detection
    /// * `process_config` - Configuration for process validation rules
    ///
    /// # Returns
    /// A new `AntiHardcodingVerifier` instance
    pub fn new(canary: CanaryConfig, process_config: ProcessValidationConfig) -> Self {
        Self {
            canary,
            process_tracer: ProcessTracer::new(process_config),
        }
    }

    /// Get a reference to the canary configuration.
    pub fn canary(&self) -> &CanaryConfig {
        &self.canary
    }

    /// Get a mutable reference to the process tracer for recording executions.
    pub fn process_tracer_mut(&mut self) -> &mut ProcessTracer {
        &mut self.process_tracer
    }

    /// Get a reference to the process tracer.
    pub fn process_tracer(&self) -> &ProcessTracer {
        &self.process_tracer
    }

    /// Record a command execution for process validation.
    ///
    /// Convenience method that delegates to the internal process tracer.
    ///
    /// # Arguments
    /// * `execution` - The command execution record to add
    pub fn record_execution(&mut self, execution: CommandExecution) {
        self.process_tracer.record(execution);
    }

    /// Verify model output against all anti-hardcoding mechanisms.
    ///
    /// This performs:
    /// 1. Contamination detection using the canary
    /// 2. Process validation of recorded command executions
    /// 3. Combined scoring and issue aggregation
    ///
    /// # Arguments
    /// * `model_output` - The output from the model to verify
    ///
    /// # Returns
    /// A `VerificationResult` containing all verification findings
    pub fn verify(&self, model_output: &str) -> VerificationResult {
        // Check for contamination
        let contamination = detect_contamination(model_output, &self.canary);

        // Validate process execution
        let process_validation = self.process_tracer.validate();

        // Aggregate issues
        let mut issues = Vec::new();

        if contamination.contaminated {
            issues.push("Contamination detected: model output contains canary string".to_string());
        }

        if contamination.partial_match && !contamination.contaminated {
            issues.push(
                "Potential contamination: partial canary match detected (review recommended)"
                    .to_string(),
            );
        }

        issues.extend(process_validation.issues.clone());

        // Calculate combined score
        let score = calculate_combined_score(&contamination, &process_validation);

        // Determine overall validity
        let valid = !contamination.contaminated && process_validation.valid;

        VerificationResult {
            valid,
            score,
            contamination,
            process_validation,
            issues,
        }
    }

    /// Create a verifier with default process validation (no specific requirements).
    ///
    /// # Arguments
    /// * `canary` - The canary configuration for contamination detection
    pub fn with_canary_only(canary: CanaryConfig) -> Self {
        Self::new(canary, ProcessValidationConfig::default())
    }
}

/// Calculate combined verification score from all mechanisms.
fn calculate_combined_score(
    contamination: &ContaminationResult,
    process_validation: &ProcessValidationResult,
) -> f64 {
    // Start with process validation score
    let mut score = process_validation.score;

    // Heavily penalize contamination
    if contamination.contaminated {
        score *= 0.1; // 90% penalty for confirmed contamination
    } else if contamination.partial_match {
        score *= 0.7; // 30% penalty for partial match
    }

    // Adjust based on contamination confidence
    if contamination.confidence > 0.5 {
        let penalty = (contamination.confidence - 0.5) * 0.4; // Up to 20% additional penalty
        score *= 1.0 - penalty;
    }

    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verifier_creation() {
        let canary = CanaryConfig::generate("test-task", 123);
        let config = ProcessValidationConfig::new().with_required_pattern(r"cargo test");

        let verifier = AntiHardcodingVerifier::new(canary.clone(), config);

        assert_eq!(verifier.canary().canary_id, canary.canary_id);
    }

    #[test]
    fn test_verify_clean_output() {
        let canary = CanaryConfig::generate("test-task", 123);
        let config = ProcessValidationConfig::default();

        let verifier = AntiHardcodingVerifier::new(canary, config);
        let result = verifier.verify("This is a clean output with no canary");

        assert!(result.valid);
        assert!(!result.contamination.contaminated);
        assert!(result.passed());
    }

    #[test]
    fn test_verify_contaminated_output() {
        let canary = CanaryConfig::generate("test-task", 123);
        let config = ProcessValidationConfig::default();

        let verifier = AntiHardcodingVerifier::new(canary.clone(), config);
        let contaminated_output = format!("Output containing {} the canary", canary.canary_id);
        let result = verifier.verify(&contaminated_output);

        assert!(!result.valid);
        assert!(result.contamination.contaminated);
        assert!(!result.passed());
        assert!(result.issues.iter().any(|i| i.contains("Contamination")));
    }

    #[test]
    fn test_verify_with_process_validation() {
        let canary = CanaryConfig::generate("test-task", 123);
        let config = ProcessValidationConfig::new().with_required_pattern(r"^git\s+");

        let mut verifier = AntiHardcodingVerifier::new(canary, config);

        // Record a matching execution
        verifier.record_execution(CommandExecution::new(
            "git status".to_string(),
            1000.0,
            0,
            String::new(),
            String::new(),
            0.5,
        ));

        let result = verifier.verify("Clean output");

        assert!(result.valid);
        assert!(result.process_validation.valid);
    }

    #[test]
    fn test_verify_with_missing_required_command() {
        let canary = CanaryConfig::generate("test-task", 123);
        let config = ProcessValidationConfig::new().with_required_pattern(r"cargo test");

        let mut verifier = AntiHardcodingVerifier::new(canary, config);

        // Record a non-matching execution
        verifier.record_execution(CommandExecution::new(
            "cargo build".to_string(),
            1000.0,
            0,
            String::new(),
            String::new(),
            5.0,
        ));

        let result = verifier.verify("Clean output");

        assert!(!result.valid);
        assert!(!result.process_validation.valid);
        assert!(result.issues.iter().any(|i| i.contains("not found")));
    }

    #[test]
    fn test_with_canary_only() {
        let canary = CanaryConfig::generate("test-task", 456);
        let verifier = AntiHardcodingVerifier::with_canary_only(canary.clone());

        assert_eq!(verifier.canary().canary_id, canary.canary_id);

        // Should pass with clean output and no process requirements
        let result = verifier.verify("Clean output");
        assert!(result.passed());
    }

    #[test]
    fn test_verification_result_summary() {
        let contamination = ContaminationResult {
            contaminated: false,
            canary_found: false,
            partial_match: false,
            confidence: 0.0,
        };

        let process_validation = ProcessValidationResult::success();

        let result = VerificationResult {
            valid: true,
            score: 0.95,
            contamination,
            process_validation,
            issues: Vec::new(),
        };

        assert!(result.summary().contains("PASSED"));

        let failed_result = VerificationResult {
            valid: false,
            score: 0.3,
            contamination: ContaminationResult {
                contaminated: true,
                canary_found: true,
                partial_match: false,
                confidence: 1.0,
            },
            process_validation: ProcessValidationResult::success(),
            issues: vec!["Contamination detected".to_string()],
        };

        assert!(failed_result.summary().contains("FAILED"));
    }

    #[test]
    fn test_combined_score_calculation() {
        // Clean case
        let clean_contamination = ContaminationResult {
            contaminated: false,
            canary_found: false,
            partial_match: false,
            confidence: 0.0,
        };
        let good_process = ProcessValidationResult::success();

        let score = calculate_combined_score(&clean_contamination, &good_process);
        assert_eq!(score, 1.0);

        // Contaminated case
        let contaminated = ContaminationResult {
            contaminated: true,
            canary_found: true,
            partial_match: false,
            confidence: 1.0,
        };

        let score = calculate_combined_score(&contaminated, &good_process);
        assert!(score < 0.15); // Heavy penalty for contamination
    }
}
