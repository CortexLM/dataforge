//! Task instance generation from templates.
//!
//! This module implements the generation of concrete task instances from templates
//! and sampled parameters. It creates all necessary files including task.yaml,
//! Dockerfile, solution scripts, tests, and generated data files.

use crate::error::GeneratorError;
use crate::generator::file_generators::{
    ConfigFileGenerator, DataFileGenerator, FileGenerator, LogFileGenerator,
};
use crate::generator::solution::{DerivedSolution, SolutionDeriver};
use crate::generator::Result;
use crate::template::TaskTemplate;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};
use uuid::Uuid;

/// Canary configuration for a generated task instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryConfig {
    /// Unique canary identifier (UUID format).
    pub canary_id: String,
    /// Locations where the canary is embedded.
    pub locations: Vec<String>,
    /// SHA256 hash of the canary ID (for verification without exposure).
    pub canary_hash: String,
    /// ISO 8601 timestamp when the canary was generated.
    pub generated_at: String,
}

impl CanaryConfig {
    /// Creates a new canary configuration.
    pub fn new(canary_id: String, canary_hash: String, generated_at: String) -> Self {
        Self {
            canary_id,
            locations: Vec::new(),
            canary_hash,
            generated_at,
        }
    }

    /// Adds a location where the canary is embedded.
    pub fn add_location(&mut self, location: impl Into<String>) {
        self.locations.push(location.into());
    }
}

/// A generated task instance with all metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedInstance {
    /// Unique task identifier (template_id-seed format).
    pub task_id: String,
    /// Directory where the task files were created.
    pub task_dir: PathBuf,
    /// Parameters used to generate this instance.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Canary configuration for anti-hardcoding.
    pub canary: CanaryConfig,
}

impl GeneratedInstance {
    /// Returns the path to the task.yaml file.
    pub fn task_yaml_path(&self) -> PathBuf {
        self.task_dir.join("task.yaml")
    }

    /// Returns the path to the Dockerfile.
    pub fn dockerfile_path(&self) -> PathBuf {
        self.task_dir.join("Dockerfile")
    }

    /// Returns the path to the solution script.
    pub fn solution_path(&self) -> PathBuf {
        self.task_dir.join("solution.sh")
    }

    /// Returns the path to the tests directory.
    pub fn tests_dir(&self) -> PathBuf {
        self.task_dir.join("tests")
    }

    /// Returns the path to the task-deps directory.
    pub fn deps_dir(&self) -> PathBuf {
        self.task_dir.join("task-deps")
    }
}

/// Generator for concrete task instances from templates.
///
/// The `InstanceGenerator` takes a template and parameters, then creates all
/// necessary files for a complete task instance.
pub struct InstanceGenerator {
    template: TaskTemplate,
    params: HashMap<String, serde_json::Value>,
}

impl InstanceGenerator {
    /// Creates a new instance generator.
    ///
    /// # Arguments
    ///
    /// * `template` - The task template to generate from
    /// * `params` - Sampled parameter values
    pub fn new(template: TaskTemplate, params: HashMap<String, serde_json::Value>) -> Self {
        Self { template, params }
    }

    /// Generates a complete task instance.
    ///
    /// Creates the task directory and all necessary files.
    pub fn generate(&self, output_dir: &Path, seed: u64) -> Result<GeneratedInstance> {
        // Create task ID
        let task_id = format!("{}-{}", self.template.id, seed);

        // Create task directory
        let task_dir = output_dir.join(&task_id);
        fs::create_dir_all(&task_dir)?;

        // Generate canary
        let canary = self.generate_canary(&task_id, seed);

        // Create params with canary_id added
        let mut params_with_canary = self.params.clone();
        params_with_canary.insert(
            "canary_id".to_string(),
            serde_json::Value::String(canary.canary_id.clone()),
        );

        // Generate task.yaml
        let task_yaml = self.generate_task_yaml(&task_id, seed, &canary)?;
        fs::write(task_dir.join("task.yaml"), task_yaml)?;

        // Generate Dockerfile
        let dockerfile = self.generate_dockerfile(&task_id)?;
        fs::write(task_dir.join("Dockerfile"), dockerfile)?;

        // Generate solution.sh (in root for backward compatibility)
        let solution = self.render_template(&self.template.solution_template)?;
        fs::write(task_dir.join("solution.sh"), &solution)?;

        // Make solution.sh executable (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(task_dir.join("solution.sh"))?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(task_dir.join("solution.sh"), perms)?;
        }

        // Also create solution directory with the script (for validation compatibility)
        let solution_dir = task_dir.join("solution");
        fs::create_dir_all(&solution_dir)?;
        fs::write(solution_dir.join("solution.sh"), &solution)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(solution_dir.join("solution.sh"))?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(solution_dir.join("solution.sh"), perms)?;
        }

        // Generate DESCRIPTION.md
        let description = self.generate_description(&task_id)?;
        fs::write(task_dir.join("DESCRIPTION.md"), description)?;

        // Generate task-deps files
        self.generate_deps(&task_dir)?;

        // Generate tests
        self.generate_tests(&task_dir)?;

        Ok(GeneratedInstance {
            task_id,
            task_dir,
            parameters: self.params.clone(),
            canary,
        })
    }

    /// Renders a template string with the current parameters.
    fn render_template(&self, template_str: &str) -> Result<String> {
        let mut context = Context::new();

        // Add all parameters to the context
        for (key, value) in &self.params {
            context.insert(key, value);
        }

        // Use Tera to render the template
        match Tera::one_off(template_str, &context, false) {
            Ok(rendered) => Ok(rendered),
            Err(e) => Err(GeneratorError::Tera(e)),
        }
    }

    /// Generates the task.yaml content.
    fn generate_task_yaml(
        &self,
        task_id: &str,
        seed: u64,
        canary: &CanaryConfig,
    ) -> Result<String> {
        // Render instruction template
        let instruction = self.render_template(&self.template.instruction_template)?;

        // Build task data structure
        let difficulty_score =
            (self.template.difficulty.min_score + self.template.difficulty.max_score) / 2.0;

        let task_data = serde_yaml::to_value(&TaskYamlData {
            id: task_id.to_string(),
            template: self.template.id.clone(),
            seed,
            version: self.template.version.clone(),
            instruction,
            difficulty: self.template.difficulty.estimated.clone(),
            difficulty_score,
            category: self.template.category.clone(),
            subcategory: self.template.subcategory.clone(),
            tags: Vec::new(),
            timeouts: TimeoutsData {
                soft_timeout: self.estimate_timeout("soft"),
                hard_timeout: self.estimate_timeout("hard"),
                expected_time: self.estimate_timeout("expected"),
            },
            canary: CanaryData {
                id: canary.canary_id.clone(),
                locations: canary.locations.clone(),
            },
            parameters: self
                .params
                .iter()
                .filter(|(k, _)| *k != "canary_id")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })?;

        Ok(serde_yaml::to_string(&task_data)?)
    }

    /// Generates the Dockerfile content.
    fn generate_dockerfile(&self, task_id: &str) -> Result<String> {
        let base_image = self.select_base_image();
        let difficulty = &self.template.difficulty.estimated;

        let dockerfile = format!(
            r#"# Base image for task execution
FROM {base_image}

# Task-specific labels
LABEL task.id="{task_id}"
LABEL task.category="{category}"
LABEL task.difficulty="{difficulty}"

# Environment setup
ENV TASK_ID="{task_id}"
ENV TASK_TIMEOUT="{timeout}"

# Create working directories
RUN mkdir -p /var/log /home/user \
    && useradd -m -s /bin/bash user || true

# Copy task dependencies
COPY task-deps/ /task-deps/

# Set up log directory structure
RUN mkdir -p /var/log/app \
    && if [ -d /task-deps/data ]; then cp -r /task-deps/data/* /var/log/app/ 2>/dev/null || true; fi

# Set permissions
RUN chown -R user:user /home/user \
    && chmod -R 755 /task-deps 2>/dev/null || true

# Working directory
WORKDIR /home/user

# Run as non-root user
USER user

# Default command (agent will override)
CMD ["/bin/bash"]
"#,
            base_image = base_image,
            task_id = task_id,
            category = self.template.category,
            difficulty = difficulty,
            timeout = self.estimate_timeout("hard"),
        );

        Ok(dockerfile)
    }

    /// Generates the DESCRIPTION.md content.
    fn generate_description(&self, task_id: &str) -> Result<String> {
        // Render the instruction template
        let instruction = self.render_template(&self.template.instruction_template)?;

        let description = format!(
            r#"# Task: {task_id}

## Overview

**Category:** {category} / {subcategory}  
**Difficulty:** {difficulty}  
**Template:** {template_id}

## Instructions

{instruction}

## Requirements

- Complete the task within the time limit
- Ensure all outputs are created at the specified locations
- Follow the instructions precisely

## Environment

This task runs in a Docker container with standard Linux utilities available.
Working directory: `/home/user`
"#,
            task_id = task_id,
            category = self.template.category,
            subcategory = self.template.subcategory,
            difficulty = self.template.difficulty.estimated,
            template_id = self.template.id,
            instruction = instruction,
        );

        Ok(description)
    }

    /// Generates task dependency files.
    fn generate_deps(&self, task_dir: &Path) -> Result<()> {
        let deps_dir = task_dir.join("task-deps");
        fs::create_dir_all(&deps_dir)?;

        // Create data directory
        let data_dir = deps_dir.join("data");
        fs::create_dir_all(&data_dir)?;

        // Create configs directory
        let configs_dir = deps_dir.join("configs");
        fs::create_dir_all(&configs_dir)?;

        for file_config in &self.template.generated_files {
            // Determine the output path
            let file_path = if let Some(suffix) = file_config.path.strip_prefix("task-deps/") {
                deps_dir.join(suffix)
            } else {
                deps_dir.join(&file_config.path)
            };

            // Create parent directories
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Render config values with parameters
            let rendered_config: HashMap<String, serde_json::Value> = file_config
                .config
                .iter()
                .map(|(k, v)| {
                    let rendered = if let Some(s) = v.as_str() {
                        match self.render_template(s) {
                            Ok(rendered) => serde_json::Value::String(rendered),
                            Err(_) => v.clone(),
                        }
                    } else {
                        v.clone()
                    };
                    (k.clone(), rendered)
                })
                .collect();

            // Get the appropriate generator and generate content
            let content = self.generate_file_content(&file_config.generator, &rendered_config)?;
            fs::write(&file_path, content)?;
        }

        Ok(())
    }

    /// Generates file content using the appropriate generator.
    fn generate_file_content(
        &self,
        generator_name: &str,
        config: &HashMap<String, serde_json::Value>,
    ) -> Result<String> {
        match generator_name {
            "log_file_generator" => {
                let generator = LogFileGenerator::new(config.clone(), self.params.clone());
                generator.generate()
            }
            "config_file_generator" => {
                let generator = ConfigFileGenerator::new(config.clone(), self.params.clone());
                generator.generate()
            }
            "data_file_generator" => {
                let generator = DataFileGenerator::new(config.clone(), self.params.clone());
                generator.generate()
            }
            _ => Err(GeneratorError::UnknownGenerator(generator_name.to_string())),
        }
    }

    /// Generates test files.
    fn generate_tests(&self, task_dir: &Path) -> Result<()> {
        let tests_dir = task_dir.join("tests");
        fs::create_dir_all(&tests_dir)?;

        // Derive solution for expected outputs
        let deriver = SolutionDeriver::new(self.template.clone(), self.params.clone());
        let derived = deriver.derive()?;

        // Generate test_outputs.py
        let test_content = self.generate_test_file(&derived)?;
        fs::write(tests_dir.join("test_outputs.py"), test_content)?;

        // Generate conftest.py
        let conftest = self.generate_conftest()?;
        fs::write(tests_dir.join("conftest.py"), conftest)?;

        Ok(())
    }

    /// Generates the pytest test file.
    fn generate_test_file(&self, solution: &DerivedSolution) -> Result<String> {
        let mut constants = String::new();

        // Generate constants from parameters
        for (name, value) in &self.params {
            let const_name = name.to_uppercase();
            let value_repr = match value {
                serde_json::Value::String(s) => {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                }
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
                _ => format!("\"{}\"", value),
            };
            constants.push_str(&format!("{} = {}\n", const_name, value_repr));
        }

        // Generate output tests
        let mut output_tests = String::new();
        for (name, output_info) in &solution.expected_outputs {
            let test_name = name.replace('-', "_").to_lowercase();
            let path = &output_info.path;

            output_tests.push_str(&format!(
                r#"    def test_{test_name}_exists(self):
        """Output {name} must exist."""
        assert Path("{path}").exists(), "Output not found: {path}"

"#,
                test_name = test_name,
                name = name,
                path = path,
            ));
        }

        let test_file = format!(
            r#"""
Test suite for task: {task_id}
Category: {category} / {subcategory}
Auto-generated - do not edit
"""

import os
import re
import json
import pytest
from pathlib import Path

# Task-specific constants (generated from parameters)
{constants}

class TestOutputVerification:
    """Verify final output correctness."""
    
{output_tests}

class TestStateVerification:
    """Verify system state after completion."""
    
    def test_no_unexpected_errors(self):
        """No unexpected error files should exist."""
        error_indicators = ["/tmp/error.log", "/home/user/error.txt"]
        for path in error_indicators:
            if Path(path).exists():
                content = Path(path).read_text()
                assert not content.strip(), f"Unexpected error content in {{path}}"


def calculate_reward() -> dict:
    """Calculate final reward for the task."""
    reward = {{"score": 0.0, "max_score": 1.0, "breakdown": {{}}}}
    
    try:
        # Check expected outputs
        expected_outputs = {expected_outputs_repr}
        
        if not expected_outputs:
            # No outputs defined, check for result.txt
            result_path = Path("/home/user/result.txt")
            if result_path.exists():
                reward["score"] = 1.0
                reward["breakdown"]["result_exists"] = 1.0
        else:
            weight_per_output = 1.0 / len(expected_outputs)
            
            for name, output_def in expected_outputs.items():
                path = Path(output_def.get("path", "/home/user/result.txt"))
                
                # File exists check (25% of output weight)
                if path.exists():
                    reward["score"] += weight_per_output * 0.25
                    reward["breakdown"][f"{{name}}_exists"] = weight_per_output * 0.25
                    
                    # Content check (75% of output weight)
                    content = path.read_text().strip()
                    expected = output_def.get("content")
                    
                    if expected and content == expected:
                        reward["score"] += weight_per_output * 0.75
                        reward["breakdown"][f"{{name}}_correct"] = weight_per_output * 0.75
                    elif output_def.get("content_pattern"):
                        if re.match(output_def["content_pattern"], content):
                            reward["score"] += weight_per_output * 0.5
                            reward["breakdown"][f"{{name}}_format"] = weight_per_output * 0.5
    
    except Exception as e:
        reward["error"] = str(e)
    
    return reward


if __name__ == "__main__":
    reward = calculate_reward()
    Path("/home/user/reward.json").write_text(json.dumps(reward, indent=2))
    print(f"Reward: {{reward['score']:.2f}}/{{reward['max_score']:.2f}}")
"#,
            task_id = self.template.id,
            category = self.template.category,
            subcategory = self.template.subcategory,
            constants = constants,
            output_tests = output_tests,
            expected_outputs_repr = self.format_expected_outputs_dict(&solution.expected_outputs),
        );

        Ok(test_file)
    }

    /// Formats expected outputs as a Python dict literal.
    fn format_expected_outputs_dict(
        &self,
        outputs: &HashMap<String, crate::generator::solution::ExpectedOutputInfo>,
    ) -> String {
        if outputs.is_empty() {
            return "{}".to_string();
        }

        let mut items = Vec::new();
        for (name, info) in outputs {
            let mut parts = vec![format!("\"path\": \"{}\"", info.path)];
            if let Some(ref content) = info.content {
                parts.push(format!(
                    "\"content\": \"{}\"",
                    content.replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
            if let Some(ref pattern) = info.content_pattern {
                parts.push(format!(
                    "\"content_pattern\": \"{}\"",
                    pattern.replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
            items.push(format!("\"{}\": {{{}}}", name, parts.join(", ")));
        }

        format!("{{{}}}", items.join(", "))
    }

    /// Generates conftest.py content.
    fn generate_conftest(&self) -> Result<String> {
        let conftest = r#""""
Pytest configuration for task verification.
"""

import pytest
import os
from pathlib import Path


@pytest.fixture(scope="session")
def task_dir():
    """Returns the task directory path."""
    return Path(os.environ.get("TASK_DIR", "/task"))


@pytest.fixture(scope="session")
def user_home():
    """Returns the user home directory."""
    return Path("/home/user")


@pytest.fixture(scope="session")
def task_id():
    """Returns the task ID."""
    return os.environ.get("TASK_ID", "unknown")


@pytest.fixture
def result_file(user_home):
    """Returns the path to the result file."""
    return user_home / "result.txt"
"#;

        Ok(conftest.to_string())
    }

    /// Generates a canary configuration for anti-hardcoding.
    fn generate_canary(&self, task_id: &str, seed: u64) -> CanaryConfig {
        // Use UUID v5 with fixed namespace for deterministic generation
        let namespace =
            Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").expect("valid UUID constant");
        let canary_input = format!("{}-{}", task_id, seed);
        let canary_uuid = Uuid::new_v5(&namespace, canary_input.as_bytes());
        let canary_id = canary_uuid.to_string();

        // Generate hash for verification
        let mut hasher = Sha256::new();
        hasher.update(canary_id.as_bytes());
        let hash_bytes = hasher.finalize();
        let canary_hash = hex::encode(&hash_bytes[..8]); // First 16 hex chars

        // Create canary with locations from template
        let mut canary = CanaryConfig::new(
            canary_id,
            canary_hash,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );

        // Add locations from template's anti-hardcoding config
        for location in &self.template.anti_hardcoding.canary_locations {
            canary.add_location(location.clone());
        }

        canary
    }

    /// Selects the appropriate base Docker image based on task requirements.
    fn select_base_image(&self) -> &'static str {
        // Select based on category - simplified version
        match self.template.category.as_str() {
            "containers" => "swe_forge/ubuntu-24.04:latest",
            "data-science" => "swe_forge/python-3.11:latest",
            _ => "swe_forge/ubuntu-24.04:latest",
        }
    }

    /// Estimates timeout based on difficulty.
    fn estimate_timeout(&self, timeout_type: &str) -> u64 {
        let base_time = match self.template.difficulty.estimated.as_str() {
            "easy" => 180,   // 3 minutes base
            "medium" => 480, // 8 minutes base
            "hard" => 900,   // 15 minutes base
            _ => 480,        // default to medium
        };

        match timeout_type {
            "soft" => base_time,
            "hard" => base_time * 2,
            "expected" => base_time / 2,
            _ => base_time,
        }
    }
}

// Helper structs for YAML serialization

#[derive(Serialize)]
struct TaskYamlData {
    id: String,
    template: String,
    seed: u64,
    version: String,
    instruction: String,
    difficulty: String,
    difficulty_score: f64,
    category: String,
    subcategory: String,
    tags: Vec<String>,
    timeouts: TimeoutsData,
    canary: CanaryData,
    parameters: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct TimeoutsData {
    soft_timeout: u64,
    hard_timeout: u64,
    expected_time: u64,
}

#[derive(Serialize)]
struct CanaryData {
    id: String,
    locations: Vec<String>,
}

// We need hex encoding for the canary hash
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
