//! Test Designer Agent for creating validation tests.
//!
//! This agent designs and adapts validation tests for benchmark tasks,
//! generating test scripts and verification specifications.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::analyzer_agent::AnalyzedTask;
use super::error::{AgentError, AgentResult};
use crate::llm::{GenerationRequest, LlmProvider, Message, ToolDefinition};

/// System prompt for test design.
const TEST_DESIGN_SYSTEM_PROMPT: &str = r#"You are an expert test engineer designing automated validation tests for benchmark tasks.

Your job is to create comprehensive test specifications that verify task completion:
1. FAIL_TO_PASS tests: Tests that should initially fail and pass after the task is solved
2. PASS_TO_PASS tests: Tests that should pass both before and after (regression tests)
3. Setup commands: Commands to prepare the test environment
4. Cleanup commands: Commands to restore the environment after testing
5. A complete test.sh script that orchestrates all tests

Test Design Guidelines:
- Tests should be deterministic and reproducible
- Use exit codes to indicate pass/fail (0 = pass, non-zero = fail)
- Include timeouts to prevent hanging
- Test both the happy path and edge cases
- Avoid tests that require external network access
- Make tests idempotent when possible"#;

/// User prompt template for test design.
const TEST_DESIGN_USER_TEMPLATE: &str = r#"Design validation tests for the following benchmark task:

Category: {category}
Subcategory: {subcategory}
Difficulty: {difficulty}

Task Title: {title}

Task Description:
{description}

Technical Context:
{technical_context}

Required Skills: {skills}

{existing_tests_section}

Design comprehensive tests with:
- At least 2 fail-to-pass tests that verify the main task objectives
- At least 1 pass-to-pass regression test
- Appropriate setup and cleanup commands
- A complete test.sh script

Timeout for individual tests: {timeout} seconds
{regression_instruction}"#;

/// Configuration for the Test Designer Agent.
#[derive(Debug, Clone)]
pub struct TestDesignerConfig {
    /// Whether to generate a complete test script.
    pub generate_test_script: bool,
    /// Whether to include regression tests (pass-to-pass).
    pub include_regression_tests: bool,
    /// Default timeout for individual tests in seconds.
    pub timeout_seconds: u32,
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for TestDesignerConfig {
    fn default() -> Self {
        Self {
            generate_test_script: true,
            include_regression_tests: true,
            timeout_seconds: 30,
            temperature: 0.4,
            max_tokens: 2000,
        }
    }
}

impl TestDesignerConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables or disables test script generation.
    pub fn with_generate_test_script(mut self, enabled: bool) -> Self {
        self.generate_test_script = enabled;
        self
    }

    /// Enables or disables regression tests.
    pub fn with_include_regression_tests(mut self, enabled: bool) -> Self {
        self.include_regression_tests = enabled;
        self
    }

    /// Sets the default timeout for tests.
    pub fn with_timeout_seconds(mut self, timeout: u32) -> Self {
        self.timeout_seconds = timeout;
        self
    }

    /// Sets the temperature for LLM generation.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the maximum tokens for LLM response.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// A single test command with expected results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCommand {
    /// The shell command to execute.
    pub command: String,
    /// Expected exit code (0 for success).
    pub expected_exit_code: i32,
    /// Optional substring that must be present in the output.
    pub expected_output_contains: Option<String>,
    /// Timeout for this command in seconds.
    pub timeout_seconds: u32,
}

impl TestCommand {
    /// Creates a new test command.
    pub fn new(command: impl Into<String>, expected_exit_code: i32, timeout_seconds: u32) -> Self {
        Self {
            command: command.into(),
            expected_exit_code,
            expected_output_contains: None,
            timeout_seconds,
        }
    }

    /// Sets the expected output substring.
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output_contains = Some(output.into());
        self
    }

    /// Returns true if this test expects success (exit code 0).
    pub fn expects_success(&self) -> bool {
        self.expected_exit_code == 0
    }

    /// Returns true if this test has output validation.
    pub fn has_output_validation(&self) -> bool {
        self.expected_output_contains.is_some()
    }
}

/// A complete test specification for a benchmark task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSpec {
    /// Tests that should fail initially and pass after task completion.
    pub fail_to_pass: Vec<TestCommand>,
    /// Tests that should pass both before and after (regression tests).
    pub pass_to_pass: Vec<TestCommand>,
    /// Commands to run before tests to set up the environment.
    pub setup_commands: Vec<String>,
    /// Commands to run after tests to clean up.
    pub cleanup_commands: Vec<String>,
    /// Complete test.sh script content.
    pub test_script_content: String,
}

impl TestSpec {
    /// Creates a new test specification.
    pub fn new(
        fail_to_pass: Vec<TestCommand>,
        pass_to_pass: Vec<TestCommand>,
        setup_commands: Vec<String>,
        cleanup_commands: Vec<String>,
        test_script_content: impl Into<String>,
    ) -> Self {
        Self {
            fail_to_pass,
            pass_to_pass,
            setup_commands,
            cleanup_commands,
            test_script_content: test_script_content.into(),
        }
    }

    /// Returns the total number of tests.
    pub fn total_tests(&self) -> usize {
        self.fail_to_pass.len() + self.pass_to_pass.len()
    }

    /// Returns true if there are any fail-to-pass tests.
    pub fn has_fail_to_pass(&self) -> bool {
        !self.fail_to_pass.is_empty()
    }

    /// Returns true if there are any regression tests.
    pub fn has_regression_tests(&self) -> bool {
        !self.pass_to_pass.is_empty()
    }

    /// Returns true if there is setup required.
    pub fn requires_setup(&self) -> bool {
        !self.setup_commands.is_empty()
    }

    /// Returns true if there is cleanup required.
    pub fn requires_cleanup(&self) -> bool {
        !self.cleanup_commands.is_empty()
    }

    /// Returns true if the test script is non-empty.
    pub fn has_test_script(&self) -> bool {
        !self.test_script_content.is_empty()
    }

    /// Generates a default test script if one wasn't provided.
    pub fn generate_default_script(&self) -> String {
        let mut script = String::from("#!/bin/bash\nset -e\n\n");

        // Setup
        if !self.setup_commands.is_empty() {
            script.push_str("# Setup\n");
            for cmd in &self.setup_commands {
                script.push_str(&format!("{}\n", cmd));
            }
            script.push('\n');
        }

        // Fail-to-pass tests
        if !self.fail_to_pass.is_empty() {
            script.push_str("# Fail-to-pass tests\n");
            for (i, test) in self.fail_to_pass.iter().enumerate() {
                script.push_str(&format!(
                    "echo \"Running fail-to-pass test {}...\"\n",
                    i + 1
                ));
                script.push_str(&format!(
                    "timeout {} {} || true\n",
                    test.timeout_seconds, test.command
                ));
            }
            script.push('\n');
        }

        // Pass-to-pass tests
        if !self.pass_to_pass.is_empty() {
            script.push_str("# Pass-to-pass tests (regression)\n");
            for (i, test) in self.pass_to_pass.iter().enumerate() {
                script.push_str(&format!(
                    "echo \"Running pass-to-pass test {}...\"\n",
                    i + 1
                ));
                script.push_str(&format!(
                    "timeout {} {}\n",
                    test.timeout_seconds, test.command
                ));
            }
            script.push('\n');
        }

        // Cleanup
        if !self.cleanup_commands.is_empty() {
            script.push_str("# Cleanup\n");
            for cmd in &self.cleanup_commands {
                script.push_str(&format!("{}\n", cmd));
            }
        }

        script.push_str("\necho \"All tests completed.\"\n");
        script
    }
}

/// Test Designer Agent that creates validation tests.
///
/// This agent designs comprehensive test specifications for benchmark tasks,
/// including fail-to-pass tests, regression tests, and complete test scripts.
pub struct TestDesignerAgent {
    llm: Arc<dyn LlmProvider>,
}

impl std::fmt::Debug for TestDesignerAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestDesignerAgent").finish_non_exhaustive()
    }
}

impl TestDesignerAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "test_designer";

    /// Creates a new test designer agent with the given LLM provider.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Designs tests for an analyzed task.
    ///
    /// # Arguments
    ///
    /// * `task` - The analyzed task to design tests for.
    /// * `existing_tests` - Optional existing tests to adapt.
    ///
    /// # Returns
    ///
    /// A `TestSpec` with the designed tests.
    pub async fn design_tests(
        &self,
        task: &AnalyzedTask,
        existing_tests: Option<Vec<String>>,
    ) -> AgentResult<TestSpec> {
        let config = TestDesignerConfig::default();
        self.design_tests_with_config(task, existing_tests, &config)
            .await
    }

    /// Designs tests for an analyzed task with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `task` - The analyzed task to design tests for.
    /// * `existing_tests` - Optional existing tests to adapt.
    /// * `config` - Configuration for test design.
    ///
    /// # Returns
    ///
    /// A `TestSpec` with the designed tests.
    pub async fn design_tests_with_config(
        &self,
        task: &AnalyzedTask,
        existing_tests: Option<Vec<String>>,
        config: &TestDesignerConfig,
    ) -> AgentResult<TestSpec> {
        let prompt = self.build_test_design_prompt(task, existing_tests.as_deref(), config);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(TEST_DESIGN_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(config.temperature)
        .with_max_tokens(config.max_tokens)
        .with_tool(ToolDefinition::function(
            "design_tests",
            "Design test specifications for a benchmark task",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "fail_to_pass": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "command": {"type": "string"},
                                "expected_exit_code": {"type": "integer"},
                                "timeout_seconds": {"type": "integer"},
                                "description": {"type": "string"}
                            },
                            "required": ["name", "command"]
                        },
                        "description": "Tests that should initially fail and pass after the task is solved"
                    },
                    "pass_to_pass": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "command": {"type": "string"},
                                "expected_exit_code": {"type": "integer"},
                                "timeout_seconds": {"type": "integer"},
                                "description": {"type": "string"}
                            },
                            "required": ["name", "command"]
                        },
                        "description": "Regression tests that should pass both before and after"
                    },
                    "setup_commands": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Commands to prepare the test environment"
                    },
                    "cleanup_commands": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Commands to clean up after testing"
                    },
                    "test_script": {
                        "type": "string",
                        "description": "Complete test.sh script content"
                    }
                },
                "required": ["fail_to_pass", "pass_to_pass"]
            }),
        ));

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_test_design_response(content, config)
    }

    /// Designs tests for multiple tasks in batch.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The analyzed tasks to design tests for.
    /// * `config` - Configuration for test design.
    ///
    /// # Returns
    ///
    /// A vector of test specifications. Failed designs are logged but not included.
    pub async fn design_tests_batch(
        &self,
        tasks: &[AnalyzedTask],
        config: &TestDesignerConfig,
    ) -> AgentResult<Vec<TestSpec>> {
        let mut test_specs = Vec::with_capacity(tasks.len());

        for task in tasks {
            match self.design_tests_with_config(task, None, config).await {
                Ok(spec) => test_specs.push(spec),
                Err(e) => {
                    tracing::warn!("Failed to design tests for '{}': {}", task.title(), e);
                }
            }
        }

        if test_specs.is_empty() && !tasks.is_empty() {
            return Err(AgentError::GenerationFailed(
                "Failed to design any tests".to_string(),
            ));
        }

        Ok(test_specs)
    }

    /// Builds the user prompt for test design.
    fn build_test_design_prompt(
        &self,
        task: &AnalyzedTask,
        existing_tests: Option<&[String]>,
        config: &TestDesignerConfig,
    ) -> String {
        let difficulty_str = match task.difficulty {
            crate::difficulty::DifficultyLevel::Easy => "Easy",
            crate::difficulty::DifficultyLevel::Medium => "Medium",
            crate::difficulty::DifficultyLevel::Hard => "Hard",
        };

        let skills_str = if task.required_skills.is_empty() {
            "Not specified".to_string()
        } else {
            task.required_skills.join(", ")
        };

        let existing_tests_section = match existing_tests {
            Some(tests) if !tests.is_empty() => {
                format!(
                    "Existing Tests (adapt these):\n```\n{}\n```\n",
                    tests.join("\n")
                )
            }
            _ => "No existing tests provided. Design new tests from scratch.".to_string(),
        };

        let regression_instruction = if config.include_regression_tests {
            "Include pass-to-pass regression tests."
        } else {
            "Do not include pass-to-pass regression tests."
        };

        TEST_DESIGN_USER_TEMPLATE
            .replace("{category}", task.category.display_name())
            .replace("{subcategory}", &task.subcategory)
            .replace("{difficulty}", difficulty_str)
            .replace("{title}", task.title())
            .replace("{description}", task.description())
            .replace("{technical_context}", &task.technical_context)
            .replace("{skills}", &skills_str)
            .replace("{existing_tests_section}", &existing_tests_section)
            .replace("{timeout}", &config.timeout_seconds.to_string())
            .replace("{regression_instruction}", regression_instruction)
    }

    /// Parses the LLM response into a TestSpec.
    fn parse_test_design_response(
        &self,
        content: &str,
        config: &TestDesignerConfig,
    ) -> AgentResult<TestSpec> {
        let parsed: TestDesignResponse = match serde_json::from_str(content.trim()) {
            Ok(v) => v,
            Err(first_err) => {
                let json_str = match self.extract_json(content) {
                    Ok(j) => j,
                    Err(_) => {
                        return Err(AgentError::ResponseParseError(format!(
                            "Failed to parse LLM response: {}",
                            first_err
                        )));
                    }
                };
                match serde_json::from_str(&json_str) {
                    Ok(v) => v,
                    Err(e) => {
                        // Last resort: try to extract commands from any JSON object
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                            if let Some(cmds) = extract_test_commands_from_value(&val) {
                                cmds
                            } else {
                                return Err(AgentError::ResponseParseError(format!(
                                    "Invalid JSON: {}",
                                    e
                                )));
                            }
                        } else {
                            return Err(AgentError::ResponseParseError(format!(
                                "Invalid JSON: {}",
                                e
                            )));
                        }
                    }
                }
            }
        };

        // Convert fail_to_pass tests
        let fail_to_pass: Vec<TestCommand> = parsed
            .fail_to_pass
            .into_iter()
            .map(|t| {
                let mut cmd = TestCommand::new(t.command, t.expected_exit_code, t.timeout_seconds);
                if let Some(output) = t.expected_output_contains {
                    cmd = cmd.with_expected_output(output);
                }
                cmd
            })
            .collect();

        // Convert pass_to_pass tests (only if configured)
        let pass_to_pass: Vec<TestCommand> = if config.include_regression_tests {
            parsed
                .pass_to_pass
                .into_iter()
                .map(|t| {
                    let mut cmd =
                        TestCommand::new(t.command, t.expected_exit_code, t.timeout_seconds);
                    if let Some(output) = t.expected_output_contains {
                        cmd = cmd.with_expected_output(output);
                    }
                    cmd
                })
                .collect()
        } else {
            Vec::new()
        };

        // Get test script content
        let test_script_content = if config.generate_test_script {
            parsed.test_script_content
        } else {
            String::new()
        };

        Ok(TestSpec::new(
            fail_to_pass,
            pass_to_pass,
            parsed.setup_commands,
            parsed.cleanup_commands,
            test_script_content,
        ))
    }

    /// Extracts JSON from the response, handling potential markdown code blocks.
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
}

/// Response structure from LLM test design - individual test.
#[derive(Debug, Deserialize)]
struct TestCommandResponse {
    command: String,
    #[serde(default)]
    expected_exit_code: i32,
    #[serde(default)]
    expected_output_contains: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_seconds: u32,
}

fn default_timeout() -> u32 {
    30
}

/// Best-effort extraction of test commands from any JSON value structure.
fn extract_test_commands_from_value(val: &serde_json::Value) -> Option<TestDesignResponse> {
    let obj = val.as_object()?;

    fn extract_cmds(val: &serde_json::Value) -> Vec<TestCommandResponse> {
        match val {
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|item| {
                    let obj = item.as_object()?;
                    let command = obj.get("command").and_then(|v| v.as_str())?.to_string();
                    Some(TestCommandResponse {
                        command,
                        expected_exit_code: obj
                            .get("expected_exit_code")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32,
                        expected_output_contains: obj
                            .get("expected_output_contains")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        timeout_seconds: obj
                            .get("timeout_seconds")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(30) as u32,
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    // Search for any key containing test commands
    let mut fail_to_pass = Vec::new();
    let mut pass_to_pass = Vec::new();

    for (key, value) in obj {
        let k = key.to_lowercase();
        if k.contains("fail") || k.contains("test") && fail_to_pass.is_empty() {
            let cmds = extract_cmds(value);
            if !cmds.is_empty() {
                fail_to_pass = cmds;
            }
        } else if k.contains("pass") || k.contains("regression") {
            let cmds = extract_cmds(value);
            if !cmds.is_empty() {
                pass_to_pass = cmds;
            }
        }
    }

    if fail_to_pass.is_empty() {
        return None;
    }

    Some(TestDesignResponse {
        fail_to_pass,
        pass_to_pass,
        setup_commands: obj
            .get("setup_commands")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        cleanup_commands: obj
            .get("cleanup_commands")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        test_script_content: obj
            .get("test_script_content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Response structure from LLM test design.
#[derive(Debug, Deserialize)]
struct TestDesignResponse {
    #[serde(
        default,
        alias = "fail_tests",
        alias = "failing_tests",
        alias = "tests_fail_to_pass"
    )]
    fail_to_pass: Vec<TestCommandResponse>,
    #[serde(
        default,
        alias = "pass_tests",
        alias = "passing_tests",
        alias = "tests_pass_to_pass"
    )]
    pass_to_pass: Vec<TestCommandResponse>,
    #[serde(default)]
    setup_commands: Vec<String>,
    #[serde(default)]
    cleanup_commands: Vec<String>,
    #[serde(default)]
    test_script_content: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::analyzer_agent::TaskCategory;
    use crate::agents::collector_agent::{CollectedTask, TaskSource};
    use crate::difficulty::DifficultyLevel;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
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
                    prompt_tokens: 200,
                    completion_tokens: 300,
                    total_tokens: 500,
                },
            })
        }
    }

    fn create_test_analyzed_task() -> AnalyzedTask {
        let task = CollectedTask::new(
            TaskSource::StackOverflow,
            "Fix file permissions",
            "Users cannot access the shared directory due to incorrect permissions...",
        );

        AnalyzedTask::new(
            task,
            TaskCategory::SystemAdmin,
            "permissions",
            DifficultyLevel::Easy,
            vec!["chmod".to_string(), "chown".to_string()],
            "Unix file permissions using chmod and chown",
            10,
            vec!["recursive permissions".to_string()],
        )
    }

    #[test]
    fn test_test_designer_config_defaults() {
        let config = TestDesignerConfig::default();
        assert!(config.generate_test_script);
        assert!(config.include_regression_tests);
        assert_eq!(config.timeout_seconds, 30);
    }

    #[test]
    fn test_test_designer_config_builder() {
        let config = TestDesignerConfig::new()
            .with_generate_test_script(false)
            .with_include_regression_tests(false)
            .with_timeout_seconds(60)
            .with_temperature(0.5)
            .with_max_tokens(3000);

        assert!(!config.generate_test_script);
        assert!(!config.include_regression_tests);
        assert_eq!(config.timeout_seconds, 60);
        assert!((config.temperature - 0.5).abs() < 0.01);
        assert_eq!(config.max_tokens, 3000);
    }

    #[test]
    fn test_test_command_creation() {
        let cmd = TestCommand::new("ls -la /tmp", 0, 10);
        assert_eq!(cmd.command, "ls -la /tmp");
        assert!(cmd.expects_success());
        assert!(!cmd.has_output_validation());

        let cmd_with_output =
            TestCommand::new("cat /etc/passwd", 0, 30).with_expected_output("root:");
        assert!(cmd_with_output.has_output_validation());
        assert_eq!(
            cmd_with_output.expected_output_contains,
            Some("root:".to_string())
        );
    }

    #[test]
    fn test_test_spec_creation() {
        let fail_tests = vec![
            TestCommand::new("test -d /shared", 0, 10),
            TestCommand::new("test -r /shared/file.txt", 0, 10),
        ];
        let pass_tests = vec![TestCommand::new("ls /tmp", 0, 5)];
        let setup = vec!["mkdir -p /test".to_string()];
        let cleanup = vec!["rm -rf /test".to_string()];

        let spec = TestSpec::new(
            fail_tests,
            pass_tests,
            setup,
            cleanup,
            "#!/bin/bash\necho test",
        );

        assert_eq!(spec.total_tests(), 3);
        assert!(spec.has_fail_to_pass());
        assert!(spec.has_regression_tests());
        assert!(spec.requires_setup());
        assert!(spec.requires_cleanup());
        assert!(spec.has_test_script());
    }

    #[test]
    fn test_test_spec_empty() {
        let spec = TestSpec::new(Vec::new(), Vec::new(), Vec::new(), Vec::new(), "");

        assert_eq!(spec.total_tests(), 0);
        assert!(!spec.has_fail_to_pass());
        assert!(!spec.has_regression_tests());
        assert!(!spec.requires_setup());
        assert!(!spec.requires_cleanup());
        assert!(!spec.has_test_script());
    }

    #[test]
    fn test_generate_default_script() {
        let fail_tests = vec![TestCommand::new("test -f /app/config.yml", 0, 10)];
        let pass_tests = vec![TestCommand::new("echo 'regression test'", 0, 5)];
        let setup = vec!["touch /app/config.yml".to_string()];
        let cleanup = vec!["rm /app/config.yml".to_string()];

        let spec = TestSpec::new(fail_tests, pass_tests, setup, cleanup, "");
        let script = spec.generate_default_script();

        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("# Setup"));
        assert!(script.contains("# Fail-to-pass tests"));
        assert!(script.contains("# Pass-to-pass tests"));
        assert!(script.contains("# Cleanup"));
        assert!(script.contains("All tests completed"));
    }

    #[tokio::test]
    async fn test_design_tests_success() {
        let mock_response = r#"{
            "fail_to_pass": [
                {
                    "command": "test -r /shared/data.txt",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 10
                },
                {
                    "command": "cat /shared/data.txt | grep 'content'",
                    "expected_exit_code": 0,
                    "expected_output_contains": "content",
                    "timeout_seconds": 15
                }
            ],
            "pass_to_pass": [
                {
                    "command": "test -d /shared",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 5
                }
            ],
            "setup_commands": ["mkdir -p /shared", "touch /shared/data.txt"],
            "cleanup_commands": ["rm -rf /shared"],
            "test_script_content": "set -e && mkdir -p /shared && test -r /shared/data.txt && rm -rf /shared"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TestDesignerAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let spec = agent
            .design_tests(&task, None)
            .await
            .expect("should succeed");

        assert_eq!(spec.fail_to_pass.len(), 2);
        assert_eq!(spec.pass_to_pass.len(), 1);
        assert_eq!(spec.setup_commands.len(), 2);
        assert_eq!(spec.cleanup_commands.len(), 1);
        assert!(spec.has_test_script());
        assert!(spec.fail_to_pass[1].has_output_validation());
    }

    #[tokio::test]
    async fn test_design_tests_with_existing_tests() {
        let mock_response = r#"{
            "fail_to_pass": [
                {
                    "command": "adapted_test.sh",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 30
                }
            ],
            "pass_to_pass": [],
            "setup_commands": [],
            "cleanup_commands": [],
            "test_script_content": "./adapted_test.sh"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TestDesignerAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let existing = vec![
            "test -f /app/config".to_string(),
            "grep pattern /app/log".to_string(),
        ];

        let spec = agent
            .design_tests(&task, Some(existing))
            .await
            .expect("should succeed");

        assert!(spec.has_fail_to_pass());
    }

    #[tokio::test]
    async fn test_design_tests_with_config_no_regression() {
        let mock_response = r#"{
            "fail_to_pass": [
                {
                    "command": "test -f /config.yml",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 10
                }
            ],
            "pass_to_pass": [
                {
                    "command": "echo regression",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 5
                }
            ],
            "setup_commands": [],
            "cleanup_commands": [],
            "test_script_content": ""
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TestDesignerAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let config = TestDesignerConfig::new()
            .with_include_regression_tests(false)
            .with_generate_test_script(false);

        let spec = agent
            .design_tests_with_config(&task, None, &config)
            .await
            .expect("should succeed");

        // Regression tests should be filtered out by config
        assert!(spec.pass_to_pass.is_empty());
        // Test script should be empty due to config
        assert!(!spec.has_test_script());
    }

    #[tokio::test]
    async fn test_design_tests_with_markdown_response() {
        let mock_response = r#"Here are the designed tests:

```json
{
    "fail_to_pass": [
        {
            "command": "test -f /file",
            "expected_exit_code": 0,
            "expected_output_contains": null,
            "timeout_seconds": 10
        }
    ],
    "pass_to_pass": [],
    "setup_commands": [],
    "cleanup_commands": [],
    "test_script_content": "test -f /file"
}
```

These tests verify file existence."#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TestDesignerAgent::new(mock_provider);

        let task = create_test_analyzed_task();
        let spec = agent
            .design_tests(&task, None)
            .await
            .expect("should succeed");

        assert!(spec.has_fail_to_pass());
    }

    #[tokio::test]
    async fn test_design_tests_batch() {
        let mock_response = r#"{
            "fail_to_pass": [
                {
                    "command": "batch_test",
                    "expected_exit_code": 0,
                    "expected_output_contains": null,
                    "timeout_seconds": 10
                }
            ],
            "pass_to_pass": [],
            "setup_commands": [],
            "cleanup_commands": [],
            "test_script_content": "echo test script"
        }"#;

        let mock_provider = Arc::new(MockLlmProvider::new(mock_response));
        let agent = TestDesignerAgent::new(mock_provider);

        let tasks = vec![create_test_analyzed_task(), create_test_analyzed_task()];

        let config = TestDesignerConfig::default();
        let specs = agent
            .design_tests_batch(&tasks, &config)
            .await
            .expect("should succeed");

        assert_eq!(specs.len(), 2);
    }

    #[test]
    fn test_agent_name_constant() {
        assert_eq!(TestDesignerAgent::AGENT_NAME, "test_designer");
    }
}
