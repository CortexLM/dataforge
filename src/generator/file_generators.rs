//! File content generators for task data files.
//!
//! This module provides generators for creating realistic task data files:
//! - Log files with realistic patterns and error injection
//! - Configuration files in various formats
//! - Data files (CSV, JSON) with structured content
//!
//! Each generator takes a configuration HashMap and parameters,
//! and produces the file content as a String.

use crate::generator::Result;
use chrono::{Duration, Utc};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for file content generators.
pub trait FileGenerator {
    /// Generates file content based on configuration and parameters.
    fn generate(&self) -> Result<String>;
}

/// Generator for realistic log files with error injection.
///
/// Creates log files that mimic real application logs with:
/// - Timestamps in configurable formats
/// - Various log levels (DEBUG, INFO, WARN, ERROR, etc.)
/// - Service/component tags
/// - Request IDs and other metadata
/// - Targeted error injection at specific lines
///
/// # Configuration
///
/// - `lines`: Number of log lines to generate
/// - `error_line`: Line number where the target error should appear
/// - `error_level`: Level of the injected error (ERROR, CRITICAL, etc.)
/// - `error_code`: HTTP status code or error code
/// - `request_id`: Request ID to inject in the error line
/// - `service_name`: Name of the service for log tags
/// - `timestamp_format`: Format for timestamps (iso8601, unix, log)
pub struct LogFileGenerator {
    config: HashMap<String, Value>,
    params: HashMap<String, Value>,
}

impl LogFileGenerator {
    /// Creates a new log file generator.
    pub fn new(config: HashMap<String, Value>, params: HashMap<String, Value>) -> Self {
        Self { config, params }
    }

    /// Gets a config value as a string.
    fn get_config_str(&self, key: &str) -> Option<String> {
        self.config.get(key).and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    }

    /// Gets a config value as an integer.
    fn get_config_int(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| match v {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        })
    }

    /// Generates a realistic log entry.
    fn generate_log_entry(
        &self,
        rng: &mut ChaCha8Rng,
        timestamp: &str,
        service_name: &str,
    ) -> String {
        let levels = ["DEBUG", "INFO", "INFO", "INFO", "WARN"]; // Weighted toward INFO
        let level = levels[rng.random_range(0..levels.len())];

        let messages = [
            "Processing request",
            "Cache hit for key",
            "Cache miss - fetching from database",
            "Database query completed",
            "User authenticated successfully",
            "Session created",
            "Request completed successfully",
            "Connection pool acquired connection",
            "Health check passed",
            "Configuration reloaded",
            "Metrics exported",
            "Background job started",
            "Background job completed",
            "Rate limit check passed",
            "Token validated",
        ];
        let message = messages[rng.random_range(0..messages.len())];

        let request_id = format!("req-{:08x}", rng.random::<u32>());
        let duration_ms = rng.random_range(1..500);

        format!(
            "{} {} [{}] {} request_id={} duration_ms={}",
            timestamp, level, service_name, message, request_id, duration_ms
        )
    }

    /// Generates the target error line.
    fn generate_error_line(
        &self,
        timestamp: &str,
        service_name: &str,
        error_level: &str,
        error_code: &str,
        request_id: &str,
    ) -> String {
        let error_messages = [
            "Service temporarily unavailable",
            "Connection refused",
            "Request timeout exceeded",
            "Internal server error",
            "Database connection failed",
            "Upstream service unavailable",
        ];

        // Use a deterministic selection based on error_code
        let code_num: usize = error_code.parse().unwrap_or(500);
        let message = error_messages[code_num % error_messages.len()];

        format!(
            "{} {} [{}] HTTP {} {} request_id={} duration_ms=5023",
            timestamp, error_level, service_name, error_code, message, request_id
        )
    }

    /// Formats a timestamp according to the specified format.
    fn format_timestamp(&self, base_time: chrono::DateTime<Utc>, format: &str) -> String {
        match format {
            "unix" => base_time.timestamp().to_string(),
            "unix_millis" => base_time.timestamp_millis().to_string(),
            "log" => base_time.format("%Y-%m-%d %H:%M:%S.%3f").to_string(),
            _ => base_time.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string(),
        }
    }
}

impl FileGenerator for LogFileGenerator {
    fn generate(&self) -> Result<String> {
        let lines_count = self.get_config_int("lines").unwrap_or(1000) as usize;
        let error_line = self.get_config_int("error_line").unwrap_or(500) as usize;
        let error_level = self
            .get_config_str("error_level")
            .unwrap_or_else(|| "ERROR".to_string());
        let error_code = self
            .get_config_str("error_code")
            .unwrap_or_else(|| "500".to_string());
        let request_id = self
            .get_config_str("request_id")
            .unwrap_or_else(|| "req-unknown".to_string());
        let service_name = self
            .get_config_str("service_name")
            .unwrap_or_else(|| "app-service".to_string());
        let timestamp_format = self
            .get_config_str("timestamp_format")
            .unwrap_or_else(|| "iso8601".to_string());

        // Create deterministic RNG from params hash
        let seed = self.compute_seed();
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        // Base time is 24 hours ago
        let base_time = Utc::now() - Duration::hours(24);

        let mut lines = Vec::with_capacity(lines_count);

        for i in 1..=lines_count {
            // Calculate timestamp for this line
            let line_time = base_time + Duration::milliseconds((i as i64) * 500);
            let timestamp = self.format_timestamp(line_time, &timestamp_format);

            let line = if i == error_line {
                // This is the target error line
                self.generate_error_line(
                    &timestamp,
                    &service_name,
                    &error_level,
                    &error_code,
                    &request_id,
                )
            } else {
                // Regular log entry
                self.generate_log_entry(&mut rng, &timestamp, &service_name)
            };

            lines.push(line);
        }

        Ok(lines.join("\n"))
    }
}

impl LogFileGenerator {
    /// Computes a seed from the parameters for deterministic generation.
    fn compute_seed(&self) -> u64 {
        let mut hash: u64 = 0;
        for (key, value) in &self.params {
            for byte in key.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
            }
            let value_str = value.to_string();
            for byte in value_str.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
            }
        }
        hash
    }
}

/// Generator for configuration files.
///
/// Creates configuration files in various formats (YAML, JSON, TOML-like, INI-like)
/// with realistic settings and values.
///
/// # Configuration
///
/// - `format`: Output format (yaml, json, ini, env)
/// - `service_name`: Service name for context
/// - `port`: Port number to include
/// - `host`: Hostname to include
/// - `additional`: Additional key-value pairs to include
pub struct ConfigFileGenerator {
    config: HashMap<String, Value>,
    #[allow(dead_code)]
    params: HashMap<String, Value>,
}

impl ConfigFileGenerator {
    /// Creates a new config file generator.
    pub fn new(config: HashMap<String, Value>, params: HashMap<String, Value>) -> Self {
        Self { config, params }
    }

    fn get_config_str(&self, key: &str) -> Option<String> {
        self.config.get(key).and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    }

    fn get_config_int(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| match v {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        })
    }

    /// Generates YAML format configuration.
    fn generate_yaml(&self, service_name: &str, port: i64, host: &str) -> String {
        format!(
            r#"# Configuration for {service_name}
# Generated by dataforge

server:
  host: "{host}"
  port: {port}
  timeout: 30
  max_connections: 100

logging:
  level: "info"
  format: "json"
  output: "/var/log/{service_name}/app.log"

database:
  host: "localhost"
  port: 5432
  name: "{service_name}_db"
  pool_size: 10

cache:
  enabled: true
  ttl: 3600
  max_size: 1000

features:
  rate_limiting: true
  metrics: true
  health_check: true
"#,
            service_name = service_name,
            host = host,
            port = port
        )
    }

    /// Generates JSON format configuration.
    fn generate_json(&self, service_name: &str, port: i64, host: &str) -> String {
        format!(
            r#"{{
  "server": {{
    "host": "{host}",
    "port": {port},
    "timeout": 30,
    "max_connections": 100
  }},
  "logging": {{
    "level": "info",
    "format": "json",
    "output": "/var/log/{service_name}/app.log"
  }},
  "database": {{
    "host": "localhost",
    "port": 5432,
    "name": "{service_name}_db",
    "pool_size": 10
  }},
  "cache": {{
    "enabled": true,
    "ttl": 3600,
    "max_size": 1000
  }},
  "features": {{
    "rate_limiting": true,
    "metrics": true,
    "health_check": true
  }}
}}"#,
            service_name = service_name,
            host = host,
            port = port
        )
    }

    /// Generates INI-like format configuration.
    fn generate_ini(&self, service_name: &str, port: i64, host: &str) -> String {
        format!(
            r#"# Configuration for {service_name}
# Generated by dataforge

[server]
host = {host}
port = {port}
timeout = 30
max_connections = 100

[logging]
level = info
format = json
output = /var/log/{service_name}/app.log

[database]
host = localhost
port = 5432
name = {service_name}_db
pool_size = 10

[cache]
enabled = true
ttl = 3600
max_size = 1000

[features]
rate_limiting = true
metrics = true
health_check = true
"#,
            service_name = service_name,
            host = host,
            port = port
        )
    }

    /// Generates environment variable format.
    fn generate_env(&self, service_name: &str, port: i64, host: &str) -> String {
        let prefix = service_name.to_uppercase().replace('-', "_");
        format!(
            r#"# Environment configuration for {service_name}
# Generated by dataforge

{prefix}_HOST={host}
{prefix}_PORT={port}
{prefix}_TIMEOUT=30
{prefix}_MAX_CONNECTIONS=100
{prefix}_LOG_LEVEL=info
{prefix}_LOG_FORMAT=json
{prefix}_LOG_OUTPUT=/var/log/{service_name}/app.log
{prefix}_DB_HOST=localhost
{prefix}_DB_PORT=5432
{prefix}_DB_NAME={service_name}_db
{prefix}_DB_POOL_SIZE=10
{prefix}_CACHE_ENABLED=true
{prefix}_CACHE_TTL=3600
{prefix}_CACHE_MAX_SIZE=1000
{prefix}_FEATURE_RATE_LIMITING=true
{prefix}_FEATURE_METRICS=true
{prefix}_FEATURE_HEALTH_CHECK=true
"#,
            service_name = service_name,
            prefix = prefix,
            host = host,
            port = port
        )
    }
}

impl FileGenerator for ConfigFileGenerator {
    fn generate(&self) -> Result<String> {
        let format = self
            .get_config_str("format")
            .unwrap_or_else(|| "yaml".to_string());
        let service_name = self
            .get_config_str("service_name")
            .unwrap_or_else(|| "app-service".to_string());
        let port = self.get_config_int("port").unwrap_or(8080);
        let host = self
            .get_config_str("host")
            .unwrap_or_else(|| "0.0.0.0".to_string());

        let content = match format.as_str() {
            "json" => self.generate_json(&service_name, port, &host),
            "ini" => self.generate_ini(&service_name, port, &host),
            "env" => self.generate_env(&service_name, port, &host),
            _ => self.generate_yaml(&service_name, port, &host),
        };

        Ok(content)
    }
}

/// Generator for structured data files (CSV, JSON arrays).
///
/// Creates data files with realistic structured content for data processing tasks.
///
/// # Configuration
///
/// - `format`: Output format (csv, json, jsonl)
/// - `rows`: Number of data rows to generate
/// - `columns`: List of column definitions
/// - `has_errors`: Whether to inject error rows
/// - `error_row`: Specific row to contain an error
pub struct DataFileGenerator {
    config: HashMap<String, Value>,
    params: HashMap<String, Value>,
}

impl DataFileGenerator {
    /// Creates a new data file generator.
    pub fn new(config: HashMap<String, Value>, params: HashMap<String, Value>) -> Self {
        Self { config, params }
    }

    fn get_config_str(&self, key: &str) -> Option<String> {
        self.config.get(key).and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    }

    fn get_config_int(&self, key: &str) -> Option<i64> {
        self.config.get(key).and_then(|v| match v {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        })
    }

    fn get_config_bool(&self, key: &str) -> Option<bool> {
        self.config.get(key).and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            Value::String(s) => s.parse().ok(),
            _ => None,
        })
    }

    /// Computes a seed from the parameters for deterministic generation.
    fn compute_seed(&self) -> u64 {
        let mut hash: u64 = 0;
        for (key, value) in &self.params {
            for byte in key.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
            }
            let value_str = value.to_string();
            for byte in value_str.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
            }
        }
        hash
    }

    /// Generates a CSV data file.
    fn generate_csv(&self, rows: usize, rng: &mut ChaCha8Rng, error_row: Option<usize>) -> String {
        let mut lines = Vec::with_capacity(rows + 1);

        // Header
        lines.push("id,name,email,age,score,status,created_at".to_string());

        let first_names = [
            "Alice", "Bob", "Carol", "David", "Eve", "Frank", "Grace", "Henry", "Ivy", "Jack",
            "Kate", "Leo", "Maya", "Nick", "Olivia", "Paul",
        ];

        let last_names = [
            "Smith",
            "Johnson",
            "Williams",
            "Brown",
            "Jones",
            "Garcia",
            "Miller",
            "Davis",
            "Rodriguez",
            "Martinez",
            "Hernandez",
            "Lopez",
            "Gonzalez",
            "Wilson",
        ];

        let statuses = ["active", "inactive", "pending", "suspended"];

        for i in 1..=rows {
            let first = first_names[rng.random_range(0..first_names.len())];
            let last = last_names[rng.random_range(0..last_names.len())];
            let name = format!("{} {}", first, last);
            let email = format!(
                "{}.{}@example.com",
                first.to_lowercase(),
                last.to_lowercase()
            );
            let age = rng.random_range(18..80);
            let score = rng.random_range(0.0..100.0);
            let status = statuses[rng.random_range(0..statuses.len())];

            let days_ago = rng.random_range(0..365);
            let created_at = Utc::now() - Duration::days(days_ago);
            let created_str = created_at.format("%Y-%m-%d").to_string();

            let line = if Some(i) == error_row {
                // Inject an error - malformed row
                format!("{},{},,{},{},{}", i, name, age, score, status)
            } else {
                format!(
                    "{},{},{},{},{:.2},{},{}",
                    i, name, email, age, score, status, created_str
                )
            };

            lines.push(line);
        }

        lines.join("\n")
    }

    /// Generates a JSON array data file.
    fn generate_json(&self, rows: usize, rng: &mut ChaCha8Rng, error_row: Option<usize>) -> String {
        let mut items = Vec::with_capacity(rows);

        let first_names = [
            "Alice", "Bob", "Carol", "David", "Eve", "Frank", "Grace", "Henry",
        ];

        let last_names = [
            "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis",
        ];

        let statuses = ["active", "inactive", "pending", "suspended"];

        for i in 1..=rows {
            let first = first_names[rng.random_range(0..first_names.len())];
            let last = last_names[rng.random_range(0..last_names.len())];
            let name = format!("{} {}", first, last);
            let email = format!(
                "{}.{}@example.com",
                first.to_lowercase(),
                last.to_lowercase()
            );
            let age = rng.random_range(18..80);
            let score = rng.random_range(0.0..100.0);
            let status = statuses[rng.random_range(0..statuses.len())];

            let days_ago = rng.random_range(0..365);
            let created_at = Utc::now() - Duration::days(days_ago);
            let created_str = created_at.format("%Y-%m-%d").to_string();

            let item = if Some(i) == error_row {
                // Inject error - invalid data
                format!(
                    r#"  {{"id": {}, "name": "{}", "email": null, "age": "invalid", "score": {:.2}, "status": "{}", "created_at": "{}"}}"#,
                    i, name, score, status, created_str
                )
            } else {
                format!(
                    r#"  {{"id": {}, "name": "{}", "email": "{}", "age": {}, "score": {:.2}, "status": "{}", "created_at": "{}"}}"#,
                    i, name, email, age, score, status, created_str
                )
            };

            items.push(item);
        }

        format!("[\n{}\n]", items.join(",\n"))
    }

    /// Generates a JSON Lines data file.
    fn generate_jsonl(
        &self,
        rows: usize,
        rng: &mut ChaCha8Rng,
        error_row: Option<usize>,
    ) -> String {
        let mut lines = Vec::with_capacity(rows);

        let first_names = [
            "Alice", "Bob", "Carol", "David", "Eve", "Frank", "Grace", "Henry",
        ];

        let last_names = [
            "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis",
        ];

        let statuses = ["active", "inactive", "pending", "suspended"];

        for i in 1..=rows {
            let first = first_names[rng.random_range(0..first_names.len())];
            let last = last_names[rng.random_range(0..last_names.len())];
            let name = format!("{} {}", first, last);
            let email = format!(
                "{}.{}@example.com",
                first.to_lowercase(),
                last.to_lowercase()
            );
            let age = rng.random_range(18..80);
            let score = rng.random_range(0.0..100.0);
            let status = statuses[rng.random_range(0..statuses.len())];

            let days_ago = rng.random_range(0..365);
            let created_at = Utc::now() - Duration::days(days_ago);
            let created_str = created_at.format("%Y-%m-%d").to_string();

            let line = if Some(i) == error_row {
                // Inject error - malformed JSON
                format!(
                    r#"{{"id": {}, "name": "{}", "email": null, "age": "invalid", "score": {:.2}, "status": "{}", "created_at": "{}"}}"#,
                    i, name, score, status, created_str
                )
            } else {
                format!(
                    r#"{{"id": {}, "name": "{}", "email": "{}", "age": {}, "score": {:.2}, "status": "{}", "created_at": "{}"}}"#,
                    i, name, email, age, score, status, created_str
                )
            };

            lines.push(line);
        }

        lines.join("\n")
    }
}

impl FileGenerator for DataFileGenerator {
    fn generate(&self) -> Result<String> {
        let format = self
            .get_config_str("format")
            .unwrap_or_else(|| "csv".to_string());
        let rows = self.get_config_int("rows").unwrap_or(100) as usize;
        let has_errors = self.get_config_bool("has_errors").unwrap_or(false);
        let error_row = if has_errors {
            Some(self.get_config_int("error_row").unwrap_or(50) as usize)
        } else {
            None
        };

        let seed = self.compute_seed();
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let content = match format.as_str() {
            "json" => self.generate_json(rows, &mut rng, error_row),
            "jsonl" => self.generate_jsonl(rows, &mut rng, error_row),
            _ => self.generate_csv(rows, &mut rng, error_row),
        };

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_file_generator() {
        let mut config = HashMap::new();
        config.insert("lines".to_string(), Value::Number(100.into()));
        config.insert("error_line".to_string(), Value::Number(50.into()));
        config.insert(
            "error_level".to_string(),
            Value::String("ERROR".to_string()),
        );
        config.insert("error_code".to_string(), Value::String("503".to_string()));
        config.insert(
            "request_id".to_string(),
            Value::String("req-12345678".to_string()),
        );
        config.insert(
            "service_name".to_string(),
            Value::String("auth-service".to_string()),
        );

        let params = HashMap::new();
        let generator = LogFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        assert!(content.contains("req-12345678"));
        assert!(content.contains("ERROR"));
        assert!(content.contains("503"));
        assert!(content.contains("auth-service"));
    }

    #[test]
    fn test_log_file_generator_line_count() {
        let mut config = HashMap::new();
        config.insert("lines".to_string(), Value::Number(50.into()));
        config.insert("error_line".to_string(), Value::Number(25.into()));

        let params = HashMap::new();
        let generator = LogFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        let line_count = content.lines().count();
        assert_eq!(line_count, 50);
    }

    #[test]
    fn test_config_file_generator_yaml() {
        let mut config = HashMap::new();
        config.insert("format".to_string(), Value::String("yaml".to_string()));
        config.insert(
            "service_name".to_string(),
            Value::String("test-service".to_string()),
        );
        config.insert("port".to_string(), Value::Number(8080.into()));

        let params = HashMap::new();
        let generator = ConfigFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        assert!(content.contains("test-service"));
        assert!(content.contains("port: 8080"));
        assert!(content.contains("server:"));
    }

    #[test]
    fn test_config_file_generator_json() {
        let mut config = HashMap::new();
        config.insert("format".to_string(), Value::String("json".to_string()));
        config.insert(
            "service_name".to_string(),
            Value::String("api-service".to_string()),
        );
        config.insert("port".to_string(), Value::Number(3000.into()));

        let params = HashMap::new();
        let generator = ConfigFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        assert!(content.contains("\"port\": 3000"));
        assert!(content.contains("api-service"));
    }

    #[test]
    fn test_data_file_generator_csv() {
        let mut config = HashMap::new();
        config.insert("format".to_string(), Value::String("csv".to_string()));
        config.insert("rows".to_string(), Value::Number(10.into()));

        let params = HashMap::new();
        let generator = DataFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        // Should have header + 10 data rows
        let line_count = content.lines().count();
        assert_eq!(line_count, 11);

        // Should have CSV header
        assert!(content.starts_with("id,name,email"));
    }

    #[test]
    fn test_data_file_generator_json() {
        let mut config = HashMap::new();
        config.insert("format".to_string(), Value::String("json".to_string()));
        config.insert("rows".to_string(), Value::Number(5.into()));

        let params = HashMap::new();
        let generator = DataFileGenerator::new(config, params);
        let content = generator.generate().expect("generation should succeed");

        assert!(content.starts_with('['));
        assert!(content.ends_with(']'));
        assert!(content.contains("\"id\":"));
    }

    #[test]
    fn test_data_file_generator_deterministic() {
        let mut config = HashMap::new();
        config.insert("format".to_string(), Value::String("csv".to_string()));
        config.insert("rows".to_string(), Value::Number(10.into()));

        let mut params = HashMap::new();
        params.insert(
            "seed_value".to_string(),
            Value::String("test-seed".to_string()),
        );

        let generator1 = DataFileGenerator::new(config.clone(), params.clone());
        let generator2 = DataFileGenerator::new(config, params);

        let content1 = generator1.generate().expect("generation should succeed");
        let content2 = generator2.generate().expect("generation should succeed");

        assert_eq!(content1, content2);
    }
}
