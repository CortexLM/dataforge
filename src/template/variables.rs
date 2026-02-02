//! Variable types and definitions for template systems.
//!
//! This module defines the various types of variables that can be used in templates,
//! along with their validation rules and serialization support.

use serde::{Deserialize, Serialize};

use crate::error::TemplateError;

/// Distribution types for numeric variables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Distribution {
    /// Uniform distribution across the range.
    #[default]
    Uniform,
    /// Normal/Gaussian distribution centered at midpoint.
    Normal,
    /// Log-uniform distribution (uniform in log space).
    LogUniform,
}

/// Network type for IP address generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum NetworkType {
    /// Private network IP ranges (10.x.x.x, 172.16-31.x.x, 192.168.x.x).
    #[default]
    Private,
    /// Public IP addresses (excluding reserved ranges).
    Public,
}

/// Types of variables that can be defined in templates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum VariableType {
    /// String variable with optional regex pattern constraint.
    String {
        /// Optional regex pattern the generated string must match.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },

    /// Integer variable with range and optional distribution.
    Int {
        /// Minimum value (inclusive).
        min: i64,
        /// Maximum value (inclusive).
        max: i64,
        /// Distribution for random generation.
        #[serde(default, skip_serializing_if = "is_uniform")]
        distribution: Distribution,
    },

    /// Floating-point variable with range and optional distribution.
    Float {
        /// Minimum value (inclusive).
        min: f64,
        /// Maximum value (inclusive).
        max: f64,
        /// Distribution for random generation.
        #[serde(default, skip_serializing_if = "is_uniform")]
        distribution: Distribution,
    },

    /// Choice from a list of options with optional weights.
    Choice {
        /// Available choices.
        choices: Vec<String>,
        /// Optional weights for each choice (must match choices length if provided).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f64>,
    },

    /// UUID variable with optional prefix.
    Uuid {
        /// Optional prefix to prepend to the UUID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
    },

    /// IP address variable with network type specification.
    Ip {
        /// Network type (private or public).
        #[serde(default)]
        network: NetworkType,
    },

    /// Network port variable, avoiding common well-known ports.
    Port {
        /// Ports to explicitly exclude from generation.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude: Vec<u16>,
    },

    /// File path variable with base directory.
    Path {
        /// Base directory for the path.
        base: String,
    },

    /// Username variable for generating realistic usernames.
    Username,

    /// Timestamp variable with format specification.
    Timestamp {
        /// Format string for the timestamp (strftime-compatible).
        format: String,
    },

    /// Service name variable for generating realistic service identifiers.
    ServiceName,
}

/// Helper function for serde to skip serializing default uniform distribution.
fn is_uniform(dist: &Distribution) -> bool {
    matches!(dist, Distribution::Uniform)
}

impl VariableType {
    /// Validates the variable type configuration.
    ///
    /// Returns an error if the configuration is invalid (e.g., min > max for ranges).
    pub fn validate(&self, name: &str) -> Result<(), TemplateError> {
        match self {
            VariableType::String { pattern } => {
                if let Some(p) = pattern {
                    // Validate the regex pattern compiles
                    regex::Regex::new(p).map_err(|e| TemplateError::InvalidRegexPattern {
                        variable: name.to_string(),
                        pattern: p.clone(),
                        message: e.to_string(),
                    })?;
                }
                Ok(())
            }
            VariableType::Int { min, max, .. } => {
                if min > max {
                    return Err(TemplateError::InvalidRange {
                        variable: name.to_string(),
                        min: min.to_string(),
                        max: max.to_string(),
                    });
                }
                Ok(())
            }
            VariableType::Float { min, max, .. } => {
                if min > max {
                    return Err(TemplateError::InvalidRange {
                        variable: name.to_string(),
                        min: min.to_string(),
                        max: max.to_string(),
                    });
                }
                Ok(())
            }
            VariableType::Choice { choices, weights } => {
                if choices.is_empty() {
                    return Err(TemplateError::EmptyChoices(name.to_string()));
                }
                if !weights.is_empty() && weights.len() != choices.len() {
                    return Err(TemplateError::WeightsMismatch {
                        variable: name.to_string(),
                        weights: weights.len(),
                        choices: choices.len(),
                    });
                }
                for weight in weights {
                    if *weight < 0.0 {
                        return Err(TemplateError::NegativeWeight(name.to_string()));
                    }
                }
                Ok(())
            }
            VariableType::Uuid { .. } => Ok(()),
            VariableType::Ip { .. } => Ok(()),
            VariableType::Port { .. } => Ok(()),
            VariableType::Path { base } => {
                if base.is_empty() {
                    return Err(TemplateError::InvalidVariableDefinition {
                        variable: name.to_string(),
                        message: "Path base directory cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            VariableType::Username => Ok(()),
            VariableType::Timestamp { format } => {
                if format.is_empty() {
                    return Err(TemplateError::InvalidVariableDefinition {
                        variable: name.to_string(),
                        message: "Timestamp format cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            VariableType::ServiceName => Ok(()),
        }
    }
}

/// Definition of a template variable with type and metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDefinition {
    /// The type and configuration of this variable.
    #[serde(flatten)]
    pub var_type: VariableType,

    /// Human-readable description of this variable's purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this variable must be provided or can be omitted.
    #[serde(default = "default_required")]
    pub required: bool,
}

/// Default value for `required` field (true).
fn default_required() -> bool {
    true
}

impl VariableDefinition {
    /// Creates a new required variable definition.
    pub fn new(var_type: VariableType) -> Self {
        Self {
            var_type,
            description: None,
            required: true,
        }
    }

    /// Creates a new variable definition with the specified required status.
    pub fn with_required(var_type: VariableType, required: bool) -> Self {
        Self {
            var_type,
            description: None,
            required,
        }
    }

    /// Adds a description to this variable definition.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Validates this variable definition.
    pub fn validate(&self, name: &str) -> Result<(), TemplateError> {
        self.var_type.validate(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_variable_serialization() {
        let var = VariableType::String {
            pattern: Some(r"^\d{4}-\d{2}-\d{2}$".to_string()),
        };
        let json = serde_json::to_string(&var).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"string\""));
        assert!(json.contains("pattern"));
    }

    #[test]
    fn test_int_variable_validation() {
        let var = VariableType::Int {
            min: 10,
            max: 5,
            distribution: Distribution::Uniform,
        };
        let result = var.validate("test_var");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TemplateError::InvalidRange { .. }));
    }

    #[test]
    fn test_choice_variable_validation() {
        let var = VariableType::Choice {
            choices: vec!["a".to_string(), "b".to_string()],
            weights: vec![0.5], // Mismatch!
        };
        let result = var.validate("test_var");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TemplateError::WeightsMismatch { .. }));
    }

    #[test]
    fn test_choice_empty_validation() {
        let var = VariableType::Choice {
            choices: vec![],
            weights: vec![],
        };
        let result = var.validate("test_var");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TemplateError::EmptyChoices(_)));
    }

    #[test]
    fn test_invalid_regex_pattern() {
        let var = VariableType::String {
            pattern: Some("[invalid".to_string()),
        };
        let result = var.validate("test_var");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TemplateError::InvalidRegexPattern { .. }));
    }

    #[test]
    fn test_variable_definition_serialization() {
        let def = VariableDefinition::new(VariableType::Int {
            min: 1,
            max: 100,
            distribution: Distribution::Uniform,
        })
        .with_description("A count between 1 and 100");

        let yaml = serde_yaml::to_string(&def).expect("serialization should succeed");
        assert!(yaml.contains("type: int"));
        assert!(yaml.contains("min: 1"));
        assert!(yaml.contains("max: 100"));
        assert!(yaml.contains("description:"));
    }

    #[test]
    fn test_uuid_variable() {
        let var = VariableType::Uuid {
            prefix: Some("task-".to_string()),
        };
        assert!(var.validate("uuid_var").is_ok());
    }

    #[test]
    fn test_ip_variable() {
        let var = VariableType::Ip {
            network: NetworkType::Private,
        };
        assert!(var.validate("ip_var").is_ok());
    }

    #[test]
    fn test_port_variable() {
        let var = VariableType::Port {
            exclude: vec![22, 80, 443],
        };
        assert!(var.validate("port_var").is_ok());
    }

    #[test]
    fn test_path_variable_empty_base() {
        let var = VariableType::Path {
            base: String::new(),
        };
        let result = var.validate("path_var");
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_variable_empty_format() {
        let var = VariableType::Timestamp {
            format: String::new(),
        };
        let result = var.validate("ts_var");
        assert!(result.is_err());
    }

    #[test]
    fn test_distribution_default() {
        let dist = Distribution::default();
        assert_eq!(dist, Distribution::Uniform);
    }

    #[test]
    fn test_network_type_default() {
        let net = NetworkType::default();
        assert_eq!(net, NetworkType::Private);
    }

    #[test]
    fn test_negative_weight_validation() {
        let var = VariableType::Choice {
            choices: vec!["a".to_string(), "b".to_string()],
            weights: vec![0.5, -0.5],
        };
        let result = var.validate("test_var");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TemplateError::NegativeWeight(_)));
    }
}
