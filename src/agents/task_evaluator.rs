//! Task Evaluator Agent for testing benchmark tasks against autonomous agents.
//!
//! This module provides a framework for evaluating how well an autonomous agent
//! can solve benchmark tasks. The agent receives ONLY the problem statement -
//! no hints, no solution, no verification information - and must solve it
//! independently.
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::agents::task_evaluator::{TaskEvaluator, EvaluationConfig, EvaluationResult};
//! use swe_forge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! // Setup LLM client
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//!
//! // Configure the evaluator
//! let config = EvaluationConfig::default();
//! let evaluator = TaskEvaluator::new(llm_client, config);
//!
//! // Evaluate a task (agent only sees the problem statement)
//! let problem = "Find all files in /var/log that contain 'ERROR' and count them.";
//! let result = evaluator.evaluate_task(problem).await?;
//!
//! println!("Success: {}", result.success);
//! println!("Duration: {:?}", result.duration);
//! println!("Steps: {}", result.steps_taken);
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::llm::{GenerationRequest, LlmProvider, Message};
use crate::utils::json_extraction::{try_extract_json_from_response, JsonExtractionError};

use super::error::{AgentError, AgentResult};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Task Evaluator.
#[derive(Debug, Clone)]
pub struct EvaluationConfig {
    /// Maximum number of steps the agent can take. Default: 50.
    pub max_steps: u32,
    /// Timeout for the entire evaluation in seconds. Default: 1200 (20 minutes).
    pub timeout_seconds: u64,
    /// Model to use for the evaluation agent. Default: empty (uses provider default).
    pub model: String,
    /// Temperature for LLM generation. Default: 0.3.
    pub temperature: f64,
    /// Maximum tokens per LLM response. Default: 2000.
    pub max_tokens: u32,
    /// Whether to allow command execution (simulated). Default: true.
    pub allow_command_execution: bool,
    /// Whether to record detailed step history. Default: true.
    pub record_step_history: bool,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        Self {
            max_steps: 50,
            timeout_seconds: 1200,
            model: String::new(),
            temperature: 0.3,
            max_tokens: 2000,
            allow_command_execution: true,
            record_step_history: true,
        }
    }
}

impl EvaluationConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of steps.
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Set the timeout in seconds.
    pub fn with_timeout_seconds(mut self, timeout: u64) -> Self {
        self.timeout_seconds = timeout;
        self
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the temperature for LLM generation.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Set the maximum tokens per response.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set whether command execution is allowed.
    pub fn with_allow_command_execution(mut self, allow: bool) -> Self {
        self.allow_command_execution = allow;
        self
    }

    /// Set whether to record step history.
    pub fn with_record_step_history(mut self, record: bool) -> Self {
        self.record_step_history = record;
        self
    }
}

// ============================================================================
// Evaluation Result Types
// ============================================================================

/// Result of evaluating a task with an autonomous agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Unique identifier for this evaluation run.
    pub id: String,
    /// Whether the agent successfully solved the task.
    pub success: bool,
    /// Duration of the evaluation.
    pub duration: Duration,
    /// Number of steps the agent took.
    pub steps_taken: u32,
    /// Final output/answer from the agent.
    pub agent_output: String,
    /// Additional notes about the evaluation (errors, observations).
    pub notes: String,
    /// Detailed history of each step taken (if recording enabled).
    pub step_history: Vec<AgentStep>,
    /// Timestamp when evaluation started.
    pub started_at: DateTime<Utc>,
    /// Timestamp when evaluation completed.
    pub completed_at: DateTime<Utc>,
    /// Reason for termination.
    pub termination_reason: TerminationReason,
}

impl EvaluationResult {
    /// Create a new successful evaluation result.
    pub fn success(
        duration: Duration,
        steps_taken: u32,
        agent_output: impl Into<String>,
        step_history: Vec<AgentStep>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            success: true,
            duration,
            steps_taken,
            agent_output: agent_output.into(),
            notes: String::new(),
            step_history,
            started_at: now - chrono::Duration::from_std(duration).unwrap_or_default(),
            completed_at: now,
            termination_reason: TerminationReason::Completed,
        }
    }

    /// Create a new failed evaluation result.
    pub fn failure(
        duration: Duration,
        steps_taken: u32,
        agent_output: impl Into<String>,
        notes: impl Into<String>,
        step_history: Vec<AgentStep>,
        reason: TerminationReason,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            success: false,
            duration,
            steps_taken,
            agent_output: agent_output.into(),
            notes: notes.into(),
            step_history,
            started_at: now - chrono::Duration::from_std(duration).unwrap_or_default(),
            completed_at: now,
            termination_reason: reason,
        }
    }

    /// Add notes to the result.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = notes.into();
        self
    }

    /// Check if the evaluation was terminated due to timeout.
    pub fn is_timeout(&self) -> bool {
        matches!(self.termination_reason, TerminationReason::Timeout)
    }

    /// Check if the evaluation hit the maximum step limit.
    pub fn is_max_steps(&self) -> bool {
        matches!(self.termination_reason, TerminationReason::MaxStepsReached)
    }
}

/// Reason why the evaluation terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationReason {
    /// Agent completed the task (successfully or not).
    Completed,
    /// Evaluation timed out.
    Timeout,
    /// Agent reached maximum step limit.
    MaxStepsReached,
    /// Agent indicated it cannot proceed.
    AgentStuck,
    /// An error occurred during evaluation.
    Error,
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::Completed => write!(f, "completed"),
            TerminationReason::Timeout => write!(f, "timeout"),
            TerminationReason::MaxStepsReached => write!(f, "max_steps_reached"),
            TerminationReason::AgentStuck => write!(f, "agent_stuck"),
            TerminationReason::Error => write!(f, "error"),
        }
    }
}

/// A single step taken by the agent during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    /// Step number (1-indexed).
    pub step_number: u32,
    /// The agent's reasoning/thought for this step.
    pub reasoning: String,
    /// Action the agent decided to take.
    pub action: AgentAction,
    /// Result/output of the action.
    pub result: String,
    /// Duration of this step.
    pub duration: Duration,
}

impl AgentStep {
    /// Create a new agent step.
    pub fn new(
        step_number: u32,
        reasoning: impl Into<String>,
        action: AgentAction,
        result: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            step_number,
            reasoning: reasoning.into(),
            action,
            result: result.into(),
            duration,
        }
    }
}

/// Actions an agent can take during task solving.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    /// Execute a command.
    ExecuteCommand { command: String },
    /// Read a file.
    ReadFile { path: String },
    /// Write content to a file.
    WriteFile { path: String, content: String },
    /// Search for files matching a pattern.
    SearchFiles { pattern: String, directory: String },
    /// Think/reason about the problem (no external action).
    Think { thought: String },
    /// Submit a final answer.
    SubmitAnswer { answer: String },
    /// Give up on the task.
    GiveUp { reason: String },
}

impl AgentAction {
    /// Get a short description of the action.
    pub fn description(&self) -> String {
        match self {
            AgentAction::ExecuteCommand { command } => {
                format!("execute: {}", truncate_string(command, 50))
            }
            AgentAction::ReadFile { path } => format!("read: {}", path),
            AgentAction::WriteFile { path, .. } => format!("write: {}", path),
            AgentAction::SearchFiles { pattern, directory } => {
                format!("search: {} in {}", pattern, directory)
            }
            AgentAction::Think { thought } => format!("think: {}", truncate_string(thought, 50)),
            AgentAction::SubmitAnswer { answer } => {
                format!("submit: {}", truncate_string(answer, 50))
            }
            AgentAction::GiveUp { reason } => format!("give_up: {}", truncate_string(reason, 50)),
        }
    }

    /// Check if this action is terminal (ends the evaluation).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentAction::SubmitAnswer { .. } | AgentAction::GiveUp { .. }
        )
    }
}

/// Truncate a string to a maximum length, adding ellipsis if needed.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

// ============================================================================
// Agent Response Parsing
// ============================================================================

/// Response structure from the evaluation agent.
#[derive(Debug, Clone, Deserialize)]
struct AgentResponse {
    /// The agent's reasoning for this step.
    reasoning: String,
    /// The action to take.
    action: ResponseAction,
    /// Whether the agent believes it has completed the task.
    #[serde(default)]
    completed: bool,
}

/// Action structure in agent response.
#[derive(Debug, Clone, Deserialize)]
struct ResponseAction {
    /// Type of action.
    #[serde(rename = "type")]
    action_type: String,
    /// Command to execute (for execute_command).
    #[serde(default)]
    command: Option<String>,
    /// File path (for read_file, write_file).
    #[serde(default)]
    path: Option<String>,
    /// Content (for write_file).
    #[serde(default)]
    content: Option<String>,
    /// Search pattern (for search_files).
    #[serde(default)]
    pattern: Option<String>,
    /// Directory (for search_files).
    #[serde(default)]
    directory: Option<String>,
    /// Thought (for think).
    #[serde(default)]
    thought: Option<String>,
    /// Answer (for submit_answer).
    #[serde(default)]
    answer: Option<String>,
    /// Reason (for give_up).
    #[serde(default)]
    reason: Option<String>,
}

impl ResponseAction {
    /// Convert to AgentAction.
    fn to_agent_action(&self) -> AgentResult<AgentAction> {
        match self.action_type.as_str() {
            "execute_command" => {
                let command = self.command.clone().ok_or_else(|| {
                    AgentError::ResponseParseError("Missing 'command' for execute_command".into())
                })?;
                Ok(AgentAction::ExecuteCommand { command })
            }
            "read_file" => {
                let path = self.path.clone().ok_or_else(|| {
                    AgentError::ResponseParseError("Missing 'path' for read_file".into())
                })?;
                Ok(AgentAction::ReadFile { path })
            }
            "write_file" => {
                let path = self.path.clone().ok_or_else(|| {
                    AgentError::ResponseParseError("Missing 'path' for write_file".into())
                })?;
                let content = self.content.clone().unwrap_or_default();
                Ok(AgentAction::WriteFile { path, content })
            }
            "search_files" => {
                let pattern = self.pattern.clone().ok_or_else(|| {
                    AgentError::ResponseParseError("Missing 'pattern' for search_files".into())
                })?;
                let directory = self.directory.clone().unwrap_or_else(|| ".".into());
                Ok(AgentAction::SearchFiles { pattern, directory })
            }
            "think" => {
                let thought = self.thought.clone().unwrap_or_default();
                Ok(AgentAction::Think { thought })
            }
            "submit_answer" => {
                let answer = self.answer.clone().ok_or_else(|| {
                    AgentError::ResponseParseError("Missing 'answer' for submit_answer".into())
                })?;
                Ok(AgentAction::SubmitAnswer { answer })
            }
            "give_up" => {
                let reason = self
                    .reason
                    .clone()
                    .unwrap_or_else(|| "No reason provided".into());
                Ok(AgentAction::GiveUp { reason })
            }
            other => Err(AgentError::ResponseParseError(format!(
                "Unknown action type: {}",
                other
            ))),
        }
    }
}

// ============================================================================
// Task Evaluator Agent
// ============================================================================

/// System prompt for the evaluation agent.
const EVALUATION_SYSTEM_PROMPT: &str = r#"You are an autonomous agent tasked with solving technical problems. You work step-by-step, taking actions and observing results until you solve the problem.

For each step, you must respond with a JSON object containing:
1. "reasoning": Your thought process for this step
2. "action": The action to take
3. "completed": true if you believe the task is complete, false otherwise

Available actions:
- execute_command: Run a shell command. {"type": "execute_command", "command": "..."}
- read_file: Read file contents. {"type": "read_file", "path": "..."}
- write_file: Create or modify a file. {"type": "write_file", "path": "...", "content": "..."}
- search_files: Find files matching a pattern. {"type": "search_files", "pattern": "...", "directory": "..."}
- think: Reason about the problem without taking action. {"type": "think", "thought": "..."}
- submit_answer: Submit your final answer when you're confident you've solved the problem. {"type": "submit_answer", "answer": "..."}
- give_up: If you cannot solve the problem after reasonable attempts. {"type": "give_up", "reason": "..."}

IMPORTANT:
- Output ONLY valid JSON, no markdown formatting or additional text
- Take one action at a time
- Observe the result before deciding the next action
- When you're confident you've solved the problem, use submit_answer
- If you're stuck after multiple attempts, use give_up rather than looping

Example response:
{"reasoning": "I need to list files in the directory first", "action": {"type": "execute_command", "command": "ls -la"}, "completed": false}"#;

/// Task Evaluator that runs an autonomous agent against benchmark tasks.
///
/// The evaluator launches an agent that receives ONLY the problem statement
/// and must solve it through reasoning and action. No hints, solutions, or
/// verification criteria are provided to the agent.
pub struct TaskEvaluator {
    /// LLM client for the agent.
    llm_client: Arc<dyn LlmProvider>,
    /// Evaluator configuration.
    config: EvaluationConfig,
}

impl TaskEvaluator {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "task_evaluator";

    /// Create a new Task Evaluator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: EvaluationConfig) -> Self {
        Self { llm_client, config }
    }

    /// Create a new Task Evaluator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, EvaluationConfig::default())
    }

    /// Evaluate a task by running an autonomous agent to solve it.
    ///
    /// The agent receives ONLY the problem statement - no hints, solution,
    /// or verification information. It must solve the task through its own
    /// reasoning and actions.
    ///
    /// # Arguments
    ///
    /// * `problem_statement` - The task description (what the agent sees)
    ///
    /// # Returns
    ///
    /// An `EvaluationResult` containing success status, duration, steps, and output.
    pub async fn evaluate_task(&self, problem_statement: &str) -> AgentResult<EvaluationResult> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        let mut conversation: Vec<Message> = vec![
            Message::system(EVALUATION_SYSTEM_PROMPT),
            Message::user(format!(
                "Please solve the following problem:\n\n{}",
                problem_statement
            )),
        ];

        let mut step_history = Vec::new();
        let mut steps_taken = 0u32;
        let mut final_output = String::new();

        loop {
            // Check timeout
            if start_time.elapsed() > timeout {
                return Ok(EvaluationResult::failure(
                    start_time.elapsed(),
                    steps_taken,
                    final_output,
                    "Evaluation timed out",
                    step_history,
                    TerminationReason::Timeout,
                ));
            }

            // Check max steps
            if steps_taken >= self.config.max_steps {
                return Ok(EvaluationResult::failure(
                    start_time.elapsed(),
                    steps_taken,
                    final_output,
                    format!("Reached maximum step limit of {}", self.config.max_steps),
                    step_history,
                    TerminationReason::MaxStepsReached,
                ));
            }

            steps_taken += 1;
            let step_start = Instant::now();

            // Get agent's next action
            let response = match self.get_agent_response(&conversation).await {
                Ok(resp) => resp,
                Err(e) => {
                    return Ok(EvaluationResult::failure(
                        start_time.elapsed(),
                        steps_taken,
                        final_output,
                        format!("Agent response error: {}", e),
                        step_history,
                        TerminationReason::Error,
                    ));
                }
            };

            // Parse the response
            let agent_response = match self.parse_agent_response(&response) {
                Ok(r) => r,
                Err(e) => {
                    // Try to continue with a recovery prompt
                    conversation.push(Message::assistant(response));
                    conversation.push(Message::user(format!(
                        "Your response was not valid JSON. Error: {}. Please respond with valid JSON containing 'reasoning', 'action', and 'completed' fields.",
                        e
                    )));
                    continue;
                }
            };

            // Convert to AgentAction
            let action = match agent_response.action.to_agent_action() {
                Ok(a) => a,
                Err(e) => {
                    conversation.push(Message::assistant(response));
                    conversation.push(Message::user(format!(
                        "Invalid action: {}. Please use one of the available actions.",
                        e
                    )));
                    continue;
                }
            };

            // Execute the action (simulated) and get result
            let action_result = self.execute_action(&action).await;

            // Record the step
            if self.config.record_step_history {
                step_history.push(AgentStep::new(
                    steps_taken,
                    &agent_response.reasoning,
                    action.clone(),
                    &action_result,
                    step_start.elapsed(),
                ));
            }

            // Check for terminal actions
            match &action {
                AgentAction::SubmitAnswer { answer } => {
                    final_output = answer.clone();
                    return Ok(EvaluationResult::success(
                        start_time.elapsed(),
                        steps_taken,
                        &final_output,
                        step_history,
                    ));
                }
                AgentAction::GiveUp { reason } => {
                    return Ok(EvaluationResult::failure(
                        start_time.elapsed(),
                        steps_taken,
                        final_output,
                        format!("Agent gave up: {}", reason),
                        step_history,
                        TerminationReason::AgentStuck,
                    ));
                }
                _ => {}
            }

            // Update conversation with this step
            conversation.push(Message::assistant(response));
            conversation.push(Message::user(format!("Result:\n{}", action_result)));

            // Check if agent believes task is complete
            if agent_response.completed && !action.is_terminal() {
                conversation.push(Message::user(
                    "You indicated the task is complete. Please submit your final answer using the submit_answer action.".to_string()
                ));
            }
        }
    }

    /// Evaluate a task with custom verification logic.
    ///
    /// This method allows providing a verification function that checks
    /// whether the agent's output is correct.
    ///
    /// # Arguments
    ///
    /// * `problem_statement` - The task description
    /// * `verifier` - Function that takes the agent's output and returns whether it's correct
    ///
    /// # Returns
    ///
    /// An `EvaluationResult` with success determined by the verifier.
    pub async fn evaluate_task_with_verification<F>(
        &self,
        problem_statement: &str,
        verifier: F,
    ) -> AgentResult<EvaluationResult>
    where
        F: Fn(&str) -> bool,
    {
        let mut result = self.evaluate_task(problem_statement).await?;

        // Only apply verification if the agent completed (didn't give up or error)
        if result.termination_reason == TerminationReason::Completed {
            result.success = verifier(&result.agent_output);
            if !result.success {
                result.notes = "Agent submitted an answer but verification failed".to_string();
            }
        }

        Ok(result)
    }

    /// Get the agent's response for the current conversation.
    async fn get_agent_response(&self, conversation: &[Message]) -> AgentResult<String> {
        let request = GenerationRequest::new(&self.config.model, conversation.to_vec())
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        response
            .first_content()
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))
    }

    /// Parse the agent's JSON response.
    fn parse_agent_response(&self, response: &str) -> AgentResult<AgentResponse> {
        let result = try_extract_json_from_response(response);
        let json_content = result.into_result_with_context(response).map_err(|e| {
            match &e {
                JsonExtractionError::Truncated { partial_preview, unclosed_braces, unclosed_brackets } => {
                    AgentError::ResponseParseError(format!(
                        "JSON appears truncated in agent response: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                        unclosed_braces, unclosed_brackets, partial_preview
                    ))
                }
                JsonExtractionError::NotFound { content_preview } => {
                    AgentError::ResponseParseError(format!(
                        "Could not extract JSON from agent response. Content starts with: '{}'",
                        content_preview
                    ))
                }
            }
        })?;

        serde_json::from_str(&json_content).map_err(|e| {
            AgentError::ResponseParseError(format!(
                "Failed to parse agent response: {}. Content: {}",
                e,
                truncate_string(&json_content, 200)
            ))
        })
    }

    /// Execute an agent action (simulated environment).
    ///
    /// In a real implementation, this would execute actual commands,
    /// read real files, etc. For evaluation purposes, this simulates
    /// the environment and returns appropriate results.
    async fn execute_action(&self, action: &AgentAction) -> String {
        match action {
            AgentAction::ExecuteCommand { command } => {
                if self.config.allow_command_execution {
                    format!(
                        "[Simulated] Command executed: {}\nOutput: Command completed successfully.",
                        command
                    )
                } else {
                    "[Command execution disabled in this evaluation]".to_string()
                }
            }
            AgentAction::ReadFile { path } => {
                format!(
                    "[Simulated] Reading file: {}\nContent: [File content would appear here]",
                    path
                )
            }
            AgentAction::WriteFile { path, content } => {
                format!("[Simulated] Wrote {} bytes to {}", content.len(), path)
            }
            AgentAction::SearchFiles { pattern, directory } => {
                format!(
                    "[Simulated] Searching for '{}' in {}:\n[Search results would appear here]",
                    pattern, directory
                )
            }
            AgentAction::Think { thought } => {
                format!("[Thought recorded]: {}", truncate_string(thought, 200))
            }
            AgentAction::SubmitAnswer { answer } => {
                format!("[Answer submitted]: {}", answer)
            }
            AgentAction::GiveUp { reason } => {
                format!("[Agent gave up]: {}", reason)
            }
        }
    }

    /// Get the evaluator configuration.
    pub fn config(&self) -> &EvaluationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use crate::utils::json_extraction::extract_json_from_response;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        responses: Mutex<Vec<String>>,
        call_count: AtomicUsize,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }

        fn single_response(response: &str) -> Self {
            Self::new(vec![response.to_string()])
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, LlmError> {
            let index = self.call_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().expect("lock poisoned");
            let content = responses
                .get(index)
                .cloned()
                .unwrap_or_else(|| responses.last().cloned().unwrap_or_default());

            Ok(GenerationResponse {
                id: format!("test-{}", index),
                model: "test-model".to_string(),
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

    fn make_submit_response(answer: &str) -> String {
        format!(
            r#"{{"reasoning": "I've determined the answer", "action": {{"type": "submit_answer", "answer": "{}"}}, "completed": true}}"#,
            answer
        )
    }

    fn make_give_up_response(reason: &str) -> String {
        format!(
            r#"{{"reasoning": "Unable to proceed", "action": {{"type": "give_up", "reason": "{}"}}, "completed": false}}"#,
            reason
        )
    }

    fn make_command_response(command: &str) -> String {
        format!(
            r#"{{"reasoning": "Executing command", "action": {{"type": "execute_command", "command": "{}"}}, "completed": false}}"#,
            command
        )
    }

    #[test]
    fn test_evaluation_config_defaults() {
        let config = EvaluationConfig::default();
        assert_eq!(config.max_steps, 50);
        assert_eq!(config.timeout_seconds, 1200);
        assert!(config.model.is_empty());
        assert!((config.temperature - 0.3).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
        assert!(config.allow_command_execution);
        assert!(config.record_step_history);
    }

    #[test]
    fn test_evaluation_config_builder() {
        let config = EvaluationConfig::new()
            .with_max_steps(50)
            .with_timeout_seconds(600)
            .with_model("gpt-4")
            .with_temperature(0.7)
            .with_max_tokens(4000)
            .with_allow_command_execution(false)
            .with_record_step_history(false);

        assert_eq!(config.max_steps, 50);
        assert_eq!(config.timeout_seconds, 600);
        assert_eq!(config.model, "gpt-4");
        assert!((config.temperature - 0.7).abs() < 0.01);
        assert_eq!(config.max_tokens, 4000);
        assert!(!config.allow_command_execution);
        assert!(!config.record_step_history);
    }

    #[test]
    fn test_temperature_clamping() {
        let config = EvaluationConfig::new().with_temperature(3.0);
        assert!((config.temperature - 2.0).abs() < 0.01);

        let config = EvaluationConfig::new().with_temperature(-1.0);
        assert!((config.temperature - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_evaluation_result_success() {
        let history = vec![AgentStep::new(
            1,
            "reasoning",
            AgentAction::SubmitAnswer {
                answer: "42".to_string(),
            },
            "submitted",
            Duration::from_secs(1),
        )];

        let result = EvaluationResult::success(Duration::from_secs(5), 1, "42", history);

        assert!(result.success);
        assert_eq!(result.steps_taken, 1);
        assert_eq!(result.agent_output, "42");
        assert_eq!(result.termination_reason, TerminationReason::Completed);
        assert!(!result.is_timeout());
        assert!(!result.is_max_steps());
    }

    #[test]
    fn test_evaluation_result_failure() {
        let result = EvaluationResult::failure(
            Duration::from_secs(300),
            20,
            "",
            "Timed out",
            vec![],
            TerminationReason::Timeout,
        );

        assert!(!result.success);
        assert!(result.is_timeout());
        assert!(!result.is_max_steps());
    }

    #[test]
    fn test_evaluation_result_max_steps() {
        let result = EvaluationResult::failure(
            Duration::from_secs(60),
            50,
            "",
            "Max steps reached",
            vec![],
            TerminationReason::MaxStepsReached,
        );

        assert!(!result.success);
        assert!(!result.is_timeout());
        assert!(result.is_max_steps());
    }

    #[test]
    fn test_agent_action_description() {
        assert!(AgentAction::ExecuteCommand {
            command: "ls".to_string()
        }
        .description()
        .contains("execute"));
        assert!(AgentAction::ReadFile {
            path: "/tmp/file".to_string()
        }
        .description()
        .contains("read"));
        assert!(AgentAction::WriteFile {
            path: "/tmp/out".to_string(),
            content: "data".to_string()
        }
        .description()
        .contains("write"));
        assert!(AgentAction::SearchFiles {
            pattern: "*.rs".to_string(),
            directory: ".".to_string()
        }
        .description()
        .contains("search"));
        assert!(AgentAction::Think {
            thought: "thinking".to_string()
        }
        .description()
        .contains("think"));
        assert!(AgentAction::SubmitAnswer {
            answer: "answer".to_string()
        }
        .description()
        .contains("submit"));
        assert!(AgentAction::GiveUp {
            reason: "stuck".to_string()
        }
        .description()
        .contains("give_up"));
    }

    #[test]
    fn test_agent_action_is_terminal() {
        assert!(!AgentAction::ExecuteCommand {
            command: "ls".to_string()
        }
        .is_terminal());
        assert!(!AgentAction::Think {
            thought: "hmm".to_string()
        }
        .is_terminal());
        assert!(AgentAction::SubmitAnswer {
            answer: "done".to_string()
        }
        .is_terminal());
        assert!(AgentAction::GiveUp {
            reason: "stuck".to_string()
        }
        .is_terminal());
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a long string", 10), "this is...");
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(TerminationReason::Completed.to_string(), "completed");
        assert_eq!(TerminationReason::Timeout.to_string(), "timeout");
        assert_eq!(
            TerminationReason::MaxStepsReached.to_string(),
            "max_steps_reached"
        );
        assert_eq!(TerminationReason::AgentStuck.to_string(), "agent_stuck");
        assert_eq!(TerminationReason::Error.to_string(), "error");
    }

    #[test]
    fn test_extract_json_from_response() {
        // Test raw JSON
        let raw = r#"{"reasoning": "test", "action": {}}"#;
        assert!(extract_json_from_response(raw).contains("reasoning"));

        // Test markdown code fence
        let markdown = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(markdown), r#"{"key": "value"}"#);

        // Test with surrounding text
        let surrounded = "Here is the JSON:\n{\"key\": \"value\"}\nEnd.";
        assert_eq!(
            extract_json_from_response(surrounded),
            r#"{"key": "value"}"#
        );
    }

    #[test]
    fn test_response_action_to_agent_action() {
        let action = ResponseAction {
            action_type: "execute_command".to_string(),
            command: Some("ls -la".to_string()),
            path: None,
            content: None,
            pattern: None,
            directory: None,
            thought: None,
            answer: None,
            reason: None,
        };
        let result = action.to_agent_action().expect("should parse");
        assert!(matches!(result, AgentAction::ExecuteCommand { .. }));

        let action = ResponseAction {
            action_type: "submit_answer".to_string(),
            command: None,
            path: None,
            content: None,
            pattern: None,
            directory: None,
            thought: None,
            answer: Some("42".to_string()),
            reason: None,
        };
        let result = action.to_agent_action().expect("should parse");
        assert!(matches!(result, AgentAction::SubmitAnswer { answer } if answer == "42"));
    }

    #[test]
    fn test_response_action_missing_required_field() {
        let action = ResponseAction {
            action_type: "execute_command".to_string(),
            command: None, // Missing!
            path: None,
            content: None,
            pattern: None,
            directory: None,
            thought: None,
            answer: None,
            reason: None,
        };
        assert!(action.to_agent_action().is_err());
    }

    #[tokio::test]
    async fn test_evaluate_task_success() {
        let mock_provider = Arc::new(MockLlmProvider::single_response(&make_submit_response(
            "The answer is 42",
        )));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let result = evaluator
            .evaluate_task("What is 6 times 7?")
            .await
            .expect("evaluation should succeed");

        assert!(result.success);
        assert_eq!(result.steps_taken, 1);
        assert!(result.agent_output.contains("42"));
        assert_eq!(result.termination_reason, TerminationReason::Completed);
    }

    #[tokio::test]
    async fn test_evaluate_task_give_up() {
        let mock_provider = Arc::new(MockLlmProvider::single_response(&make_give_up_response(
            "This is too hard",
        )));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let result = evaluator
            .evaluate_task("Prove P=NP")
            .await
            .expect("evaluation should succeed");

        assert!(!result.success);
        assert_eq!(result.termination_reason, TerminationReason::AgentStuck);
        assert!(result.notes.contains("gave up"));
    }

    #[tokio::test]
    async fn test_evaluate_task_max_steps() {
        let mock_provider = Arc::new(MockLlmProvider::single_response(&make_command_response(
            "echo step",
        )));
        let config = EvaluationConfig::new().with_max_steps(3);
        let evaluator = TaskEvaluator::new(mock_provider, config);

        let result = evaluator
            .evaluate_task("Do something complex")
            .await
            .expect("evaluation should succeed");

        assert!(!result.success);
        assert_eq!(result.steps_taken, 3);
        assert_eq!(
            result.termination_reason,
            TerminationReason::MaxStepsReached
        );
    }

    #[tokio::test]
    async fn test_evaluate_task_multi_step() {
        let responses = vec![
            make_command_response("ls"),
            make_command_response("cat file.txt"),
            make_submit_response("Found the data"),
        ];
        let mock_provider = Arc::new(MockLlmProvider::new(responses));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let result = evaluator
            .evaluate_task("Find the data in the file")
            .await
            .expect("evaluation should succeed");

        assert!(result.success);
        assert_eq!(result.steps_taken, 3);
        assert_eq!(result.step_history.len(), 3);
    }

    #[tokio::test]
    async fn test_evaluate_task_with_verification_pass() {
        let mock_provider = Arc::new(MockLlmProvider::single_response(&make_submit_response(
            "42",
        )));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let result = evaluator
            .evaluate_task_with_verification("What is 6*7?", |output| output.contains("42"))
            .await
            .expect("evaluation should succeed");

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_evaluate_task_with_verification_fail() {
        let mock_provider = Arc::new(MockLlmProvider::single_response(&make_submit_response(
            "24",
        )));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let result = evaluator
            .evaluate_task_with_verification("What is 6*7?", |output| output.contains("42"))
            .await
            .expect("evaluation should succeed");

        assert!(!result.success);
        assert!(result.notes.contains("verification failed"));
    }

    #[tokio::test]
    async fn test_step_history_recording() {
        let responses = vec![make_command_response("ls"), make_submit_response("done")];
        let mock_provider = Arc::new(MockLlmProvider::new(responses));
        let config = EvaluationConfig::new().with_record_step_history(true);
        let evaluator = TaskEvaluator::new(mock_provider, config);

        let result = evaluator
            .evaluate_task("List files")
            .await
            .expect("evaluation should succeed");

        assert_eq!(result.step_history.len(), 2);
        assert_eq!(result.step_history[0].step_number, 1);
        assert_eq!(result.step_history[1].step_number, 2);
    }

    #[tokio::test]
    async fn test_execute_action_simulated() {
        let mock_provider = Arc::new(MockLlmProvider::single_response("{}"));
        let evaluator = TaskEvaluator::with_defaults(mock_provider);

        let cmd_result = evaluator
            .execute_action(&AgentAction::ExecuteCommand {
                command: "ls".to_string(),
            })
            .await;
        assert!(cmd_result.contains("Simulated"));

        let read_result = evaluator
            .execute_action(&AgentAction::ReadFile {
                path: "/tmp/file".to_string(),
            })
            .await;
        assert!(read_result.contains("Reading file"));

        let write_result = evaluator
            .execute_action(&AgentAction::WriteFile {
                path: "/tmp/out".to_string(),
                content: "test data".to_string(),
            })
            .await;
        assert!(write_result.contains("Wrote"));
        assert!(write_result.contains("9 bytes"));
    }

    #[tokio::test]
    async fn test_command_execution_disabled() {
        let mock_provider = Arc::new(MockLlmProvider::single_response("{}"));
        let config = EvaluationConfig::new().with_allow_command_execution(false);
        let evaluator = TaskEvaluator::new(mock_provider, config);

        let result = evaluator
            .execute_action(&AgentAction::ExecuteCommand {
                command: "rm -rf /".to_string(),
            })
            .await;
        assert!(result.contains("disabled"));
    }

    #[test]
    fn test_agent_step_creation() {
        let step = AgentStep::new(
            1,
            "Need to list files",
            AgentAction::ExecuteCommand {
                command: "ls -la".to_string(),
            },
            "file1.txt\nfile2.txt",
            Duration::from_millis(150),
        );

        assert_eq!(step.step_number, 1);
        assert!(step.reasoning.contains("list files"));
        assert!(step.result.contains("file1.txt"));
        assert_eq!(step.duration, Duration::from_millis(150));
    }
}
