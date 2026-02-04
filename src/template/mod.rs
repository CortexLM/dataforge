//! Template system for dataforge task definitions.
//!
//! This module provides the infrastructure for defining, loading, and validating
//! task templates. Templates define the structure of benchmark tasks, including
//! variable definitions, instruction templates, expected outputs, and anti-hardcoding
//! configurations.
//!
//! # Example
//!
//! ```ignore
//! use dataforge::template::{TemplateLoader, TaskTemplate};
//!
//! let mut loader = TemplateLoader::new();
//! loader.load_directory("templates/")?;
//!
//! let template = loader.get("debug-log-001")?;
//! println!("Loaded template: {}", template.id);
//! ```

pub mod schema;
pub mod types;
pub mod variables;

pub use schema::{
    AntiHardcodingConfig, DifficultyConfig, ExpectedOutput, GeneratedFileConfig,
    ProcessValidationConfig, TaskTemplate,
};
pub use variables::{Distribution, NetworkType, VariableDefinition, VariableType};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::TemplateError;

/// Template loader for loading and caching task templates from YAML files.
#[derive(Debug, Default)]
pub struct TemplateLoader {
    /// Cache of loaded templates, keyed by template ID.
    templates: HashMap<String, TaskTemplate>,
    /// Paths of loaded template files, keyed by template ID.
    loaded_paths: HashMap<String, PathBuf>,
}

impl TemplateLoader {
    /// Creates a new empty template loader.
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            loaded_paths: HashMap::new(),
        }
    }

    /// Loads a single template from a YAML file.
    ///
    /// The template is validated after loading. If validation fails, an error is returned
    /// and the template is not added to the cache.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the YAML template file.
    ///
    /// # Returns
    ///
    /// A reference to the loaded template, or an error if loading/validation fails.
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<&TaskTemplate, TemplateError> {
        let path = path.as_ref();
        let path_str = path.display().to_string();

        // Read the file content
        let content = fs::read_to_string(path).map_err(TemplateError::Io)?;

        // Parse the YAML content
        let template: TaskTemplate =
            serde_yaml::from_str(&content).map_err(|e| TemplateError::ParseError {
                path: path_str.clone(),
                message: e.to_string(),
            })?;

        // Validate the template
        template.validate()?;

        // Check for duplicate template ID
        if self.templates.contains_key(&template.id) {
            return Err(TemplateError::DuplicateTemplateId(template.id.clone()));
        }

        // Store the template and path
        let id = template.id.clone();
        self.templates.insert(id.clone(), template);
        self.loaded_paths.insert(id.clone(), path.to_path_buf());

        // Return a reference to the stored template
        Ok(self.templates.get(&id).expect("template was just inserted"))
    }

    /// Loads all YAML templates from a directory (non-recursive).
    ///
    /// Files must have a `.yaml` or `.yml` extension to be loaded.
    ///
    /// # Arguments
    ///
    /// * `dir` - Path to the directory containing template files.
    ///
    /// # Returns
    ///
    /// The number of templates successfully loaded.
    pub fn load_directory<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, TemplateError> {
        let dir = dir.as_ref();
        let mut count = 0;

        let entries = fs::read_dir(dir).map_err(TemplateError::Io)?;

        for entry in entries {
            let entry = entry.map_err(TemplateError::Io)?;
            let path = entry.path();

            // Skip directories and non-YAML files
            if path.is_dir() {
                continue;
            }

            let is_yaml = path
                .extension()
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false);

            if is_yaml {
                self.load_file(&path)?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Loads all YAML templates from a directory recursively.
    ///
    /// # Arguments
    ///
    /// * `dir` - Path to the root directory.
    ///
    /// # Returns
    ///
    /// The number of templates successfully loaded.
    pub fn load_directory_recursive<P: AsRef<Path>>(
        &mut self,
        dir: P,
    ) -> Result<usize, TemplateError> {
        let dir = dir.as_ref();
        let mut count = 0;

        self.load_recursive_inner(dir, &mut count)?;

        Ok(count)
    }

    /// Internal recursive directory loading implementation.
    fn load_recursive_inner(&mut self, dir: &Path, count: &mut usize) -> Result<(), TemplateError> {
        let entries = fs::read_dir(dir).map_err(TemplateError::Io)?;

        for entry in entries {
            let entry = entry.map_err(TemplateError::Io)?;
            let path = entry.path();

            if path.is_dir() {
                self.load_recursive_inner(&path, count)?;
            } else {
                let is_yaml = path
                    .extension()
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false);

                if is_yaml {
                    self.load_file(&path)?;
                    *count += 1;
                }
            }
        }

        Ok(())
    }

    /// Gets a template by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The template ID to look up.
    ///
    /// # Returns
    ///
    /// A reference to the template, or an error if not found.
    pub fn get(&self, id: &str) -> Result<&TaskTemplate, TemplateError> {
        self.templates
            .get(id)
            .ok_or_else(|| TemplateError::NotFound(id.to_string()))
    }

    /// Gets a template by its ID, returning None if not found.
    pub fn get_opt(&self, id: &str) -> Option<&TaskTemplate> {
        self.templates.get(id)
    }

    /// Gets the file path from which a template was loaded.
    pub fn get_path(&self, id: &str) -> Option<&Path> {
        self.loaded_paths.get(id).map(|p| p.as_path())
    }

    /// Returns the number of loaded templates.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Returns true if no templates are loaded.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Returns an iterator over all template IDs.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.templates.keys().map(|s| s.as_str())
    }

    /// Returns an iterator over all loaded templates.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &TaskTemplate)> {
        self.templates.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Returns templates filtered by category.
    pub fn by_category(&self, category: &str) -> Vec<&TaskTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Returns templates filtered by subcategory.
    pub fn by_subcategory(&self, category: &str, subcategory: &str) -> Vec<&TaskTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == category && t.subcategory == subcategory)
            .collect()
    }

    /// Returns templates filtered by difficulty level.
    pub fn by_difficulty(&self, difficulty: &str) -> Vec<&TaskTemplate> {
        self.templates
            .values()
            .filter(|t| t.difficulty.estimated == difficulty)
            .collect()
    }

    /// Removes a template from the cache.
    ///
    /// Returns true if the template was present and removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let removed = self.templates.remove(id).is_some();
        self.loaded_paths.remove(id);
        removed
    }

    /// Clears all loaded templates from the cache.
    pub fn clear(&mut self) {
        self.templates.clear();
        self.loaded_paths.clear();
    }

    /// Reloads a template from its original file path.
    ///
    /// This is useful for refreshing a template after the source file has been modified.
    pub fn reload(&mut self, id: &str) -> Result<&TaskTemplate, TemplateError> {
        let path = self
            .loaded_paths
            .get(id)
            .ok_or_else(|| TemplateError::NotFound(id.to_string()))?
            .clone();

        // Remove the existing template
        self.templates.remove(id);

        // Reload from file
        self.load_file(&path)
    }

    /// Validates all loaded templates.
    ///
    /// Returns a list of (template_id, error) pairs for any templates that fail validation.
    pub fn validate_all(&self) -> Vec<(String, TemplateError)> {
        let mut errors = Vec::new();

        for (id, template) in &self.templates {
            if let Err(e) = template.validate() {
                errors.push((id.clone(), e));
            }
        }

        errors
    }

    /// Registers a template directly without loading from file.
    ///
    /// The template is validated before being added.
    pub fn register(&mut self, template: TaskTemplate) -> Result<(), TemplateError> {
        // Validate the template
        template.validate()?;

        // Check for duplicate
        if self.templates.contains_key(&template.id) {
            return Err(TemplateError::DuplicateTemplateId(template.id.clone()));
        }

        self.templates.insert(template.id.clone(), template);
        Ok(())
    }
}

/// A type alias for the Template type (TaskTemplate).
pub type Template = TaskTemplate;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    fn create_test_template_yaml() -> String {
        r#"
id: test-debug-001
version: "1.0.0"
category: debugging
subcategory: log-analysis
difficulty:
  estimated: medium
  min_score: 0.5
  max_score: 0.9
variables:
  error_count:
    type: int
    min: 1
    max: 100
    description: Number of errors to inject
instruction_template: "Find all ERROR entries in the log file."
solution_template: "grep 'ERROR' /var/log/app.log | wc -l"
generated_files:
  - path: logs/app.log
    generator: tera
expected_outputs:
  result:
    path: output.txt
    exists: true
anti_hardcoding:
  canary_locations:
    - logs/app.log:error_line
  process_validation:
    required_patterns:
      - "grep.*ERROR"
"#
        .to_string()
    }

    #[test]
    fn test_template_loader_new() {
        let loader = TemplateLoader::new();
        assert!(loader.is_empty());
        assert_eq!(loader.len(), 0);
    }

    #[test]
    fn test_load_single_file() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        writeln!(file, "{}", create_test_template_yaml()).expect("failed to write");

        let mut loader = TemplateLoader::new();
        let template = loader.load_file(file.path()).expect("failed to load");

        assert_eq!(template.id, "test-debug-001");
        assert_eq!(template.category, "debugging");
        assert_eq!(loader.len(), 1);
    }

    #[test]
    fn test_load_directory() {
        let dir = tempdir().expect("failed to create temp dir");

        // Create two template files
        let file1_path = dir.path().join("template1.yaml");
        let file2_path = dir.path().join("template2.yaml");

        let yaml1 = r#"
id: template-001
version: "1.0.0"
category: security
subcategory: audit
difficulty:
  estimated: easy
  min_score: 0.8
  max_score: 1.0
instruction_template: "Check permissions"
solution_template: "ls -la"
"#;

        let yaml2 = r#"
id: template-002
version: "1.0.0"
category: debugging
subcategory: crash
difficulty:
  estimated: hard
  min_score: 0.0
  max_score: 0.7
instruction_template: "Debug crash"
solution_template: "gdb ./app"
"#;

        fs::write(&file1_path, yaml1).expect("failed to write file1");
        fs::write(&file2_path, yaml2).expect("failed to write file2");

        let mut loader = TemplateLoader::new();
        let count = loader
            .load_directory(dir.path())
            .expect("failed to load dir");

        assert_eq!(count, 2);
        assert_eq!(loader.len(), 2);
        assert!(loader.get("template-001").is_ok());
        assert!(loader.get("template-002").is_ok());
    }

    #[test]
    fn test_get_not_found() {
        let loader = TemplateLoader::new();
        let result = loader.get("nonexistent");
        assert!(matches!(result, Err(TemplateError::NotFound(_))));
    }

    #[test]
    fn test_duplicate_template_id() {
        let mut loader = TemplateLoader::new();

        let mut file1 = NamedTempFile::new().expect("failed to create temp file");
        let mut file2 = NamedTempFile::new().expect("failed to create temp file");

        writeln!(file1, "{}", create_test_template_yaml()).expect("failed to write");
        writeln!(file2, "{}", create_test_template_yaml()).expect("failed to write");

        loader
            .load_file(file1.path())
            .expect("failed to load first");
        let result = loader.load_file(file2.path());

        assert!(matches!(result, Err(TemplateError::DuplicateTemplateId(_))));
    }

    #[test]
    fn test_by_category() {
        let mut loader = TemplateLoader::new();

        let template1 = TaskTemplate::new(
            "task-1",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        let template2 = TaskTemplate::new(
            "task-2",
            "1.0.0",
            "security",
            "audit",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );
        let template3 = TaskTemplate::new(
            "task-3",
            "1.0.0",
            "debugging",
            "crash",
            DifficultyConfig::hard(),
            "Instructions",
            "Solution",
        );

        loader.register(template1).expect("failed to register");
        loader.register(template2).expect("failed to register");
        loader.register(template3).expect("failed to register");

        let debugging = loader.by_category("debugging");
        assert_eq!(debugging.len(), 2);

        let security = loader.by_category("security");
        assert_eq!(security.len(), 1);
    }

    #[test]
    fn test_by_difficulty() {
        let mut loader = TemplateLoader::new();

        let template1 = TaskTemplate::new(
            "task-1",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        let template2 = TaskTemplate::new(
            "task-2",
            "1.0.0",
            "debugging",
            "crash",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );

        loader.register(template1).expect("failed to register");
        loader.register(template2).expect("failed to register");

        let easy = loader.by_difficulty("easy");
        assert_eq!(easy.len(), 1);

        let medium = loader.by_difficulty("medium");
        assert_eq!(medium.len(), 1);
    }

    #[test]
    fn test_remove() {
        let mut loader = TemplateLoader::new();

        let template = TaskTemplate::new(
            "task-to-remove",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        loader.register(template).expect("failed to register");

        assert!(loader.get("task-to-remove").is_ok());
        assert!(loader.remove("task-to-remove"));
        assert!(loader.get("task-to-remove").is_err());
        assert!(!loader.remove("task-to-remove")); // Already removed
    }

    #[test]
    fn test_clear() {
        let mut loader = TemplateLoader::new();

        let template = TaskTemplate::new(
            "task-1",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        loader.register(template).expect("failed to register");

        assert_eq!(loader.len(), 1);
        loader.clear();
        assert!(loader.is_empty());
    }

    #[test]
    fn test_ids_iterator() {
        let mut loader = TemplateLoader::new();

        let template1 = TaskTemplate::new(
            "task-a",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        let template2 = TaskTemplate::new(
            "task-b",
            "1.0.0",
            "security",
            "audit",
            DifficultyConfig::medium(),
            "Instructions",
            "Solution",
        );

        loader.register(template1).expect("failed to register");
        loader.register(template2).expect("failed to register");

        let ids: Vec<&str> = loader.ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"task-a"));
        assert!(ids.contains(&"task-b"));
    }

    #[test]
    fn test_validate_all() {
        let mut loader = TemplateLoader::new();

        let template = TaskTemplate::new(
            "valid-task",
            "1.0.0",
            "debugging",
            "log",
            DifficultyConfig::easy(),
            "Instructions",
            "Solution",
        );
        loader.register(template).expect("failed to register");

        let errors = loader.validate_all();
        assert!(errors.is_empty());
    }
}
