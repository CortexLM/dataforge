//! Template schema definitions for task templates.
//!
//! This module defines the structure of task templates including difficulty configuration,
//! file generation, expected outputs, and anti-hardcoding validation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::TemplateError;
use crate::template::variables::VariableDefinition;

/// Valid difficulty levels for tasks.
const VALID_DIFFICULTY_LEVELS: [&str; 3] = ["easy", "medium", "hard"];

/// Configuration for task difficulty scoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifficultyConfig {
    /// Human-readable difficulty level: "easy", "medium", or "hard".
    pub estimated: String,
    /// Minimum acceptable score for this difficulty (0.0-1.0).
    pub min_score: f64,
    /// Maximum score achievable for this difficulty (0.0-1.0).
    pub max_score: f64,
}

impl DifficultyConfig {
    /// Creates a new difficulty configuration.
    pub fn new(estimated: impl Into<String>, min_score: f64, max_score: f64) -> Self {
        Self {
            estimated: estimated.into(),
            min_score,
            max_score,
        }
    }

    /// Creates an "easy" difficulty configuration with typical score range.
    pub fn easy() -> Self {
        Self {
            estimated: "easy".to_string(),
            min_score: 0.8,
            max_score: 1.0,
        }
    }

    /// Creates a "medium" difficulty configuration with typical score range.
    pub fn medium() -> Self {
        Self {
            estimated: "medium".to_string(),
            min_score: 0.5,
            max_score: 0.9,
        }
    }

    /// Creates a "hard" difficulty configuration with typical score range.
    pub fn hard() -> Self {
        Self {
            estimated: "hard".to_string(),
            min_score: 0.0,
            max_score: 0.7,
        }
    }

    /// Validates the difficulty configuration.
    pub fn validate(&self, template_id: &str) -> Result<(), TemplateError> {
        // Validate difficulty level
        if !VALID_DIFFICULTY_LEVELS.contains(&self.estimated.as_str()) {
            return Err(TemplateError::InvalidDifficultyLevel(
                self.estimated.clone(),
            ));
        }

        // Validate score range
        if self.min_score > self.max_score {
            return Err(TemplateError::InvalidDifficultyRange {
                min: self.min_score,
                max: self.max_score,
            });
        }

        // Validate scores are in valid range
        if self.min_score < 0.0 || self.min_score > 1.0 {
            return Err(TemplateError::Validation(format!(
                "Template '{}': min_score must be between 0.0 and 1.0, got {}",
                template_id, self.min_score
            )));
        }
        if self.max_score < 0.0 || self.max_score > 1.0 {
            return Err(TemplateError::Validation(format!(
                "Template '{}': max_score must be between 0.0 and 1.0, got {}",
                template_id, self.max_score
            )));
        }

        Ok(())
    }
}

/// Configuration for generating a file as part of task setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedFileConfig {
    /// Output path for the generated file (relative to task directory).
    pub path: String,
    /// Name of the generator to use (e.g., "tera", "copy", "json", "yaml").
    pub generator: String,
    /// Generator-specific configuration parameters.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, serde_json::Value>,
}

impl GeneratedFileConfig {
    /// Creates a new generated file configuration.
    pub fn new(path: impl Into<String>, generator: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            generator: generator.into(),
            config: HashMap::new(),
        }
    }

    /// Adds a configuration parameter.
    pub fn with_config(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    /// Validates the generated file configuration.
    pub fn validate(&self, template_id: &str) -> Result<(), TemplateError> {
        if self.path.is_empty() {
            return Err(TemplateError::Validation(format!(
                "Template '{}': generated file path cannot be empty",
                template_id
            )));
        }
        if self.generator.is_empty() {
            return Err(TemplateError::Validation(format!(
                "Template '{}': generator name cannot be empty for file '{}'",
                template_id, self.path
            )));
        }
        Ok(())
    }
}

/// Expected output specification for task verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpectedOutput {
    /// Path to the expected output file (relative to task directory).
    pub path: String,
    /// Exact content expected (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Regex pattern the content should match (alternative to exact content).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_pattern: Option<String>,
    /// Whether the file should exist after task completion.
    #[serde(default = "default_true")]
    pub exists: bool,
}

/// Returns true for serde default.
fn default_true() -> bool {
    true
}

impl ExpectedOutput {
    /// Creates a new expected output requiring the file to exist.
    pub fn exists(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: None,
            content_pattern: None,
            exists: true,
        }
    }

    /// Creates a new expected output with exact content match.
    pub fn with_content(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: Some(content.into()),
            content_pattern: None,
            exists: true,
        }
    }

    /// Creates a new expected output with pattern match.
    pub fn with_pattern(path: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: None,
            content_pattern: Some(pattern.into()),
            exists: true,
        }
    }

    /// Creates an expected output specifying the file should not exist.
    pub fn not_exists(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: None,
            content_pattern: None,
            exists: false,
        }
    }

    /// Validates the expected output configuration.
    pub fn validate(&self, template_id: &str, output_name: &str) -> Result<(), TemplateError> {
        if self.path.is_empty() {
            return Err(TemplateError::Validation(format!(
                "Template '{}': expected output '{}' path cannot be empty",
                template_id, output_name
            )));
        }

        // Validate content_pattern is a valid regex if provided
        if let Some(pattern) = &self.content_pattern {
            regex::Regex::new(pattern).map_err(|e| {
                TemplateError::Validation(format!(
                    "Template '{}': invalid content_pattern for '{}': {}",
                    template_id, output_name, e
                ))
            })?;
        }

        // Both content and content_pattern shouldn't be set together
        if self.content.is_some() && self.content_pattern.is_some() {
            return Err(TemplateError::Validation(format!(
                "Template '{}': expected output '{}' cannot have both content and content_pattern",
                template_id, output_name
            )));
        }

        Ok(())
    }
}

/// Configuration for process-level validation of task solutions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcessValidationConfig {
    /// Patterns that must appear in the solution process (commands, file operations).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_patterns: Vec<String>,
    /// Patterns that must NOT appear in the solution (indicates hardcoding).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbidden_patterns: Vec<String>,
}

impl ProcessValidationConfig {
    /// Creates a new process validation configuration.
    pub fn new() -> Self {
        Self {
            required_patterns: Vec::new(),
            forbidden_patterns: Vec::new(),
        }
    }

    /// Adds a required pattern.
    pub fn require_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.required_patterns.push(pattern.into());
        self
    }

    /// Adds a forbidden pattern.
    pub fn forbid_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.forbidden_patterns.push(pattern.into());
        self
    }

    /// Validates the process validation configuration.
    pub fn validate(&self, template_id: &str) -> Result<(), TemplateError> {
        // Validate required patterns are valid regexes
        for pattern in &self.required_patterns {
            regex::Regex::new(pattern).map_err(|e| {
                TemplateError::Validation(format!(
                    "Template '{}': invalid required_pattern '{}': {}",
                    template_id, pattern, e
                ))
            })?;
        }

        // Validate forbidden patterns are valid regexes
        for pattern in &self.forbidden_patterns {
            regex::Regex::new(pattern).map_err(|e| {
                TemplateError::Validation(format!(
                    "Template '{}': invalid forbidden_pattern '{}': {}",
                    template_id, pattern, e
                ))
            })?;
        }

        Ok(())
    }
}

/// Configuration for anti-hardcoding detection.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AntiHardcodingConfig {
    /// Locations where canary values will be injected for hardcoding detection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canary_locations: Vec<String>,
    /// Process-level validation configuration.
    #[serde(default)]
    pub process_validation: ProcessValidationConfig,
}

impl AntiHardcodingConfig {
    /// Creates a new anti-hardcoding configuration.
    pub fn new() -> Self {
        Self {
            canary_locations: Vec::new(),
            process_validation: ProcessValidationConfig::new(),
        }
    }

    /// Adds a canary location.
    pub fn with_canary_location(mut self, location: impl Into<String>) -> Self {
        self.canary_locations.push(location.into());
        self
    }

    /// Sets the process validation configuration.
    pub fn with_process_validation(mut self, config: ProcessValidationConfig) -> Self {
        self.process_validation = config;
        self
    }

    /// Validates the anti-hardcoding configuration.
    pub fn validate(&self, template_id: &str) -> Result<(), TemplateError> {
        self.process_validation.validate(template_id)
    }
}

/// Complete task template definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskTemplate {
    /// Unique identifier for this template.
    pub id: String,
    /// Semantic version of this template.
    pub version: String,
    /// Primary category (e.g., "debugging", "security").
    pub category: String,
    /// Subcategory within the primary category.
    pub subcategory: String,
    /// Difficulty configuration.
    pub difficulty: DifficultyConfig,
    /// Variable definitions for template instantiation.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, VariableDefinition>,
    /// Template for task instructions (Tera/Jinja2 syntax).
    pub instruction_template: String,
    /// Template for the expected solution (for validation).
    pub solution_template: String,
    /// Files to generate as part of task setup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_files: Vec<GeneratedFileConfig>,
    /// Expected outputs for verification.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub expected_outputs: HashMap<String, ExpectedOutput>,
    /// Anti-hardcoding detection configuration.
    #[serde(default)]
    pub anti_hardcoding: AntiHardcodingConfig,
}

impl TaskTemplate {
    /// Creates a new task template with required fields.
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        category: impl Into<String>,
        subcategory: impl Into<String>,
        difficulty: DifficultyConfig,
        instruction_template: impl Into<String>,
        solution_template: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            category: category.into(),
            subcategory: subcategory.into(),
            difficulty,
            variables: HashMap::new(),
            instruction_template: instruction_template.into(),
            solution_template: solution_template.into(),
            generated_files: Vec::new(),
            expected_outputs: HashMap::new(),
            anti_hardcoding: AntiHardcodingConfig::new(),
        }
    }

    /// Adds a variable definition to the template.
    pub fn with_variable(mut self, name: impl Into<String>, def: VariableDefinition) -> Self {
        self.variables.insert(name.into(), def);
        self
    }

    /// Adds a generated file configuration.
    pub fn with_generated_file(mut self, config: GeneratedFileConfig) -> Self {
        self.generated_files.push(config);
        self
    }

    /// Adds an expected output specification.
    pub fn with_expected_output(mut self, name: impl Into<String>, output: ExpectedOutput) -> Self {
        self.expected_outputs.insert(name.into(), output);
        self
    }

    /// Sets the anti-hardcoding configuration.
    pub fn with_anti_hardcoding(mut self, config: AntiHardcodingConfig) -> Self {
        self.anti_hardcoding = config;
        self
    }

    /// Validates the entire template configuration.
    pub fn validate(&self) -> Result<(), TemplateError> {
        // Validate template ID
        self.validate_id()?;

        // Validate version
        self.validate_version()?;

        // Validate required fields are not empty
        self.validate_required_fields()?;

        // Validate difficulty
        self.difficulty.validate(&self.id)?;

        // Validate all variable definitions
        for (name, def) in &self.variables {
            def.validate(name)?;
        }

        // Validate generated files
        for file_config in &self.generated_files {
            file_config.validate(&self.id)?;
        }

        // Validate expected outputs
        for (name, output) in &self.expected_outputs {
            output.validate(&self.id, name)?;
        }

        // Validate anti-hardcoding configuration
        self.anti_hardcoding.validate(&self.id)?;

        Ok(())
    }

    /// Validates the template ID format.
    fn validate_id(&self) -> Result<(), TemplateError> {
        if self.id.is_empty() {
            return Err(TemplateError::InvalidTemplateId(self.id.clone()));
        }

        // ID must contain only alphanumeric characters, hyphens, and underscores
        let valid = self
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(TemplateError::InvalidTemplateId(self.id.clone()));
        }

        Ok(())
    }

    /// Validates the semantic version format.
    fn validate_version(&self) -> Result<(), TemplateError> {
        // Simple semver validation: X.Y.Z where X, Y, Z are non-negative integers
        let parts: Vec<&str> = self.version.split('.').collect();
        if parts.len() != 3 {
            return Err(TemplateError::InvalidVersion(self.version.clone()));
        }

        for part in parts {
            if part.parse::<u32>().is_err() {
                return Err(TemplateError::InvalidVersion(self.version.clone()));
            }
        }

        Ok(())
    }

    /// Validates that required fields are present and non-empty.
    fn validate_required_fields(&self) -> Result<(), TemplateError> {
        if self.category.is_empty() {
            return Err(TemplateError::MissingRequiredField {
                template: self.id.clone(),
                field: "category".to_string(),
            });
        }

        if self.subcategory.is_empty() {
            return Err(TemplateError::MissingRequiredField {
                template: self.id.clone(),
                field: "subcategory".to_string(),
            });
        }

        if self.instruction_template.is_empty() {
            return Err(TemplateError::MissingRequiredField {
                template: self.id.clone(),
                field: "instruction_template".to_string(),
            });
        }

        if self.solution_template.is_empty() {
            return Err(TemplateError::MissingRequiredField {
                template: self.id.clone(),
                field: "solution_template".to_string(),
            });
        }

        Ok(())
    }

    /// Returns all variable names defined in this template.
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.keys().map(|s| s.as_str()).collect()
    }

    /// Returns the variable definition for a given name.
    pub fn get_variable(&self, name: &str) -> Option<&VariableDefinition> {
        self.variables.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::variables::{Distribution, VariableType};

    #[test]
    fn test_difficulty_config_validation() {
        let config = DifficultyConfig::new("medium", 0.5, 0.9);
        assert!(config.validate("test-template").is_ok());
    }

    #[test]
    fn test_difficulty_config_invalid_level() {
        let config = DifficultyConfig::new("impossible", 0.0, 1.0);
        let result = config.validate("test-template");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::InvalidDifficultyLevel(_)
        ));
    }

    #[test]
    fn test_difficulty_config_invalid_range() {
        let config = DifficultyConfig::new("medium", 0.9, 0.5);
        let result = config.validate("test-template");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::InvalidDifficultyRange { .. }
        ));
    }

    #[test]
    fn test_generated_file_config() {
        let config = GeneratedFileConfig::new("config.yaml", "tera").with_config(
            "template".to_string(),
            serde_json::json!("config.yaml.tera"),
        );
        assert_eq!(config.path, "config.yaml");
        assert_eq!(config.generator, "tera");
        assert!(config.validate("test-template").is_ok());
    }

    #[test]
    fn test_expected_output_validation() {
        let output = ExpectedOutput::with_pattern("output.txt", r"\d+");
        assert!(output.validate("test-template", "result").is_ok());
    }

    #[test]
    fn test_expected_output_invalid_pattern() {
        let output = ExpectedOutput::with_pattern("output.txt", "[invalid");
        let result = output.validate("test-template", "result");
        assert!(result.is_err());
    }

    #[test]
    fn test_expected_output_both_content_and_pattern() {
        let mut output = ExpectedOutput::with_content("output.txt", "exact");
        output.content_pattern = Some(r"\d+".to_string());
        let result = output.validate("test-template", "result");
        assert!(result.is_err());
    }

    #[test]
    fn test_task_template_validation() {
        let template = TaskTemplate::new(
            "test-task-001",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Analyze the log file at {{ log_path }}",
            "grep 'ERROR' {{ log_path }}",
        )
        .with_variable(
            "log_path",
            VariableDefinition::new(VariableType::Path {
                base: "/var/log".to_string(),
            }),
        );

        assert!(template.validate().is_ok());
    }

    #[test]
    fn test_task_template_invalid_id() {
        let template = TaskTemplate::new(
            "invalid id!",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );
        let result = template.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::InvalidTemplateId(_)
        ));
    }

    #[test]
    fn test_task_template_invalid_version() {
        let template = TaskTemplate::new(
            "test-task",
            "not-a-version",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );
        let result = template.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::InvalidVersion(_)
        ));
    }

    #[test]
    fn test_task_template_missing_category() {
        let template = TaskTemplate::new(
            "test-task",
            "1.0.0",
            "",
            "log-analysis",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );
        let result = template.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::MissingRequiredField { field, .. } if field == "category"
        ));
    }

    #[test]
    fn test_process_validation_config() {
        let config = ProcessValidationConfig::new()
            .require_pattern(r"grep.*ERROR")
            .forbid_pattern(r"echo\s+'hardcoded'");
        assert!(config.validate("test-template").is_ok());
    }

    #[test]
    fn test_anti_hardcoding_config() {
        let config = AntiHardcodingConfig::new()
            .with_canary_location("config.yaml:password")
            .with_process_validation(
                ProcessValidationConfig::new()
                    .require_pattern(r"read.*config")
                    .forbid_pattern(r"secret123"),
            );
        assert!(config.validate("test-template").is_ok());
    }

    #[test]
    fn test_difficulty_presets() {
        let easy = DifficultyConfig::easy();
        assert_eq!(easy.estimated, "easy");
        assert!(easy.validate("test").is_ok());

        let medium = DifficultyConfig::medium();
        assert_eq!(medium.estimated, "medium");
        assert!(medium.validate("test").is_ok());

        let hard = DifficultyConfig::hard();
        assert_eq!(hard.estimated, "hard");
        assert!(hard.validate("test").is_ok());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let template = TaskTemplate::new(
            "test-task-001",
            "1.0.0",
            "debugging",
            "log-analysis",
            DifficultyConfig::medium(),
            "Analyze the log file at {{ log_path }}",
            "grep 'ERROR' {{ log_path }}",
        )
        .with_variable(
            "count",
            VariableDefinition::new(VariableType::Int {
                min: 1,
                max: 100,
                distribution: Distribution::Uniform,
            }),
        )
        .with_generated_file(GeneratedFileConfig::new("logs/app.log", "tera"))
        .with_expected_output("result", ExpectedOutput::exists("output.txt"));

        let yaml = serde_yaml::to_string(&template).expect("serialization should succeed");
        let parsed: TaskTemplate =
            serde_yaml::from_str(&yaml).expect("deserialization should succeed");

        assert_eq!(parsed.id, template.id);
        assert_eq!(parsed.version, template.version);
        assert_eq!(parsed.category, template.category);
        assert_eq!(parsed.variables.len(), template.variables.len());
    }
}
