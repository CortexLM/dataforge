//! Schema validation for task templates.
//!
//! This module validates task templates against the expected schema,
//! checking for required fields, valid formats, and consistent data.

use serde::{Deserialize, Serialize};

use crate::template::TaskTemplate;

/// Result of validating a task template against the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaValidationResult {
    /// Whether the template passes all required validations.
    pub valid: bool,
    /// Critical errors that must be fixed.
    pub errors: Vec<SchemaError>,
    /// Non-critical warnings that should be addressed.
    pub warnings: Vec<String>,
}

impl SchemaValidationResult {
    /// Create a new valid result with no errors or warnings.
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add an error to the result.
    pub fn add_error(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(SchemaError {
            field: field.into(),
            message: message.into(),
            severity: ErrorSeverity::Error,
        });
        self.valid = false;
    }

    /// Add a warning to the result.
    pub fn add_warning(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// A schema validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaError {
    /// Field that failed validation.
    pub field: String,
    /// Description of the validation error.
    pub message: String,
    /// Severity of the error.
    pub severity: ErrorSeverity,
}

/// Severity level for schema errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    /// Critical error that must be fixed.
    Error,
    /// Warning that should be addressed but doesn't block validation.
    Warning,
}

/// Validator for task template schemas.
pub struct SchemaValidator;

impl SchemaValidator {
    /// Validate a task template against the schema.
    ///
    /// Performs the following validations:
    /// - ID format (alphanumeric with hyphens and underscores)
    /// - Version format (semver X.Y.Z)
    /// - Difficulty score consistency
    /// - Required fields present
    /// - Variable placeholder usage
    ///
    /// # Arguments
    ///
    /// * `template` - The task template to validate
    ///
    /// # Returns
    ///
    /// A `SchemaValidationResult` containing any errors and warnings.
    pub fn validate_template(template: &TaskTemplate) -> SchemaValidationResult {
        let mut result = SchemaValidationResult::valid();

        // Validate ID format (use the existing template's validation logic)
        if !Self::is_valid_id(&template.id) {
            result.add_error(
                "id",
                "ID must be alphanumeric with hyphens and underscores (e.g., 'find-files-001')",
            );
        }

        // Validate version format
        if !is_valid_semver(&template.version) {
            result.add_error("version", "Version must be valid semver (X.Y.Z)");
        }

        // Validate difficulty scores
        if template.difficulty.min_score >= template.difficulty.max_score {
            result.add_error("difficulty", "min_score must be less than max_score");
        }

        if template.difficulty.min_score < 0.0 || template.difficulty.min_score > 1.0 {
            result.add_error(
                "difficulty.min_score",
                "min_score must be between 0.0 and 1.0",
            );
        }

        if template.difficulty.max_score < 0.0 || template.difficulty.max_score > 1.0 {
            result.add_error(
                "difficulty.max_score",
                "max_score must be between 0.0 and 1.0",
            );
        }

        // Validate difficulty label
        let valid_labels = ["easy", "medium", "hard"];
        if !valid_labels.contains(&template.difficulty.estimated.to_lowercase().as_str()) {
            result.add_warning(format!(
                "Difficulty level '{}' is non-standard. Expected one of: {:?}",
                template.difficulty.estimated, valid_labels
            ));
        }

        // Validate instruction template has variable placeholders
        if !template.instruction_template.contains("{{") {
            result.add_warning(
                "instruction_template has no variable placeholders - task may be static",
            );
        }

        // Validate category is not empty
        if template.category.trim().is_empty() {
            result.add_error("category", "Category cannot be empty");
        }

        // Validate subcategory is not empty
        if template.subcategory.trim().is_empty() {
            result.add_error("subcategory", "Subcategory cannot be empty");
        }

        // Check that instruction template references defined variables
        for name in template.variables.keys() {
            let placeholder = format!("{{{{{}}}}}", name);
            let placeholder_space = format!("{{{{ {} }}}}", name);
            if !template.instruction_template.contains(&placeholder)
                && !template.instruction_template.contains(&placeholder_space)
            {
                result.add_warning(format!(
                    "Variable '{}' is defined but may not be used in instruction_template",
                    name
                ));
            }
        }

        // Check for undefined variables in instruction template
        let template_vars = Self::extract_template_variables(&template.instruction_template);
        let defined_vars: std::collections::HashSet<&str> =
            template.variables.keys().map(|s| s.as_str()).collect();
        for var in template_vars {
            if !defined_vars.contains(var.as_str()) {
                result.add_error(
                    "instruction_template",
                    format!("Variable '{{{{{}}}}}' used but not defined", var),
                );
            }
        }

        // Validate generated files have unique paths
        let mut file_paths: Vec<&str> = template
            .generated_files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        file_paths.sort();
        for window in file_paths.windows(2) {
            if window[0] == window[1] {
                result.add_error(
                    "generated_files",
                    format!("Duplicate file path: '{}'", window[0]),
                );
            }
        }

        // Validate expected outputs have unique paths
        let mut output_paths: Vec<&str> = template
            .expected_outputs
            .keys()
            .map(|s| s.as_str())
            .collect();
        output_paths.sort();
        for window in output_paths.windows(2) {
            if window[0] == window[1] {
                result.add_error(
                    "expected_outputs",
                    format!("Duplicate output key: '{}'", window[0]),
                );
            }
        }

        result
    }

    /// Check if an ID string is valid (alphanumeric with hyphens and underscores).
    fn is_valid_id(id: &str) -> bool {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Extract variable names from a template string.
    fn extract_template_variables(template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' && chars.peek() == Some(&'{') {
                chars.next(); // consume second '{'
                let mut var_name = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        break;
                    }
                    var_name.push(chars.next().expect("char should exist"));
                }
                // Consume closing '}}'
                if chars.next() == Some('}') && chars.peek() == Some(&'}') {
                    chars.next();
                }
                let var_name = var_name.trim().to_string();
                if !var_name.is_empty() && !vars.contains(&var_name) {
                    vars.push(var_name);
                }
            }
        }

        vars
    }
}

/// Check if a version string is valid semver (X.Y.Z).
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{DifficultyConfig, VariableDefinition, VariableType};

    fn create_valid_template() -> TaskTemplate {
        TaskTemplate::new(
            "test-task-001",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Find the file named {{ filename }} in {{ directory }}",
            "find {{ directory }} -name '{{ filename }}'",
        )
        .with_variable(
            "filename",
            VariableDefinition::new(VariableType::String { pattern: None }),
        )
        .with_variable(
            "directory",
            VariableDefinition::new(VariableType::Path {
                base: "/home".to_string(),
            }),
        )
    }

    #[test]
    fn test_validate_valid_template() {
        let template = create_valid_template();
        let result = SchemaValidator::validate_template(&template);

        // Valid template may have warnings about undefined variables,
        // but should not have errors from basic structure checks
        assert!(
            result.valid,
            "Valid template should pass validation: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_invalid_id() {
        let mut template = create_valid_template();
        template.id = "Invalid ID!".to_string();

        let result = SchemaValidator::validate_template(&template);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.field == "id"));
    }

    #[test]
    fn test_validate_invalid_version() {
        let mut template = create_valid_template();
        template.version = "1.0".to_string();

        let result = SchemaValidator::validate_template(&template);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.field == "version"));
    }

    #[test]
    fn test_validate_invalid_difficulty_scores() {
        let mut template = create_valid_template();
        template.difficulty.min_score = 0.8;
        template.difficulty.max_score = 0.3;

        let result = SchemaValidator::validate_template(&template);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.field == "difficulty"));
    }

    #[test]
    fn test_is_valid_semver() {
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("0.1.0"));
        assert!(is_valid_semver("10.20.30"));
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("1.0.0.0"));
        assert!(!is_valid_semver("1.0.a"));
        assert!(!is_valid_semver(""));
    }

    #[test]
    fn test_is_valid_id() {
        assert!(SchemaValidator::is_valid_id("test-task-001"));
        assert!(SchemaValidator::is_valid_id("simple"));
        assert!(SchemaValidator::is_valid_id("a-b-c"));
        assert!(SchemaValidator::is_valid_id("with_underscore"));
        assert!(!SchemaValidator::is_valid_id(""));
        assert!(!SchemaValidator::is_valid_id("has space"));
        assert!(!SchemaValidator::is_valid_id("has!special"));
    }

    #[test]
    fn test_extract_template_variables() {
        let vars =
            SchemaValidator::extract_template_variables("Hello {{name}}, you have {{count}} items");
        assert_eq!(vars, vec!["name", "count"]);

        let vars = SchemaValidator::extract_template_variables("No variables here");
        assert!(vars.is_empty());

        let vars = SchemaValidator::extract_template_variables("{{a}} and {{b}} and {{a}} again");
        assert_eq!(vars, vec!["a", "b"]);

        // Test with spaces around variable names (Tera/Jinja2 style)
        let vars = SchemaValidator::extract_template_variables(
            "Hello {{ name }}, you have {{ count }} items",
        );
        assert_eq!(vars, vec!["name", "count"]);
    }
}
