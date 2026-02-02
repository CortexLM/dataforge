//! Task Generator Agent for the multi-agent validation system.
//!
//! This agent generates tasks based on templates and difficulty levels,
//! using the existing Generator infrastructure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::difficulty::DifficultyLevel;
use crate::generator::{GeneratedInstance, Generator};
use crate::template::TaskTemplate;

use super::error::{AgentError, AgentResult};
use super::types::{GeneratedTask, ValidationResult};

/// Configuration for the Task Generator Agent.
#[derive(Debug, Clone)]
pub struct GeneratorAgentConfig {
    /// Base output directory for generated tasks.
    pub output_dir: PathBuf,
    /// Templates available for generation.
    templates: HashMap<String, TaskTemplate>,
}

impl GeneratorAgentConfig {
    /// Creates a new configuration with the given output directory.
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            templates: HashMap::new(),
        }
    }

    /// Adds a template to the configuration.
    pub fn with_template(mut self, template: TaskTemplate) -> Self {
        self.templates.insert(template.id.clone(), template);
        self
    }

    /// Adds multiple templates to the configuration.
    pub fn with_templates(mut self, templates: impl IntoIterator<Item = TaskTemplate>) -> Self {
        for template in templates {
            self.templates.insert(template.id.clone(), template);
        }
        self
    }

    /// Returns the available template IDs.
    pub fn template_ids(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// Gets a template by ID.
    pub fn get_template(&self, id: &str) -> Option<&TaskTemplate> {
        self.templates.get(id)
    }
}

/// Task Generator Agent that creates benchmark tasks from templates.
///
/// This agent encapsulates the task generation logic and provides
/// a consistent interface for the validation pipeline.
#[derive(Debug)]
pub struct GeneratorAgent {
    config: GeneratorAgentConfig,
}

impl GeneratorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "task_generator";

    /// Creates a new generator agent with the given configuration.
    pub fn new(config: GeneratorAgentConfig) -> Self {
        Self { config }
    }

    /// Generates a task based on difficulty level and seed.
    ///
    /// This method selects an appropriate template based on the requested difficulty,
    /// generates the task instance, and returns a GeneratedTask with metadata.
    ///
    /// # Arguments
    ///
    /// * `difficulty` - The target difficulty level for the task
    /// * `seed` - Random seed for deterministic generation
    ///
    /// # Returns
    ///
    /// A `GeneratedTask` containing the task metadata and paths, or an error.
    pub async fn generate_task(
        &self,
        difficulty: DifficultyLevel,
        seed: u64,
    ) -> AgentResult<GeneratedTask> {
        // Select a template matching the difficulty
        let template = self
            .select_template_for_difficulty(difficulty)
            .ok_or_else(|| {
                AgentError::TemplateNotFound(format!(
                    "No template found for difficulty {:?}",
                    difficulty
                ))
            })?;

        self.generate_task_from_template(template, difficulty, seed)
            .await
    }

    /// Generates a task from a specific template.
    ///
    /// # Arguments
    ///
    /// * `template_id` - The ID of the template to use
    /// * `seed` - Random seed for deterministic generation
    ///
    /// # Returns
    ///
    /// A `GeneratedTask` containing the task metadata and paths, or an error.
    pub async fn generate_task_with_template(
        &self,
        template_id: &str,
        seed: u64,
    ) -> AgentResult<GeneratedTask> {
        let template = self.config.get_template(template_id).ok_or_else(|| {
            AgentError::TemplateNotFound(format!("Template '{}' not found", template_id))
        })?;

        let difficulty = Self::difficulty_from_string(&template.difficulty.estimated)?;
        self.generate_task_from_template(template, difficulty, seed)
            .await
    }

    /// Internal method to generate a task from a template.
    async fn generate_task_from_template(
        &self,
        template: &TaskTemplate,
        difficulty: DifficultyLevel,
        seed: u64,
    ) -> AgentResult<GeneratedTask> {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&self.config.output_dir)?;

        // Create the generator
        let generator = Generator::new(template.clone(), seed);

        // Generate the task instance
        let instance = generator.generate(&self.config.output_dir)?;

        // Render the instruction template to get the actual instruction text
        let instruction = self.render_instruction(template, &instance)?;

        // Create the GeneratedTask from the instance
        let generated_task = GeneratedTask::from_instance(
            &instance,
            &template.id,
            difficulty,
            instruction,
            &template.category,
            &template.subcategory,
        );

        Ok(generated_task)
    }

    /// Renders the instruction template with the generated parameters.
    fn render_instruction(
        &self,
        template: &TaskTemplate,
        instance: &GeneratedInstance,
    ) -> AgentResult<String> {
        let mut context = tera::Context::new();
        for (key, value) in &instance.parameters {
            context.insert(key, value);
        }

        tera::Tera::one_off(&template.instruction_template, &context, false).map_err(|e| {
            AgentError::GenerationFailed(format!("Failed to render instruction: {}", e))
        })
    }

    /// Selects a template that matches the given difficulty level.
    fn select_template_for_difficulty(&self, difficulty: DifficultyLevel) -> Option<&TaskTemplate> {
        let difficulty_str = match difficulty {
            DifficultyLevel::Easy => "easy",
            DifficultyLevel::Medium => "medium",
            DifficultyLevel::Hard => "hard",
        };

        // Find a template matching the difficulty
        self.config
            .templates
            .values()
            .find(|t| t.difficulty.estimated == difficulty_str)
    }

    /// Converts a difficulty string to DifficultyLevel.
    fn difficulty_from_string(s: &str) -> AgentResult<DifficultyLevel> {
        match s.to_lowercase().as_str() {
            "easy" => Ok(DifficultyLevel::Easy),
            "medium" => Ok(DifficultyLevel::Medium),
            "hard" => Ok(DifficultyLevel::Hard),
            other => Err(AgentError::InvalidDifficulty(other.to_string())),
        }
    }

    /// Creates a validation result for the generation stage.
    ///
    /// Generation always "passes" if it completes without error,
    /// but we include metadata about the generated task.
    pub fn create_validation_result(&self, task: &GeneratedTask) -> ValidationResult {
        ValidationResult::success_full(
            format!(
                "Successfully generated task '{}' from template '{}' with difficulty {:?}",
                task.task_id, task.template_id, task.difficulty
            ),
            format!("Task directory: {:?}", task.task_dir),
            1.0,
            Self::AGENT_NAME,
        )
    }

    /// Returns the output directory.
    pub fn output_dir(&self) -> &Path {
        &self.config.output_dir
    }

    /// Returns the number of available templates.
    pub fn template_count(&self) -> usize {
        self.config.templates.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{DifficultyConfig, TaskTemplate};
    use tempfile::TempDir;

    fn create_test_template(difficulty: &str) -> TaskTemplate {
        TaskTemplate::new(
            format!("test-{}-001", difficulty),
            "1.0.0",
            "debugging",
            "log-analysis",
            match difficulty {
                "easy" => DifficultyConfig::easy(),
                "hard" => DifficultyConfig::hard(),
                _ => DifficultyConfig::medium(),
            },
            "Find the error in the log file.",
            "grep 'ERROR' /var/log/app.log",
        )
    }

    #[tokio::test]
    async fn test_generator_agent_creation() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config = GeneratorAgentConfig::new(temp_dir.path())
            .with_template(create_test_template("medium"));

        let agent = GeneratorAgent::new(config);
        assert_eq!(agent.template_count(), 1);
    }

    #[tokio::test]
    async fn test_template_selection() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config = GeneratorAgentConfig::new(temp_dir.path())
            .with_template(create_test_template("easy"))
            .with_template(create_test_template("medium"))
            .with_template(create_test_template("hard"));

        let agent = GeneratorAgent::new(config);

        let easy_template = agent.select_template_for_difficulty(DifficultyLevel::Easy);
        assert!(easy_template.is_some());
        assert_eq!(
            easy_template
                .expect("has easy template")
                .difficulty
                .estimated,
            "easy"
        );

        let hard_template = agent.select_template_for_difficulty(DifficultyLevel::Hard);
        assert!(hard_template.is_some());
        assert_eq!(
            hard_template
                .expect("has hard template")
                .difficulty
                .estimated,
            "hard"
        );
    }

    #[test]
    fn test_difficulty_from_string() {
        assert_eq!(
            GeneratorAgent::difficulty_from_string("easy").expect("valid difficulty"),
            DifficultyLevel::Easy
        );
        assert_eq!(
            GeneratorAgent::difficulty_from_string("MEDIUM").expect("valid difficulty"),
            DifficultyLevel::Medium
        );
        assert_eq!(
            GeneratorAgent::difficulty_from_string("Hard").expect("valid difficulty"),
            DifficultyLevel::Hard
        );
        assert!(GeneratorAgent::difficulty_from_string("impossible").is_err());
    }

    #[test]
    fn test_validation_result_creation() {
        let task = GeneratedTask::minimal(
            "test-123",
            "test-template",
            DifficultyLevel::Medium,
            "Test instruction",
        );

        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config = GeneratorAgentConfig::new(temp_dir.path());
        let agent = GeneratorAgent::new(config);

        let result = agent.create_validation_result(&task);
        assert!(result.is_success());
        assert_eq!(result.agent_name(), GeneratorAgent::AGENT_NAME);
        assert_eq!(result.score(), Some(1.0));
    }
}
