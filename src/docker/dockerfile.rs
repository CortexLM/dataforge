//! Dockerfile generation for swe_forge tasks.
//!
//! This module provides utilities for generating Dockerfiles based on task configurations,
//! including base image selection and customization for different task categories.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base Ubuntu image with essential tools.
pub const BASE_UBUNTU: &str = "swe_forge/ubuntu-24.04:latest";

/// Python 3.13 image for Python-focused tasks.
pub const BASE_PYTHON: &str = "swe_forge/python-3.13:latest";

/// Node.js 22 image for JavaScript/TypeScript tasks.
pub const BASE_NODE: &str = "swe_forge/node-22:latest";

/// Rust 1.80 image for systems programming tasks.
pub const BASE_RUST: &str = "swe_forge/rust-1.80:latest";

/// Multi-language image with Python, Node, Go, and Rust.
pub const BASE_MULTI_LANG: &str = "swe_forge/multi-lang:latest";

/// Configuration for generating a Dockerfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerfileConfig {
    /// Base Docker image to use.
    pub base_image: String,
    /// Unique identifier for the task.
    pub task_id: String,
    /// Task category (e.g., "debugging", "data-science").
    pub category: String,
    /// Task difficulty level ("easy", "medium", "hard").
    pub difficulty: String,
    /// System packages to install via apt-get.
    pub packages: Vec<String>,
    /// Files to copy into the container as (source, destination) pairs.
    pub copy_paths: Vec<(String, String)>,
    /// Environment variables to set in the container.
    pub env_vars: HashMap<String, String>,
    /// User to run as in the container.
    pub user: String,
    /// Working directory in the container.
    pub workdir: String,
}

impl Default for DockerfileConfig {
    fn default() -> Self {
        Self {
            base_image: BASE_UBUNTU.to_string(),
            task_id: String::new(),
            category: String::new(),
            difficulty: "medium".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        }
    }
}

/// Builder for generating Dockerfile content.
#[derive(Debug, Clone)]
pub struct DockerfileBuilder {
    config: DockerfileConfig,
}

impl DockerfileBuilder {
    /// Create a new DockerfileBuilder with the given configuration.
    pub fn new(config: DockerfileConfig) -> Self {
        Self { config }
    }

    /// Build and return the Dockerfile content as a string.
    pub fn build(&self) -> String {
        let mut lines = Vec::new();

        // Base image
        lines.push(format!("FROM {}", self.config.base_image));
        lines.push(String::new());

        // Labels
        lines.push(format!(
            "LABEL swe_forge.task.id=\"{}\"",
            self.config.task_id
        ));
        lines.push(format!(
            "LABEL swe_forge.task.category=\"{}\"",
            self.config.category
        ));
        lines.push(format!(
            "LABEL swe_forge.task.difficulty=\"{}\"",
            self.config.difficulty
        ));
        lines.push(String::new());

        // Install additional packages if specified (with validation)
        let valid_packages = filter_valid_packages(&self.config.packages);
        if !valid_packages.is_empty() {
            lines.push("USER root".to_string());
            lines.push(format!(
                "RUN apt-get update && apt-get install -y --no-install-recommends {} && rm -rf /var/lib/apt/lists/*",
                valid_packages.join(" \\\n    ")
            ));
            lines.push(String::new());
        }

        // Environment variables
        for (key, value) in &self.config.env_vars {
            lines.push(format!("ENV {}=\"{}\"", key, escape_env_value(value)));
        }
        if !self.config.env_vars.is_empty() {
            lines.push(String::new());
        }

        // Switch to the configured user
        lines.push(format!("USER {}", self.config.user));
        lines.push(String::new());

        // Copy files
        for (src, dst) in &self.config.copy_paths {
            lines.push(format!("COPY {} {}", src, dst));
        }
        if !self.config.copy_paths.is_empty() {
            lines.push(String::new());
        }

        // Working directory
        lines.push(format!("WORKDIR {}", self.config.workdir));
        lines.push(String::new());

        // Default command
        lines.push("CMD [\"/bin/bash\"]".to_string());

        lines.join("\n")
    }
}

/// Escape special characters in environment variable values for Dockerfile.
fn escape_env_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
}

/// Validate a package name to prevent command injection.
///
/// Package names must only contain alphanumeric characters, hyphens, underscores,
/// periods, colons (for versioning), and plus signs. This prevents shell
/// metacharacters from being injected into apt-get commands.
///
/// # Arguments
/// * `package` - The package name to validate
///
/// # Returns
/// `true` if the package name is safe, `false` otherwise.
pub fn is_valid_package_name(package: &str) -> bool {
    if package.is_empty() {
        return false;
    }
    package
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '+'))
}

/// Filter and return only valid package names from a list.
///
/// Invalid package names are silently dropped. Use this to sanitize
/// package lists before Dockerfile generation.
pub fn filter_valid_packages(packages: &[String]) -> Vec<String> {
    packages
        .iter()
        .filter(|p| is_valid_package_name(p))
        .cloned()
        .collect()
}

/// Select the appropriate base image based on task category and requirements.
///
/// # Arguments
/// * `category` - The task category (e.g., "data-science", "systems", "web")
/// * `requirements` - Additional requirements that may influence image selection
///
/// # Returns
/// The base image name to use for the task.
pub fn select_base_image(category: &str, requirements: &[String]) -> String {
    // Check requirements for specific language needs
    let needs_python = requirements
        .iter()
        .any(|r| r.contains("python") || r.contains("pip"));
    let needs_node = requirements
        .iter()
        .any(|r| r.contains("node") || r.contains("npm") || r.contains("javascript"));
    let needs_rust = requirements
        .iter()
        .any(|r| r.contains("rust") || r.contains("cargo"));

    // Count how many languages are needed
    let language_count = [needs_python, needs_node, needs_rust]
        .iter()
        .filter(|&&x| x)
        .count();

    // If multiple languages needed, use multi-lang image
    if language_count > 1 {
        return BASE_MULTI_LANG.to_string();
    }

    // Check specific language requirements first
    if needs_python {
        return BASE_PYTHON.to_string();
    }
    if needs_node {
        return BASE_NODE.to_string();
    }
    if needs_rust {
        return BASE_RUST.to_string();
    }

    // Fall back to category-based selection
    match category.to_lowercase().as_str() {
        "data-science" | "data-analysis" | "machine-learning" | "ml" => BASE_PYTHON.to_string(),
        "web" | "frontend" | "backend-node" | "javascript" | "typescript" => BASE_NODE.to_string(),
        "systems" | "low-level" | "performance" | "embedded" => BASE_RUST.to_string(),
        "full-stack" | "devops" | "multi-language" => BASE_MULTI_LANG.to_string(),
        _ => BASE_UBUNTU.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dockerfile_builder_basic() {
        let config = DockerfileConfig {
            base_image: BASE_UBUNTU.to_string(),
            task_id: "test-001".to_string(),
            category: "file-operations".to_string(),
            difficulty: "easy".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let dockerfile = DockerfileBuilder::new(config).build();

        assert!(dockerfile.contains("FROM swe_forge/ubuntu-24.04:latest"));
        assert!(dockerfile.contains("LABEL swe_forge.task.id=\"test-001\""));
        assert!(dockerfile.contains("USER user"));
        assert!(dockerfile.contains("WORKDIR /home/user"));
        assert!(dockerfile.contains("CMD [\"/bin/bash\"]"));
    }

    #[test]
    fn test_dockerfile_builder_with_packages() {
        let config = DockerfileConfig {
            base_image: BASE_UBUNTU.to_string(),
            task_id: "test-002".to_string(),
            category: "debugging".to_string(),
            difficulty: "medium".to_string(),
            packages: vec!["vim".to_string(), "curl".to_string()],
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let dockerfile = DockerfileBuilder::new(config).build();

        assert!(dockerfile.contains("apt-get install"));
        assert!(dockerfile.contains("vim"));
        assert!(dockerfile.contains("curl"));
    }

    #[test]
    fn test_dockerfile_builder_with_env_vars() {
        let mut env_vars = HashMap::new();
        env_vars.insert("TASK_ID".to_string(), "test-003".to_string());
        env_vars.insert("DEBUG".to_string(), "true".to_string());

        let config = DockerfileConfig {
            base_image: BASE_PYTHON.to_string(),
            task_id: "test-003".to_string(),
            category: "data-science".to_string(),
            difficulty: "hard".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars,
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let dockerfile = DockerfileBuilder::new(config).build();

        assert!(dockerfile.contains("ENV TASK_ID=\"test-003\""));
        assert!(dockerfile.contains("ENV DEBUG=\"true\""));
    }

    #[test]
    fn test_select_base_image_by_category() {
        assert_eq!(
            select_base_image("data-science", &[]),
            BASE_PYTHON.to_string()
        );
        assert_eq!(select_base_image("web", &[]), BASE_NODE.to_string());
        assert_eq!(select_base_image("systems", &[]), BASE_RUST.to_string());
        assert_eq!(
            select_base_image("file-operations", &[]),
            BASE_UBUNTU.to_string()
        );
    }

    #[test]
    fn test_select_base_image_by_requirements() {
        let python_reqs = vec!["python3".to_string(), "pip".to_string()];
        assert_eq!(
            select_base_image("general", &python_reqs),
            BASE_PYTHON.to_string()
        );

        let node_reqs = vec!["npm".to_string()];
        assert_eq!(
            select_base_image("general", &node_reqs),
            BASE_NODE.to_string()
        );

        let multi_reqs = vec!["python".to_string(), "node".to_string()];
        assert_eq!(
            select_base_image("general", &multi_reqs),
            BASE_MULTI_LANG.to_string()
        );
    }

    #[test]
    fn test_escape_env_value() {
        assert_eq!(escape_env_value("simple"), "simple");
        assert_eq!(escape_env_value("with\"quote"), "with\\\"quote");
        assert_eq!(escape_env_value("with$var"), "with\\$var");
        assert_eq!(escape_env_value("with\\backslash"), "with\\\\backslash");
    }

    #[test]
    fn test_is_valid_package_name() {
        // Valid package names
        assert!(is_valid_package_name("vim"));
        assert!(is_valid_package_name("python3"));
        assert!(is_valid_package_name("python3.11"));
        assert!(is_valid_package_name("lib32-glibc"));
        assert!(is_valid_package_name("libssl-dev"));
        assert!(is_valid_package_name("g++"));
        assert!(is_valid_package_name("package:amd64"));

        // Invalid package names (potential injection)
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name("pkg; rm -rf /"));
        assert!(!is_valid_package_name("pkg && whoami"));
        assert!(!is_valid_package_name("pkg | cat /etc/passwd"));
        assert!(!is_valid_package_name("$(malicious)"));
        assert!(!is_valid_package_name("`id`"));
        assert!(!is_valid_package_name("pkg\nmalicious"));
    }

    #[test]
    fn test_filter_valid_packages() {
        let packages = vec![
            "vim".to_string(),
            "curl".to_string(),
            "pkg; rm -rf /".to_string(), // Invalid - should be filtered
            "python3".to_string(),
            "".to_string(), // Invalid - empty
        ];

        let filtered = filter_valid_packages(&packages);
        assert_eq!(filtered, vec!["vim", "curl", "python3"]);
    }
}
