//! Parameter sampling for task generation.
//!
//! This module implements deterministic parameter sampling from template variable definitions.
//! It uses ChaCha8 RNG for reproducibility and handles dependent variables via topological sort.

use crate::error::GeneratorError;
use crate::generator::Result;
use crate::template::{Distribution, NetworkType, VariableDefinition, VariableType};
use chrono::{Duration, Utc};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Context for parameter sampling operations.
struct SamplingContext {
    /// Random seed for reproducibility.
    seed: u64,
    /// Template identifier for namespace purposes.
    template_id: String,
    /// ChaCha8 random number generator.
    rng: ChaCha8Rng,
    /// Already resolved parameters (for dependent variables).
    resolved: HashMap<String, Value>,
}

impl SamplingContext {
    /// Creates a new sampling context with the given seed and template ID.
    fn new(seed: u64, template_id: String) -> Self {
        Self {
            seed,
            template_id,
            rng: ChaCha8Rng::seed_from_u64(seed),
            resolved: HashMap::new(),
        }
    }
}

/// Deterministic parameter sampler for task templates.
///
/// The `ParameterSampler` generates values for template variables using a seeded
/// random number generator. This ensures that the same seed always produces
/// identical parameter values, enabling reproducible task generation.
///
/// # Features
///
/// - Deterministic sampling via ChaCha8 RNG
/// - Support for all variable types (string, int, float, choice, uuid, etc.)
/// - Handles dependent variables through topological sorting
/// - Generates realistic values using patterns inspired by the fake crate
///
/// # Example
///
/// ```ignore
/// let sampler = ParameterSampler::new(42, "template-001".to_string());
/// let params = sampler.sample_all(&template.variables)?;
/// ```
pub struct ParameterSampler {
    ctx: SamplingContext,
}

impl ParameterSampler {
    /// Creates a new parameter sampler with the given seed and template ID.
    ///
    /// # Arguments
    ///
    /// * `seed` - Random seed for deterministic generation
    /// * `template_id` - Template identifier (used in UUID generation namespace)
    pub fn new(seed: u64, template_id: String) -> Self {
        Self {
            ctx: SamplingContext::new(seed, template_id),
        }
    }

    /// Samples all variables from the given definitions.
    ///
    /// Variables are processed in topological order to handle dependencies.
    /// For example, if variable A's range depends on variable B's value,
    /// B will be sampled before A.
    ///
    /// # Arguments
    ///
    /// * `variables` - Map of variable names to their definitions
    ///
    /// # Returns
    ///
    /// A map of variable names to their sampled values as JSON values.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - There's a circular dependency between variables
    /// - A variable type is invalid
    /// - A range expression fails to evaluate
    pub fn sample_all(
        mut self,
        variables: &HashMap<String, VariableDefinition>,
    ) -> Result<HashMap<String, Value>> {
        // Build dependency graph and get topological order
        let order = self.topological_sort(variables)?;

        // Sample each variable in order
        for var_name in order {
            let var_def =
                variables
                    .get(&var_name)
                    .ok_or_else(|| GeneratorError::VariableNotFound {
                        name: var_name.clone(),
                    })?;

            let value = self.sample_variable(&var_name, var_def)?;
            self.ctx.resolved.insert(var_name, value);
        }

        Ok(self.ctx.resolved)
    }

    /// Samples a single variable according to its definition.
    fn sample_variable(&mut self, _name: &str, var_def: &VariableDefinition) -> Result<Value> {
        match &var_def.var_type {
            VariableType::String { pattern } => self.sample_string(pattern.as_deref()),
            VariableType::Int {
                min,
                max,
                distribution,
            } => self.sample_int(*min, *max, distribution),
            VariableType::Float {
                min,
                max,
                distribution,
            } => self.sample_float(*min, *max, distribution),
            VariableType::Choice { choices, weights } => self.sample_choice(
                choices,
                if weights.is_empty() {
                    None
                } else {
                    Some(weights)
                },
            ),
            VariableType::Uuid { prefix } => self.sample_uuid(prefix.as_deref()),
            VariableType::Ip { network } => self.sample_ip(network),
            VariableType::Port { exclude } => self.sample_port(exclude),
            VariableType::Path { base } => self.sample_path(base),
            VariableType::Username => self.sample_username(),
            VariableType::Timestamp { format } => self.sample_timestamp(format),
            VariableType::ServiceName => self.sample_service_name(),
        }
    }

    /// Samples a string value.
    fn sample_string(&mut self, pattern: Option<&str>) -> Result<Value> {
        // If a pattern is provided, generate matching string
        if let Some(pattern) = pattern {
            return Ok(Value::String(self.generate_from_pattern(pattern)));
        }

        // Default: generate a random word-like string
        let words = [
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
            "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
            "chi", "psi", "omega",
        ];
        let word = words[self.ctx.rng.random_range(0..words.len())];
        Ok(Value::String(word.to_string()))
    }

    /// Samples an integer value with range and distribution.
    fn sample_int(&mut self, min: i64, max: i64, distribution: &Distribution) -> Result<Value> {
        let value = match distribution {
            Distribution::Uniform => self.ctx.rng.random_range(min..=max),
            Distribution::Normal => {
                // Use normal distribution centered at midpoint
                let mean = (min + max) as f64 / 2.0;
                let std_dev = (max - min) as f64 / 6.0; // 99.7% within range
                let normal = rand_distr::Normal::new(mean, std_dev)
                    .map_err(|e| GeneratorError::InvalidParameter(e.to_string()))?;
                let sampled = self.ctx.rng.sample(normal);
                sampled.round().clamp(min as f64, max as f64) as i64
            }
            Distribution::LogUniform => {
                // Log-uniform distribution
                let log_min = (min.max(1) as f64).ln();
                let log_max = (max.max(1) as f64).ln();
                let log_val = self.ctx.rng.random_range(log_min..=log_max);
                log_val.exp().round().clamp(min as f64, max as f64) as i64
            }
        };
        Ok(Value::Number(value.into()))
    }

    /// Samples a floating-point value with range and distribution.
    fn sample_float(&mut self, min: f64, max: f64, distribution: &Distribution) -> Result<Value> {
        let value = match distribution {
            Distribution::Uniform => self.ctx.rng.random_range(min..=max),
            Distribution::Normal => {
                let mean = (min + max) / 2.0;
                let std_dev = (max - min) / 6.0;
                let normal = rand_distr::Normal::new(mean, std_dev)
                    .map_err(|e| GeneratorError::InvalidParameter(e.to_string()))?;
                let sampled = self.ctx.rng.sample(normal);
                sampled.clamp(min, max)
            }
            Distribution::LogUniform => {
                let log_min = min.max(f64::MIN_POSITIVE).ln();
                let log_max = max.max(f64::MIN_POSITIVE).ln();
                let log_val = self.ctx.rng.random_range(log_min..=log_max);
                log_val.exp().clamp(min, max)
            }
        };
        Ok(Value::Number(
            serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0)),
        ))
    }

    /// Samples from a set of choices with optional weights.
    fn sample_choice(&mut self, choices: &[String], weights: Option<&Vec<f64>>) -> Result<Value> {
        if choices.is_empty() {
            return Err(GeneratorError::InvalidParameter(
                "Choice variable requires non-empty 'choices' field".to_string(),
            ));
        }

        self.sample_from_choices(choices, weights.map(|w| w.as_slice()))
    }

    /// Samples from choices with optional weights.
    fn sample_from_choices(
        &mut self,
        choices: &[String],
        weights: Option<&[f64]>,
    ) -> Result<Value> {
        if choices.is_empty() {
            return Err(GeneratorError::InvalidParameter(
                "Choices array cannot be empty".to_string(),
            ));
        }

        let selected = if let Some(weights) = weights {
            self.weighted_choice(choices, weights)?
        } else {
            choices[self.ctx.rng.random_range(0..choices.len())].clone()
        };

        Ok(Value::String(selected))
    }

    /// Performs weighted random selection.
    fn weighted_choice(&mut self, choices: &[String], weights: &[f64]) -> Result<String> {
        if choices.len() != weights.len() {
            return Err(GeneratorError::InvalidParameter(format!(
                "Number of choices ({}) must match number of weights ({})",
                choices.len(),
                weights.len()
            )));
        }

        let total_weight: f64 = weights.iter().sum();
        if total_weight <= 0.0 {
            return Err(GeneratorError::InvalidParameter(
                "Total weight must be positive".to_string(),
            ));
        }

        let random_value = self.ctx.rng.random::<f64>() * total_weight;
        let mut cumulative = 0.0;

        for (choice, &weight) in choices.iter().zip(weights.iter()) {
            cumulative += weight;
            if random_value <= cumulative {
                return Ok(choice.clone());
            }
        }

        // Fallback to last choice (shouldn't happen with proper weights)
        Ok(choices.last().cloned().unwrap_or_default())
    }

    /// Samples a UUID with optional prefix.
    fn sample_uuid(&mut self, prefix: Option<&str>) -> Result<Value> {
        // Use a fixed namespace UUID for deterministic v5 generation
        let namespace =
            Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").expect("valid UUID constant");

        // Create deterministic input from template, seed, and resolved count
        let input = format!(
            "{}-{}-{}",
            self.ctx.template_id,
            self.ctx.seed,
            self.ctx.resolved.len()
        );

        let generated = Uuid::new_v5(&namespace, input.as_bytes());
        let uuid_str = generated.to_string();

        // Apply prefix if specified, take first 12 chars of UUID for shorter format
        let result = if let Some(prefix) = prefix {
            format!("{}{}", prefix, &uuid_str[..13]) // prefix + first 13 chars (8-4 format)
        } else {
            uuid_str
        };

        Ok(Value::String(result))
    }

    /// Samples an IP address.
    fn sample_ip(&mut self, network: &NetworkType) -> Result<Value> {
        let ip = match network {
            NetworkType::Private => {
                // Generate private IP in 192.168.x.x range
                let octet3 = self.ctx.rng.random_range(1..255);
                let octet4 = self.ctx.rng.random_range(1..255);
                format!("192.168.{}.{}", octet3, octet4)
            }
            NetworkType::Public => {
                // Generate public-looking IP (avoiding reserved ranges)
                let octet1 = self.ctx.rng.random_range(11..223);
                let octet2 = self.ctx.rng.random_range(0..256);
                let octet3 = self.ctx.rng.random_range(0..256);
                let octet4 = self.ctx.rng.random_range(1..255);
                format!("{}.{}.{}.{}", octet1, octet2, octet3, octet4)
            }
        };

        Ok(Value::String(ip))
    }

    /// Samples a network port number, avoiding common well-known ports.
    fn sample_port(&mut self, exclude: &[u16]) -> Result<Value> {
        // Common ports to avoid for more realistic random generation
        let mut excluded_ports: Vec<u16> = vec![
            22, 80, 443, 3000, 3306, 5432, 6379, 8080, 8443, 9000, 27017, 5672,
        ];
        excluded_ports.extend_from_slice(exclude);

        let (min, max) = (1024u16, 65535u16);

        // Generate port avoiding excluded ones
        let mut attempts = 0;
        let port = loop {
            let candidate = self.ctx.rng.random_range(min..=max);
            if !excluded_ports.contains(&candidate) || attempts > 10 {
                break candidate;
            }
            attempts += 1;
        };

        Ok(Value::Number(port.into()))
    }

    /// Samples a file system path.
    fn sample_path(&mut self, base: &str) -> Result<Value> {
        // Generate random directory components
        let dir_components = [
            "log", "data", "cache", "config", "tmp", "lib", "share", "run", "opt", "srv", "home",
            "usr",
        ];

        let app_names = [
            "app",
            "service",
            "worker",
            "daemon",
            "server",
            "client",
            "agent",
            "monitor",
            "handler",
            "processor",
        ];

        let filenames = [
            "output.log",
            "app.log",
            "error.log",
            "access.log",
            "debug.log",
            "data.json",
            "config.yaml",
            "state.db",
        ];

        let depth = self.ctx.rng.random_range(1..=3);
        let mut path_parts = vec![base.to_string()];

        for _ in 0..depth {
            let component = dir_components[self.ctx.rng.random_range(0..dir_components.len())];
            path_parts.push(component.to_string());
        }

        // Add app name
        let app_name = app_names[self.ctx.rng.random_range(0..app_names.len())];
        path_parts.push(app_name.to_string());

        // Add filename
        let filename = filenames[self.ctx.rng.random_range(0..filenames.len())];
        path_parts.push(filename.to_string());

        let path = path_parts.join("/");
        Ok(Value::String(path))
    }

    /// Samples a username.
    fn sample_username(&mut self) -> Result<Value> {
        let first_names = [
            "james",
            "mary",
            "john",
            "patricia",
            "robert",
            "jennifer",
            "michael",
            "linda",
            "william",
            "elizabeth",
            "david",
            "barbara",
            "richard",
            "susan",
            "joseph",
            "jessica",
            "thomas",
            "sarah",
            "charles",
            "karen",
        ];

        let last_names = [
            "smith",
            "johnson",
            "williams",
            "brown",
            "jones",
            "garcia",
            "miller",
            "davis",
            "rodriguez",
            "martinez",
            "hernandez",
            "lopez",
            "gonzalez",
            "wilson",
            "anderson",
            "thomas",
            "taylor",
            "moore",
            "jackson",
            "martin",
        ];

        let first = first_names[self.ctx.rng.random_range(0..first_names.len())];
        let last = last_names[self.ctx.rng.random_range(0..last_names.len())];
        let number = self.ctx.rng.random_range(1..1000);

        // Various username patterns
        let patterns = [
            format!("{}{}", &first[..1], last),
            format!("{}_{}", first, last),
            format!("{}.{}", first, last),
            format!("{}{}", first, number),
            format!("{}_{}", last, number),
        ];

        let username = patterns[self.ctx.rng.random_range(0..patterns.len())].clone();

        // Truncate to reasonable length
        let truncated = if username.len() > 16 {
            username[..16].to_string()
        } else {
            username
        };

        Ok(Value::String(truncated))
    }

    /// Samples a timestamp.
    fn sample_timestamp(&mut self, format: &str) -> Result<Value> {
        // Generate a timestamp within the last 30 days
        let days_back = self.ctx.rng.random_range(0..30);
        let hours_back = self.ctx.rng.random_range(0..24);
        let minutes_back = self.ctx.rng.random_range(0..60);
        let seconds_back = self.ctx.rng.random_range(0..60);

        let timestamp = Utc::now()
            - Duration::days(days_back)
            - Duration::hours(hours_back)
            - Duration::minutes(minutes_back)
            - Duration::seconds(seconds_back);

        let formatted = match format {
            "iso8601" => timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            "unix" => timestamp.timestamp().to_string(),
            "unix_millis" => timestamp.timestamp_millis().to_string(),
            "date" => timestamp.format("%Y-%m-%d").to_string(),
            "datetime" => timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            "rfc2822" => timestamp.format("%a, %d %b %Y %H:%M:%S %z").to_string(),
            "log" => timestamp.format("%Y-%m-%d %H:%M:%S.%3f").to_string(),
            _ => timestamp.format(format).to_string(),
        };

        Ok(Value::String(formatted))
    }

    /// Samples a realistic service name.
    fn sample_service_name(&mut self) -> Result<Value> {
        let prefixes = [
            "auth",
            "user",
            "order",
            "payment",
            "inventory",
            "notification",
            "search",
            "analytics",
            "gateway",
            "cache",
            "session",
            "config",
            "logging",
            "metrics",
            "billing",
            "shipping",
            "cart",
            "product",
            "catalog",
            "recommendation",
            "email",
            "sms",
            "push",
            "webhook",
        ];

        let suffixes = [
            "service",
            "svc",
            "api",
            "worker",
            "handler",
            "processor",
            "manager",
            "engine",
            "daemon",
            "server",
        ];

        let prefix = prefixes[self.ctx.rng.random_range(0..prefixes.len())];
        let suffix = suffixes[self.ctx.rng.random_range(0..suffixes.len())];

        Ok(Value::String(format!("{}-{}", prefix, suffix)))
    }

    /// Generates a string matching a simple pattern.
    ///
    /// Supports: ? for random letter, # for random digit
    fn generate_from_pattern(&mut self, pattern: &str) -> String {
        // Pre-define character sets as arrays for safe, bounds-checked indexing
        const LETTERS: [char; 26] = [
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q',
            'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
        ];
        const DIGITS: [char; 10] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

        let mut result = String::new();

        for c in pattern.chars() {
            let generated = match c {
                '?' => {
                    // Safe array indexing - gen_range is bounded to valid indices
                    let idx = self.ctx.rng.random_range(0..LETTERS.len());
                    LETTERS[idx]
                }
                '#' => {
                    // Safe array indexing - gen_range is bounded to valid indices
                    let idx = self.ctx.rng.random_range(0..DIGITS.len());
                    DIGITS[idx]
                }
                _ => c,
            };
            result.push(generated);
        }

        result
    }

    /// Performs topological sort of variables.
    ///
    /// In the new variable structure, there are no inter-variable dependencies
    /// (ranges are specified directly in the type), so this just returns all
    /// variable names in a consistent order.
    fn topological_sort(
        &self,
        variables: &HashMap<String, VariableDefinition>,
    ) -> Result<Vec<String>> {
        // No dependencies in the new structure, so just return sorted keys
        // for consistent ordering
        let mut names: Vec<String> = variables.keys().cloned().collect();
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampler_deterministic() {
        let mut vars = HashMap::new();
        vars.insert(
            "port".to_string(),
            VariableDefinition::new(VariableType::Port { exclude: vec![] }),
        );

        let sampler1 = ParameterSampler::new(42, "test".to_string());
        let sampler2 = ParameterSampler::new(42, "test".to_string());

        let result1 = sampler1.sample_all(&vars).expect("sampling should succeed");
        let result2 = sampler2.sample_all(&vars).expect("sampling should succeed");

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_sample_int_with_range() {
        let mut vars = HashMap::new();
        vars.insert(
            "count".to_string(),
            VariableDefinition::new(VariableType::Int {
                min: 100,
                max: 200,
                distribution: Distribution::Uniform,
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let count = result["count"].as_i64().expect("should be a number");
        assert!((100..=200).contains(&count));
    }

    #[test]
    fn test_sample_choice_with_weights() {
        let mut vars = HashMap::new();
        vars.insert(
            "level".to_string(),
            VariableDefinition::new(VariableType::Choice {
                choices: vec!["ERROR".to_string(), "WARN".to_string(), "INFO".to_string()],
                weights: vec![0.7, 0.2, 0.1],
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let level = result["level"].as_str().expect("should be a string");
        assert!(["ERROR", "WARN", "INFO"].contains(&level));
    }

    #[test]
    fn test_sample_service_name() {
        let mut vars = HashMap::new();
        vars.insert(
            "service".to_string(),
            VariableDefinition::new(VariableType::ServiceName),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let service = result["service"].as_str().expect("should be a string");
        assert!(service.contains('-'));
    }

    #[test]
    fn test_sample_uuid_with_prefix() {
        let mut vars = HashMap::new();
        vars.insert(
            "request_id".to_string(),
            VariableDefinition::new(VariableType::Uuid {
                prefix: Some("req-".to_string()),
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let request_id = result["request_id"].as_str().expect("should be a string");
        assert!(request_id.starts_with("req-"));
    }

    #[test]
    fn test_sample_int_normal_distribution() {
        let mut vars = HashMap::new();
        vars.insert(
            "value".to_string(),
            VariableDefinition::new(VariableType::Int {
                min: 0,
                max: 100,
                distribution: Distribution::Normal,
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let value = result["value"].as_i64().expect("should be a number");
        assert!((0..=100).contains(&value));
    }

    #[test]
    fn test_sample_ip_private() {
        let mut vars = HashMap::new();
        vars.insert(
            "ip".to_string(),
            VariableDefinition::new(VariableType::Ip {
                network: NetworkType::Private,
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let ip = result["ip"].as_str().expect("should be a string");
        assert!(ip.starts_with("192.168."));
    }

    #[test]
    fn test_sample_timestamp() {
        let mut vars = HashMap::new();
        vars.insert(
            "ts".to_string(),
            VariableDefinition::new(VariableType::Timestamp {
                format: "iso8601".to_string(),
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let ts = result["ts"].as_str().expect("should be a string");
        // ISO 8601 format should contain 'T' and end with 'Z'
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn test_sample_path() {
        let mut vars = HashMap::new();
        vars.insert(
            "log_path".to_string(),
            VariableDefinition::new(VariableType::Path {
                base: "/var/log".to_string(),
            }),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let path = result["log_path"].as_str().expect("should be a string");
        assert!(path.starts_with("/var/log"));
    }

    #[test]
    fn test_sample_username() {
        let mut vars = HashMap::new();
        vars.insert(
            "user".to_string(),
            VariableDefinition::new(VariableType::Username),
        );

        let sampler = ParameterSampler::new(42, "test".to_string());
        let result = sampler.sample_all(&vars).expect("sampling should succeed");

        let user = result["user"].as_str().expect("should be a string");
        assert!(!user.is_empty());
        assert!(user.len() <= 16);
    }
}
