//! Task generation pipeline for synth-bench.
//!
//! This module implements the complete pipeline for generating task instances from templates:
//!
//! 1. **Parameter Sampling** - Deterministic generation of parameter values from template variables
//! 2. **Instance Generation** - Creating concrete task files from templates and parameters
//! 3. **Solution Derivation** - Computing expected outputs and verification data
//! 4. **File Generation** - Creating realistic task data files (logs, configs, etc.)
//!
//! # Example
//!
//! ```ignore
//! use synth_bench::generator::{Generator, ParameterSampler, InstanceGenerator};
//! use synth_bench::template::TaskTemplate;
//!
//! // Load a template
//! let template: TaskTemplate = load_template("log-analysis-001")?;
//!
//! // Sample parameters with a specific seed for reproducibility
//! let sampler = ParameterSampler::new(42, template.id.clone());
//! let params = sampler.sample_all(&template.variables)?;
//!
//! // Generate the task instance
//! let generator = InstanceGenerator::new(template, params);
//! let instance = generator.generate(&output_dir, 42)?;
//! ```

pub mod file_generators;
pub mod instance;
pub mod sampler;
pub mod solution;

pub use file_generators::{
    ConfigFileGenerator, DataFileGenerator, FileGenerator, LogFileGenerator,
};
pub use instance::{CanaryConfig, GeneratedInstance, InstanceGenerator};
pub use sampler::ParameterSampler;
pub use solution::{DerivedSolution, ExpectedOutputInfo, SolutionDeriver};

use crate::error::GeneratorError;
use crate::template::TaskTemplate;
use std::collections::HashMap;
use std::path::Path;

/// Result type alias for generator operations.
pub type Result<T> = std::result::Result<T, GeneratorError>;

/// High-level generator that combines all pipeline stages.
///
/// The `Generator` provides a convenient API for the complete task generation workflow:
/// parameter sampling, instance generation, and solution derivation.
pub struct Generator {
    template: TaskTemplate,
    seed: u64,
}

impl Generator {
    /// Creates a new generator for the given template and seed.
    ///
    /// # Arguments
    ///
    /// * `template` - The task template to generate instances from
    /// * `seed` - Random seed for deterministic generation
    pub fn new(template: TaskTemplate, seed: u64) -> Self {
        Self { template, seed }
    }

    /// Generates a complete task instance.
    ///
    /// This method performs all stages of the generation pipeline:
    /// 1. Samples parameters from the template variables
    /// 2. Generates task files (task.yaml, Dockerfile, solution.sh, tests/)
    /// 3. Generates task data files (logs, configs, etc.)
    /// 4. Derives the solution and expected outputs
    ///
    /// # Arguments
    ///
    /// * `output_dir` - Directory where the task instance will be created
    ///
    /// # Returns
    ///
    /// A `GeneratedInstance` containing all information about the generated task.
    pub fn generate(&self, output_dir: &Path) -> Result<GeneratedInstance> {
        // Stage 1: Sample parameters
        let sampler = ParameterSampler::new(self.seed, self.template.id.clone());
        let params = sampler.sample_all(&self.template.variables)?;

        // Stage 2: Generate instance
        let instance_generator = InstanceGenerator::new(self.template.clone(), params);
        instance_generator.generate(output_dir, self.seed)
    }

    /// Samples parameters without generating files.
    ///
    /// Useful for previewing what parameters would be generated for a given seed.
    pub fn sample_parameters(&self) -> Result<HashMap<String, serde_json::Value>> {
        let sampler = ParameterSampler::new(self.seed, self.template.id.clone());
        sampler.sample_all(&self.template.variables)
    }

    /// Derives the solution for the current parameters.
    ///
    /// # Arguments
    ///
    /// * `params` - The sampled parameters to use for derivation
    pub fn derive_solution(
        &self,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<DerivedSolution> {
        let deriver = SolutionDeriver::new(self.template.clone(), params);
        deriver.derive()
    }

    /// Returns a reference to the underlying template.
    pub fn template(&self) -> &TaskTemplate {
        &self.template
    }

    /// Returns the seed used for generation.
    pub fn seed(&self) -> u64 {
        self.seed
    }
}
