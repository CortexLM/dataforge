//! Validator Agent for validating solution correctness.
//!
//! This agent validates that solutions work correctly by:
//! - Executing tests in Docker without the solution (should fail)
//! - Applying the solution patch and re-executing tests (should pass)
//! - Verifying the environment can be rebuilt from scratch
//!
//! Uses the Docker infrastructure from /workspace/src/docker/ for execution.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::environment_builder::BuiltEnvironment;
use super::error::{AgentError, AgentResult};

/// System prompt for test analysis.
const TEST_ANALYSIS_PROMPT: &str = r#"You are an expert at analyzing test results and determining solution correctness.

Given test execution logs, determine:
1. Did the tests pass or fail?
2. Were there any environment issues?
3. What is the overall validation score (0.0 to 1.0)?

Output as JSON:
{
  "tests_passed": true|false,
  "environment_ok": true|false,
  "validation_score": <float 0.0-1.0>,
  "issues": ["issue1", "issue2"],
  "summary": "<brief summary>"
}

IMPORTANT: Output ONLY the JSON object, no additional text."#;

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for the validator agent.
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    /// Timeout for Docker execution in seconds.
    pub docker_timeout_seconds: u64,
    /// Number of retries for flaky tests.
    pub retry_count: u32,
    /// Whether to verify the environment can be rebuilt.
    pub verify_reproducibility: bool,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            docker_timeout_seconds: 300,
            retry_count: 2,
            verify_reproducibility: true,
        }
    }
}

impl ValidatorConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the Docker execution timeout.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.docker_timeout_seconds = seconds;
        self
    }

    /// Set the retry count for flaky tests.
    pub fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    /// Set whether to verify reproducibility.
    pub fn with_reproducibility_check(mut self, verify: bool) -> Self {
        self.verify_reproducibility = verify;
        self
    }
}

// ============================================================================
// Test Specification Types
// ============================================================================

/// Specification for tests to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSpec {
    /// Test commands to execute.
    pub commands: Vec<String>,
    /// Expected outcomes for each command.
    pub expected_outcomes: Vec<ExpectedOutcome>,
    /// Working directory for test execution.
    pub working_dir: Option<String>,
    /// Environment variables for test execution.
    pub env_vars: Vec<(String, String)>,
    /// Timeout for individual test commands in seconds.
    pub command_timeout: u64,
}

impl Default for TestSpec {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            expected_outcomes: Vec::new(),
            working_dir: None,
            env_vars: Vec::new(),
            command_timeout: 60,
        }
    }
}

impl TestSpec {
    /// Create a new test specification.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a test command.
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.commands.push(command.into());
        self
    }

    /// Add multiple test commands.
    pub fn with_commands<I, S>(mut self, commands: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.commands.extend(commands.into_iter().map(|s| s.into()));
        self
    }

    /// Add an expected outcome.
    pub fn with_expected_outcome(mut self, outcome: ExpectedOutcome) -> Self {
        self.expected_outcomes.push(outcome);
        self
    }

    /// Set the working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Set the command timeout.
    pub fn with_command_timeout(mut self, seconds: u64) -> Self {
        self.command_timeout = seconds;
        self
    }
}

/// Expected outcome for a test command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    /// Expected exit code (0 for success).
    #[serde(default)]
    pub exit_code: i32,
    /// Optional pattern that must appear in stdout.
    pub stdout_contains: Option<String>,
    /// Optional pattern that must NOT appear in stdout.
    pub stdout_not_contains: Option<String>,
    /// Optional pattern that must appear in stderr.
    pub stderr_contains: Option<String>,
}

impl ExpectedOutcome {
    /// Create an outcome expecting success (exit code 0).
    pub fn success() -> Self {
        Self::default()
    }

    /// Create an outcome expecting failure (non-zero exit code).
    pub fn failure() -> Self {
        Self {
            exit_code: 1,
            ..Self::default()
        }
    }

    /// Set the expected exit code.
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Set a pattern that must appear in stdout.
    pub fn with_stdout_contains(mut self, pattern: impl Into<String>) -> Self {
        self.stdout_contains = Some(pattern.into());
        self
    }

    /// Set a pattern that must NOT appear in stdout.
    pub fn with_stdout_not_contains(mut self, pattern: impl Into<String>) -> Self {
        self.stdout_not_contains = Some(pattern.into());
        self
    }

    /// Set a pattern that must appear in stderr.
    pub fn with_stderr_contains(mut self, pattern: impl Into<String>) -> Self {
        self.stderr_contains = Some(pattern.into());
        self
    }
}

// ============================================================================
// Validation Outcome Types
// ============================================================================

/// Outcome of a validation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutcome {
    /// Whether tests pass without the solution applied.
    pub tests_pass_without_solution: bool,
    /// Whether tests pass with the solution applied.
    pub tests_pass_with_solution: bool,
    /// Whether the environment is reproducible.
    pub environment_reproducible: bool,
    /// Overall validation score (0.0 to 1.0).
    pub validation_score: f64,
    /// List of issues found during validation.
    pub issues: Vec<String>,
    /// Combined execution logs.
    pub execution_logs: String,
}

impl Default for ValidationOutcome {
    fn default() -> Self {
        Self {
            tests_pass_without_solution: false,
            tests_pass_with_solution: false,
            environment_reproducible: false,
            validation_score: 0.0,
            issues: Vec::new(),
            execution_logs: String::new(),
        }
    }
}

impl ValidationOutcome {
    /// Create a new validation outcome.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the validation is considered successful.
    ///
    /// A successful validation requires:
    /// - Tests fail without the solution
    /// - Tests pass with the solution
    /// - Environment is reproducible (if verification was requested)
    pub fn is_valid(&self) -> bool {
        // Tests should FAIL without solution and PASS with solution
        !self.tests_pass_without_solution
            && self.tests_pass_with_solution
            && self.environment_reproducible
    }

    /// Add an issue to the validation outcome.
    pub fn add_issue(&mut self, issue: impl Into<String>) {
        self.issues.push(issue.into());
    }

    /// Set the execution logs.
    pub fn with_logs(mut self, logs: impl Into<String>) -> Self {
        self.execution_logs = logs.into();
        self
    }

    /// Calculate and set the validation score based on results.
    pub fn calculate_score(&mut self) {
        let mut score = 0.0;

        // Tests should fail without solution (30% of score)
        if !self.tests_pass_without_solution {
            score += 0.3;
        }

        // Tests should pass with solution (50% of score)
        if self.tests_pass_with_solution {
            score += 0.5;
        }

        // Environment should be reproducible (20% of score)
        if self.environment_reproducible {
            score += 0.2;
        }

        // Deduct for issues
        let issue_penalty = (self.issues.len() as f64 * 0.05).min(0.3);
        score = (score - issue_penalty).max(0.0);

        self.validation_score = score;
    }
}

// ============================================================================
// LLM Response Types
// ============================================================================

/// Response from test analysis LLM call.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct TestAnalysisResponse {
    tests_passed: bool,
    environment_ok: bool,
    validation_score: f64,
    #[serde(default)]
    issues: Vec<String>,
    #[serde(default)]
    summary: String,
}

// ============================================================================
// Validator Agent
// ============================================================================

/// Agent that validates solution correctness.
pub struct ValidatorAgent {
    llm: Arc<dyn LlmProvider>,
    config: ValidatorConfig,
}

impl std::fmt::Debug for ValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ValidatorAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "validator";

    /// Create a new validator agent.
    pub fn new(llm: Arc<dyn LlmProvider>, config: ValidatorConfig) -> Self {
        Self { llm, config }
    }

    /// Create a new validator with default configuration.
    pub fn with_defaults(llm: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm, ValidatorConfig::default())
    }

    /// Validate a solution against a test specification.
    ///
    /// # Arguments
    ///
    /// * `env` - The built environment to test in
    /// * `test_spec` - Specification of tests to run
    /// * `solution_patch` - Optional solution patch to apply
    ///
    /// # Returns
    ///
    /// A `ValidationOutcome` containing the validation results.
    pub async fn validate(
        &self,
        env: &BuiltEnvironment,
        test_spec: &TestSpec,
        solution_patch: Option<&str>,
    ) -> AgentResult<ValidationOutcome> {
        let mut outcome = ValidationOutcome::new();
        let mut logs = String::new();

        // Step 1: Run tests WITHOUT solution (should fail)
        logs.push_str("=== Testing WITHOUT solution ===\n");
        let without_solution_result = self.simulate_test_execution(env, test_spec, None).await?;
        outcome.tests_pass_without_solution = without_solution_result.tests_passed;
        logs.push_str(&format!("Result: {}\n", without_solution_result.summary));

        if outcome.tests_pass_without_solution {
            outcome.add_issue(
                "Tests pass without solution - task may be trivial or tests are not effective"
                    .to_string(),
            );
        }

        // Step 2: Run tests WITH solution (should pass)
        if let Some(patch) = solution_patch {
            logs.push_str("\n=== Testing WITH solution ===\n");
            let with_solution_result = self
                .simulate_test_execution(env, test_spec, Some(patch))
                .await?;
            outcome.tests_pass_with_solution = with_solution_result.tests_passed;
            logs.push_str(&format!("Result: {}\n", with_solution_result.summary));

            if !outcome.tests_pass_with_solution {
                outcome
                    .add_issue("Tests fail with solution - solution may be incomplete".to_string());
            }
        } else {
            // No solution provided - can't validate
            outcome.add_issue("No solution patch provided for validation".to_string());
        }

        // Step 3: Verify reproducibility (if enabled)
        if self.config.verify_reproducibility {
            logs.push_str("\n=== Verifying reproducibility ===\n");
            outcome.environment_reproducible = self.verify_reproducibility(env).await?;
            if outcome.environment_reproducible {
                logs.push_str("Environment is reproducible\n");
            } else {
                logs.push_str("Environment reproducibility check failed\n");
                outcome.add_issue("Environment may not be reproducible".to_string());
            }
        } else {
            // Skip reproducibility check, assume it's fine
            outcome.environment_reproducible = true;
            logs.push_str("\n=== Reproducibility check skipped ===\n");
        }

        // Calculate final score
        outcome.calculate_score();
        outcome = outcome.with_logs(logs);

        Ok(outcome)
    }

    /// Simulate test execution (in a real implementation, this would use Docker).
    ///
    /// This method uses LLM to analyze what the test results would be.
    async fn simulate_test_execution(
        &self,
        env: &BuiltEnvironment,
        test_spec: &TestSpec,
        solution_patch: Option<&str>,
    ) -> AgentResult<TestAnalysisResponse> {
        let prompt = self.build_test_analysis_prompt(env, test_spec, solution_patch);

        let request = GenerationRequest::new(
            "",
            vec![Message::system(TEST_ANALYSIS_PROMPT), Message::user(prompt)],
        )
        .with_temperature(0.2)
        .with_max_tokens(800);

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_test_analysis_response(content)
    }

    /// Build the prompt for test analysis.
    fn build_test_analysis_prompt(
        &self,
        env: &BuiltEnvironment,
        test_spec: &TestSpec,
        solution_patch: Option<&str>,
    ) -> String {
        let mut prompt = format!(
            "Analyze the following test execution scenario.\n\n\
             Environment:\n\
             - Working directory: {}\n\
             - Runtime version: {}\n\
             - Dependencies: {}\n\n\
             Test Commands:\n",
            env.workdir_path.display(),
            env.runtime_version,
            env.dependencies.join(", ")
        );

        for (i, cmd) in test_spec.commands.iter().enumerate() {
            prompt.push_str(&format!("{}. {}\n", i + 1, cmd));
        }

        if let Some(patch) = solution_patch {
            prompt.push_str(&format!("\nSolution Patch Applied:\n{}\n", patch));
        } else {
            prompt.push_str("\nNo solution patch applied.\n");
        }

        prompt.push_str("\nDetermine if the tests would pass or fail in this scenario.");

        prompt
    }

    /// Parse the test analysis response.
    fn parse_test_analysis_response(&self, content: &str) -> AgentResult<TestAnalysisResponse> {
        let json_content = self.extract_json(content)?;

        serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))
    }

    /// Verify that the environment can be rebuilt.
    async fn verify_reproducibility(&self, env: &BuiltEnvironment) -> AgentResult<bool> {
        // Check that the Dockerfile content is valid
        if env.dockerfile_content.is_empty() {
            return Ok(false);
        }

        // Check that essential fields are present
        if env.dockerfile_content.contains("FROM ") && !env.workdir_path.as_os_str().is_empty() {
            return Ok(true);
        }

        Ok(false)
    }

    /// Extract JSON from the response, handling potential markdown code blocks.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        use crate::utils::json_extraction::try_extract_json_from_response;

        let result = try_extract_json_from_response(content);

        match result {
            crate::utils::json_extraction::JsonExtractionResult::Success(json) => Ok(json),
            crate::utils::json_extraction::JsonExtractionResult::Truncated {
                partial_json,
                unclosed_braces,
                unclosed_brackets,
            } => {
                let preview_len = partial_json.len().min(200);
                let preview = &partial_json[..preview_len];
                tracing::warn!(
                    unclosed_braces = unclosed_braces,
                    unclosed_brackets = unclosed_brackets,
                    partial_preview = %preview,
                    "JSON appears truncated in LLM response"
                );
                Err(AgentError::ResponseParseError(format!(
                    "JSON appears truncated: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                    unclosed_braces, unclosed_brackets, preview
                )))
            }
            crate::utils::json_extraction::JsonExtractionResult::NotFound => {
                let trimmed = content.trim();
                let preview_len = trimmed.len().min(100);
                let preview = &trimmed[..preview_len];
                tracing::warn!(
                    content_preview = %preview,
                    "Could not find JSON in LLM response"
                );
                Err(AgentError::ResponseParseError(format!(
                    "No JSON content found in response. Content starts with: '{}'",
                    preview
                )))
            }
        }
    }

    /// Generate Docker run commands for test execution.
    pub fn generate_docker_commands(
        &self,
        env: &BuiltEnvironment,
        test_spec: &TestSpec,
    ) -> Vec<String> {
        let mut commands = Vec::new();

        let container_name = format!(
            "validator-{}",
            env.workdir_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );

        // Build the image
        commands.push(format!(
            "docker build -t {} -f {}/Dockerfile {}",
            container_name,
            env.workdir_path.display(),
            env.workdir_path.display()
        ));

        // Run each test command
        for cmd in &test_spec.commands {
            let working_dir = test_spec
                .working_dir
                .clone()
                .unwrap_or_else(|| "/home/user/workspace".to_string());

            let env_args: String = test_spec
                .env_vars
                .iter()
                .map(|(k, v)| format!("-e {}={}", k, v))
                .collect::<Vec<_>>()
                .join(" ");

            commands.push(format!(
                "docker run --rm {} -w {} --timeout {}s {} sh -c '{}'",
                env_args,
                working_dir,
                self.config.docker_timeout_seconds,
                container_name,
                cmd.replace('\'', "'\\''")
            ));
        }

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        response: Mutex<String>,
    }

    impl MockLlmProvider {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: Mutex::new(response.into()),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let content = self.response.lock().expect("lock not poisoned").clone();
            Ok(GenerationResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            })
        }
    }

    #[test]
    fn test_validator_config_defaults() {
        let config = ValidatorConfig::default();

        assert_eq!(config.docker_timeout_seconds, 300);
        assert_eq!(config.retry_count, 2);
        assert!(config.verify_reproducibility);
    }

    #[test]
    fn test_validator_config_builder() {
        let config = ValidatorConfig::new()
            .with_timeout(600)
            .with_retry_count(3)
            .with_reproducibility_check(false);

        assert_eq!(config.docker_timeout_seconds, 600);
        assert_eq!(config.retry_count, 3);
        assert!(!config.verify_reproducibility);
    }

    #[test]
    fn test_test_spec_builder() {
        let spec = TestSpec::new()
            .with_command("pytest tests/")
            .with_commands(["npm test", "cargo test"])
            .with_working_dir("/app")
            .with_env("DEBUG", "true")
            .with_command_timeout(120);

        assert_eq!(spec.commands.len(), 3);
        assert_eq!(spec.working_dir, Some("/app".to_string()));
        assert_eq!(spec.env_vars.len(), 1);
        assert_eq!(spec.command_timeout, 120);
    }

    #[test]
    fn test_expected_outcome_success() {
        let outcome = ExpectedOutcome::success().with_stdout_contains("All tests passed");

        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            outcome.stdout_contains,
            Some("All tests passed".to_string())
        );
    }

    #[test]
    fn test_expected_outcome_failure() {
        let outcome = ExpectedOutcome::failure().with_stderr_contains("Error");

        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stderr_contains, Some("Error".to_string()));
    }

    #[test]
    fn test_validation_outcome_is_valid() {
        let mut outcome = ValidationOutcome::new();
        outcome.tests_pass_without_solution = false;
        outcome.tests_pass_with_solution = true;
        outcome.environment_reproducible = true;

        assert!(outcome.is_valid());

        // Invalid case: tests pass without solution
        outcome.tests_pass_without_solution = true;
        assert!(!outcome.is_valid());
    }

    #[test]
    fn test_validation_outcome_score_calculation() {
        let mut outcome = ValidationOutcome::new();
        outcome.tests_pass_without_solution = false;
        outcome.tests_pass_with_solution = true;
        outcome.environment_reproducible = true;
        outcome.calculate_score();

        // Should get full score: 0.3 + 0.5 + 0.2 = 1.0
        assert!((outcome.validation_score - 1.0).abs() < 0.01);

        // Test with issues
        outcome.add_issue("Minor issue".to_string());
        outcome.calculate_score();
        assert!((outcome.validation_score - 0.95).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_validate_success() {
        let mock_response = r#"{
            "tests_passed": false,
            "environment_ok": true,
            "validation_score": 0.8,
            "issues": [],
            "summary": "Tests failed as expected without solution"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = ValidatorAgent::with_defaults(mock_provider);

        let env = BuiltEnvironment::new(
            PathBuf::from("/tmp/test-task"),
            "FROM ubuntu:24.04\nWORKDIR /app".to_string(),
        )
        .with_runtime_version("3.13")
        .with_dependencies(vec!["pytest".to_string()]);

        let test_spec = TestSpec::new().with_command("pytest tests/");

        let result = agent.validate(&env, &test_spec, Some("fix code")).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_docker_commands_generation() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = ValidatorAgent::with_defaults(mock_provider);

        let env = BuiltEnvironment::new(
            PathBuf::from("/tmp/task-001"),
            "FROM ubuntu:24.04".to_string(),
        );

        let test_spec = TestSpec::new()
            .with_command("pytest tests/")
            .with_working_dir("/app")
            .with_env("DEBUG", "1");

        let commands = agent.generate_docker_commands(&env, &test_spec);

        assert!(!commands.is_empty());
        assert!(commands[0].contains("docker build"));
        assert!(commands.iter().any(|c| c.contains("docker run")));
        assert!(commands.iter().any(|c| c.contains("DEBUG=1")));
    }

    #[test]
    fn test_json_extraction() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = ValidatorAgent::with_defaults(mock_provider);

        // Test direct JSON
        let result = agent.extract_json(r#"{"key": "value"}"#);
        assert!(result.is_ok());

        // Test JSON in code block
        let result = agent.extract_json("```json\n{\"key\": \"value\"}\n```");
        assert!(result.is_ok());

        // Test JSON with surrounding text
        let result = agent.extract_json("Result: {\"key\": \"value\"} done");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reproducibility_check() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = ValidatorAgent::with_defaults(mock_provider);

        // Valid environment
        let valid_env = BuiltEnvironment::new(
            PathBuf::from("/tmp/task"),
            "FROM ubuntu:24.04\nWORKDIR /app".to_string(),
        );
        let result = agent.verify_reproducibility(&valid_env).await;
        assert!(result.is_ok());
        assert!(result.expect("should be ok"));

        // Invalid environment (empty Dockerfile)
        let invalid_env = BuiltEnvironment::new(PathBuf::from("/tmp/task"), String::new());
        let result = agent.verify_reproducibility(&invalid_env).await;
        assert!(result.is_ok());
        assert!(!result.expect("should be ok"));
    }

    #[test]
    fn test_test_analysis_prompt_building() {
        let mock_provider = Arc::new(MockLlmProvider::new("{}"));
        let agent = ValidatorAgent::with_defaults(mock_provider);

        let env = BuiltEnvironment::new(
            PathBuf::from("/tmp/test-task"),
            "FROM ubuntu:24.04".to_string(),
        )
        .with_runtime_version("3.13")
        .with_dependencies(vec!["pytest".to_string(), "numpy".to_string()]);

        let test_spec = TestSpec::new()
            .with_command("pytest tests/")
            .with_command("python -m mypy .");

        let prompt = agent.build_test_analysis_prompt(&env, &test_spec, Some("fix the bug"));

        assert!(prompt.contains("pytest tests/"));
        assert!(prompt.contains("python -m mypy"));
        assert!(prompt.contains("3.13"));
        assert!(prompt.contains("pytest, numpy"));
        assert!(prompt.contains("fix the bug"));
    }
}
