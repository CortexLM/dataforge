//! Solution derivation for benchmark tasks.
//!
//! This module provides functionality for computing expected outputs
//! and verification data from templates and sampled parameters.

use crate::error::GeneratorError;
use crate::template::TaskTemplate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Context, Tera};

/// Represents a derived solution for a generated task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedSolution {
    /// The task ID this solution is for.
    pub task_id: String,
    /// The rendered solution command/script.
    pub solution_command: String,
    /// Expected output values keyed by output name.
    pub expected_outputs: HashMap<String, ExpectedOutputInfo>,
    /// Verification hints for the grader.
    pub verification_hints: Vec<String>,
}

/// Information about an expected output for test generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutputInfo {
    /// The path where the output should be written.
    pub path: String,
    /// The expected content (if known).
    pub content: Option<String>,
    /// Regex pattern the content should match.
    pub content_pattern: Option<String>,
    /// Whether the file should exist.
    pub exists: bool,
}

/// An expected value for output verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedValue {
    /// The expected value (can be any JSON type).
    pub value: serde_json::Value,
    /// Whether the value should be an exact match.
    pub exact_match: bool,
    /// Optional regex pattern for validation.
    pub pattern: Option<String>,
    /// Tolerance for numeric comparisons.
    pub tolerance: Option<f64>,
}

impl ExpectedValue {
    /// Create an expected value with exact match requirement.
    pub fn exact(value: serde_json::Value) -> Self {
        Self {
            value,
            exact_match: true,
            pattern: None,
            tolerance: None,
        }
    }

    /// Create an expected value with pattern matching.
    pub fn pattern(pattern: impl Into<String>) -> Self {
        Self {
            value: serde_json::Value::Null,
            exact_match: false,
            pattern: Some(pattern.into()),
            tolerance: None,
        }
    }

    /// Create an expected numeric value with tolerance.
    pub fn numeric_with_tolerance(value: f64, tolerance: f64) -> Self {
        Self {
            value: serde_json::json!(value),
            exact_match: false,
            pattern: None,
            tolerance: Some(tolerance),
        }
    }
}

/// Deriver for computing solutions from templates and parameters.
pub struct SolutionDeriver {
    template: TaskTemplate,
    params: HashMap<String, serde_json::Value>,
}

impl SolutionDeriver {
    /// Create a new solution deriver.
    ///
    /// # Arguments
    /// * `template` - The task template
    /// * `params` - The sampled parameters
    pub fn new(template: TaskTemplate, params: HashMap<String, serde_json::Value>) -> Self {
        Self { template, params }
    }

    /// Derive the solution for the task.
    ///
    /// This renders the solution template with the sampled parameters
    /// and computes expected output values.
    pub fn derive(&self) -> Result<DerivedSolution, GeneratorError> {
        // Create Tera context from parameters
        let mut context = Context::new();
        for (key, value) in &self.params {
            context.insert(key, value);
        }

        // Render the solution template
        let solution_command = Tera::one_off(&self.template.solution_template, &context, false)
            .map_err(GeneratorError::Tera)?;

        // Compute expected outputs from template
        let expected_outputs = self.compute_expected_outputs(&context)?;

        // Generate verification hints
        let verification_hints = self.generate_verification_hints();

        Ok(DerivedSolution {
            task_id: self.template.id.clone(),
            solution_command,
            expected_outputs,
            verification_hints,
        })
    }

    /// Compute expected outputs from the template's expected_outputs configuration.
    fn compute_expected_outputs(
        &self,
        context: &Context,
    ) -> Result<HashMap<String, ExpectedOutputInfo>, GeneratorError> {
        let mut outputs = HashMap::new();

        for (name, expected) in &self.template.expected_outputs {
            // Render content template if present
            let content = if let Some(ref content_template) = expected.content {
                let rendered = Tera::one_off(content_template, context, false)
                    .map_err(GeneratorError::Tera)?;
                Some(rendered)
            } else {
                None
            };

            outputs.insert(
                name.clone(),
                ExpectedOutputInfo {
                    path: expected.path.clone(),
                    content,
                    content_pattern: expected.content_pattern.clone(),
                    exists: expected.exists,
                },
            );
        }

        Ok(outputs)
    }

    /// Generate verification hints based on the template.
    fn generate_verification_hints(&self) -> Vec<String> {
        let mut hints = Vec::new();

        // Add hints based on expected outputs
        for (name, expected) in &self.template.expected_outputs {
            if expected.exists {
                hints.push(format!(
                    "Verify that output '{}' exists at '{}'",
                    name, expected.path
                ));
            }
            if let Some(ref pattern) = &expected.content_pattern {
                hints.push(format!(
                    "Output '{}' should match pattern: {}",
                    name, pattern
                ));
            }
        }

        // Add hints from anti-hardcoding config
        let anti_hc = &self.template.anti_hardcoding;
        if !anti_hc.canary_locations.is_empty() {
            hints.push("Task contains canary strings for contamination detection".to_string());
        }
        for pattern in &anti_hc.process_validation.required_patterns {
            hints.push(format!(
                "Solution must include command matching: {}",
                pattern
            ));
        }

        hints
    }

    /// Get the template reference.
    pub fn template(&self) -> &TaskTemplate {
        &self.template
    }

    /// Get the parameters reference.
    pub fn params(&self) -> &HashMap<String, serde_json::Value> {
        &self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::DifficultyConfig;

    fn create_test_template() -> TaskTemplate {
        TaskTemplate::new(
            "test-solution",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Find errors with code {{error_code}}",
            "grep '{{error_code}}' /var/log/app.log",
        )
    }

    #[test]
    fn test_derive_solution() {
        let template = create_test_template();
        let mut params = HashMap::new();
        params.insert("error_code".to_string(), serde_json::json!("500"));

        let deriver = SolutionDeriver::new(template, params);
        let solution = deriver.derive().expect("derivation should succeed");

        assert_eq!(solution.task_id, "test-solution");
        assert!(solution.solution_command.contains("500"));
        assert!(solution.solution_command.contains("grep"));
    }

    #[test]
    fn test_expected_value_exact() {
        let expected = ExpectedValue::exact(serde_json::json!("test"));
        assert!(expected.exact_match);
        assert_eq!(expected.value, serde_json::json!("test"));
    }

    #[test]
    fn test_expected_value_pattern() {
        let expected = ExpectedValue::pattern(r"\d+");
        assert!(!expected.exact_match);
        assert_eq!(expected.pattern, Some(r"\d+".to_string()));
    }

    #[test]
    fn test_expected_value_numeric_tolerance() {
        let expected = ExpectedValue::numeric_with_tolerance(3.125, 0.01);
        assert!(!expected.exact_match);
        assert_eq!(expected.tolerance, Some(0.01));
    }
}
