//! Agent execution loop for the scaffold system.
//!
//! This module implements the main agent loop that:
//! 1. Captures state
//! 2. Gets LLM action
//! 3. Parses action (tool call)
//! 4. Executes tool
//! 5. Records observation
//! 6. Checks termination

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

use super::prompts::{build_system_prompt, STEP_LIMIT_PROMPT, SUMMARY_PROMPT};
use super::tools::{ExecutionContext, ToolError, ToolRegistry, ToolResult};
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
pub enum AgentError {
    /// LLM provider error.
    #[error("LLM error: {0}")]
    LlmError(#[from] crate::error::LlmError),

    /// Tool execution error.
    #[error("Tool error: {0}")]
    ToolError(#[from] ToolError),

    /// Failed to parse LLM response.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Agent exceeded maximum steps.
    #[error("Step limit exceeded: {max_steps} steps")]
    StepLimitExceeded { max_steps: usize },

    /// Context or configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Configuration for the agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of steps the agent can take.
    pub max_steps: usize,
    /// Model to use for LLM requests.
    pub model: String,
    /// Temperature for LLM sampling.
    pub temperature: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 50,
            model: String::new(), // Use LLM provider's default
            temperature: 0.2,
            max_tokens: 4096,
        }
    }
}

impl AgentConfig {
    /// Create a new agent configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of steps.
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the temperature for sampling.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set the maximum tokens for responses.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Result of a single agent step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step number (0-indexed).
    pub step: usize,
    /// The LLM's response text.
    pub llm_response: String,
    /// Tool call if one was made.
    pub tool_call: Option<ToolCall>,
    /// Result of tool execution if a tool was called.
    pub tool_result: Option<ToolResult>,
    /// Whether the agent has finished.
    pub is_terminal: bool,
}

/// A tool call extracted from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Name of the tool to call.
    pub name: String,
    /// Arguments for the tool.
    pub arguments: Value,
}

/// Result of agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// Whether the task was completed successfully.
    pub success: bool,
    /// Summary of what the agent accomplished.
    pub summary: String,
    /// Number of steps taken.
    pub steps_taken: usize,
    /// All step results.
    pub steps: Vec<StepResult>,
    /// Final state or error message.
    pub final_message: String,
}

/// Trait for parsing tool calls from LLM responses.
pub trait ToolCallParser: Send + Sync {
    /// Parse tool calls from an LLM response.
    fn parse(&self, response: &str) -> Result<Option<ToolCall>, AgentError>;
}

/// Default tool call parser that looks for JSON function calls.
pub struct JsonToolCallParser;

impl Default for JsonToolCallParser {
    fn default() -> Self {
        Self
    }
}

impl ToolCallParser for JsonToolCallParser {
    fn parse(&self, response: &str) -> Result<Option<ToolCall>, AgentError> {
        // Look for tool call in various formats

        // Format 1: JSON with "tool" and "arguments" keys
        if let Some(call) = self.parse_json_format(response)? {
            return Ok(Some(call));
        }

        // Format 2: Function call format like tool_name(args)
        if let Some(call) = self.parse_function_format(response)? {
            return Ok(Some(call));
        }

        // Format 3: Look for code blocks with tool calls
        if let Some(call) = self.parse_code_block_format(response)? {
            return Ok(Some(call));
        }

        Ok(None)
    }
}

impl JsonToolCallParser {
    /// Parse JSON format tool calls.
    fn parse_json_format(&self, response: &str) -> Result<Option<ToolCall>, AgentError> {
        // Find JSON objects in the response
        let mut depth = 0;
        let mut start = None;

        for (i, c) in response.char_indices() {
            match c {
                '{' => {
                    if depth == 0 {
                        start = Some(i);
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(s) = start {
                            let json_str = &response[s..=i];
                            if let Ok(value) = serde_json::from_str::<Value>(json_str) {
                                // Check for tool call structure
                                if let Some(tool_name) = value.get("tool").and_then(|v| v.as_str())
                                {
                                    let arguments = value
                                        .get("arguments")
                                        .cloned()
                                        .unwrap_or(Value::Object(serde_json::Map::new()));
                                    return Ok(Some(ToolCall {
                                        name: tool_name.to_string(),
                                        arguments,
                                    }));
                                }
                                // Alternative: check for "name" and "parameters"
                                if let Some(tool_name) = value.get("name").and_then(|v| v.as_str())
                                {
                                    let arguments = value
                                        .get("parameters")
                                        .or_else(|| value.get("args"))
                                        .cloned()
                                        .unwrap_or(Value::Object(serde_json::Map::new()));
                                    return Ok(Some(ToolCall {
                                        name: tool_name.to_string(),
                                        arguments,
                                    }));
                                }
                            }
                        }
                        start = None;
                    }
                }
                _ => {}
            }
        }

        Ok(None)
    }

    /// Parse function call format like bash({"command": "ls"}).
    fn parse_function_format(&self, response: &str) -> Result<Option<ToolCall>, AgentError> {
        // Look for patterns like: tool_name({ ... })
        let tool_names = ["bash", "read_file", "write_file", "edit_file", "search"];

        for tool_name in &tool_names {
            let pattern = format!("{}(", tool_name);
            if let Some(start) = response.find(&pattern) {
                let args_start = start + pattern.len();
                let remaining = &response[args_start..];

                // Find matching closing paren
                let mut depth = 1;
                let mut end = None;
                for (i, c) in remaining.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(e) = end {
                    let args_str = &remaining[..e];
                    if let Ok(arguments) = serde_json::from_str(args_str) {
                        return Ok(Some(ToolCall {
                            name: tool_name.to_string(),
                            arguments,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Parse tool calls from code blocks.
    fn parse_code_block_format(&self, response: &str) -> Result<Option<ToolCall>, AgentError> {
        // Look for ```json blocks with tool calls
        let json_block_start = "```json";
        let block_end = "```";

        if let Some(start) = response.find(json_block_start) {
            let content_start = start + json_block_start.len();
            let remaining = &response[content_start..];
            if let Some(end) = remaining.find(block_end) {
                let json_str = remaining[..end].trim();
                if let Ok(value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(tool_name) = value.get("tool").and_then(|v| v.as_str()) {
                        let arguments = value
                            .get("arguments")
                            .cloned()
                            .unwrap_or(Value::Object(serde_json::Map::new()));
                        return Ok(Some(ToolCall {
                            name: tool_name.to_string(),
                            arguments,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Main agent execution loop.
pub struct AgentLoop {
    /// LLM provider for generating responses.
    llm_client: Arc<dyn LlmProvider>,
    /// Tool registry with available tools.
    tool_registry: ToolRegistry,
    /// Tool call parser.
    parser: Box<dyn ToolCallParser>,
    /// Agent configuration.
    config: AgentConfig,
}

impl AgentLoop {
    /// Create a new agent loop.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: AgentConfig) -> Self {
        Self {
            llm_client,
            tool_registry: ToolRegistry::with_default_tools(),
            parser: Box::new(JsonToolCallParser),
            config,
        }
    }

    /// Create an agent loop with a custom tool registry.
    pub fn with_tools(
        llm_client: Arc<dyn LlmProvider>,
        config: AgentConfig,
        tool_registry: ToolRegistry,
    ) -> Self {
        Self {
            llm_client,
            tool_registry,
            parser: Box::new(JsonToolCallParser),
            config,
        }
    }

    /// Set a custom tool call parser.
    pub fn with_parser(mut self, parser: Box<dyn ToolCallParser>) -> Self {
        self.parser = parser;
        self
    }

    /// Run the agent loop for a given task.
    ///
    /// # Arguments
    ///
    /// * `task_description` - Description of the task to complete
    /// * `ctx` - Execution context with container info
    ///
    /// # Returns
    ///
    /// The result of agent execution including all steps taken.
    pub async fn run(
        &self,
        task_description: &str,
        ctx: &ExecutionContext,
    ) -> Result<AgentResult, AgentError> {
        let mut conversation = Vec::new();
        let mut steps = Vec::new();

        // Build system prompt
        let system_prompt = build_system_prompt(task_description, &ctx.working_dir);
        conversation.push(Message::system(&system_prompt));

        // Add tool definitions to the system context
        let tools_schema = self.tool_registry.to_json_schema();
        let tools_msg = format!(
            "You have access to the following tools. To use a tool, respond with a JSON object containing 'tool' and 'arguments' keys.\n\nTools:\n{}",
            serde_json::to_string_pretty(&tools_schema).unwrap_or_default()
        );
        conversation.push(Message::user(&tools_msg));
        conversation.push(Message::assistant(
            "I understand. I will use these tools to complete the task by responding with JSON tool calls when needed.",
        ));

        // Initial task message
        conversation.push(Message::user(format!(
            "Please complete the following task:\n\n{}",
            task_description
        )));

        // Main agent loop
        let mut step = 0;
        let mut is_complete = false;

        while step < self.config.max_steps && !is_complete {
            // Get LLM response
            let request = GenerationRequest::new(self.config.model.clone(), conversation.clone())
                .with_temperature(self.config.temperature)
                .with_max_tokens(self.config.max_tokens);

            let response = self.llm_client.generate(request).await?;
            let llm_text = response
                .first_content()
                .ok_or_else(|| AgentError::ParseError("Empty LLM response".to_string()))?
                .to_string();

            // Add assistant response to conversation
            conversation.push(Message::assistant(&llm_text));

            // Parse tool call from response
            let tool_call = self.parser.parse(&llm_text)?;

            // Execute tool if present
            let (tool_result, is_terminal) = if let Some(ref call) = tool_call {
                let result = self.execute_tool(call, ctx).await;
                let is_terminal = self.check_termination(&llm_text, &result);

                // Add tool result to conversation
                let result_msg = match &result {
                    Ok(r) => {
                        if r.success {
                            format!("Tool '{}' succeeded:\n{}", call.name, r.output)
                        } else {
                            format!(
                                "Tool '{}' failed:\n{}",
                                call.name,
                                r.error.as_deref().unwrap_or("Unknown error")
                            )
                        }
                    }
                    Err(e) => format!("Tool '{}' error: {}", call.name, e),
                };
                conversation.push(Message::user(&result_msg));

                (
                    Some(result.unwrap_or_else(|e| ToolResult::failure(e.to_string()))),
                    is_terminal,
                )
            } else {
                // No tool call - check if agent indicates completion
                let is_terminal = self.check_completion_without_tool(&llm_text);
                (None, is_terminal)
            };

            // Record step
            steps.push(StepResult {
                step,
                llm_response: llm_text,
                tool_call,
                tool_result,
                is_terminal,
            });

            if is_terminal {
                is_complete = true;
            }

            step += 1;
        }

        // Get summary from agent
        let summary = if is_complete {
            self.get_summary(&mut conversation).await?
        } else {
            // Agent hit step limit
            self.get_step_limit_summary(&mut conversation).await?
        };

        Ok(AgentResult {
            success: is_complete,
            summary: summary.clone(),
            steps_taken: step,
            steps,
            final_message: summary,
        })
    }

    /// Execute a tool call.
    async fn execute_tool(
        &self,
        call: &ToolCall,
        ctx: &ExecutionContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .tool_registry
            .get(&call.name)
            .ok_or_else(|| ToolError::NotAvailable(format!("Tool '{}' not found", call.name)))?;

        tool.execute(call.arguments.clone(), ctx).await
    }

    /// Check if the agent's response indicates termination.
    fn check_termination(
        &self,
        response: &str,
        tool_result: &Result<ToolResult, ToolError>,
    ) -> bool {
        // Check for explicit completion indicators
        let completion_phrases = [
            "task is complete",
            "task completed",
            "successfully completed",
            "finished the task",
            "done with the task",
        ];

        let response_lower = response.to_lowercase();
        for phrase in &completion_phrases {
            if response_lower.contains(phrase) {
                return true;
            }
        }

        // Check if tool result indicates completion
        if let Ok(result) = tool_result {
            if result.success && response_lower.contains("final") {
                return true;
            }
        }

        false
    }

    /// Check if the agent indicates completion without a tool call.
    fn check_completion_without_tool(&self, response: &str) -> bool {
        let completion_indicators = [
            "task is complete",
            "task has been completed",
            "successfully completed the task",
            "the task is done",
            "i have finished",
            "all steps completed",
        ];

        let response_lower = response.to_lowercase();
        for indicator in &completion_indicators {
            if response_lower.contains(indicator) {
                return true;
            }
        }

        false
    }

    /// Get a summary from the agent.
    async fn get_summary(&self, conversation: &mut Vec<Message>) -> Result<String, AgentError> {
        conversation.push(Message::user(SUMMARY_PROMPT));

        let request = GenerationRequest::new(self.config.model.clone(), conversation.clone())
            .with_temperature(0.3)
            .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;
        response
            .first_content()
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::ParseError("Empty summary response".to_string()))
    }

    /// Get a summary when the step limit is reached.
    async fn get_step_limit_summary(
        &self,
        conversation: &mut Vec<Message>,
    ) -> Result<String, AgentError> {
        conversation.push(Message::user(STEP_LIMIT_PROMPT));

        let request = GenerationRequest::new(self.config.model.clone(), conversation.clone())
            .with_temperature(0.3)
            .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;
        response
            .first_content()
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::ParseError("Empty summary response".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_steps, 50);
        assert!(config.model.is_empty());
        assert!((config.temperature - 0.2).abs() < f64::EPSILON);
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_agent_config_builder() {
        let config = AgentConfig::new()
            .with_max_steps(100)
            .with_model("gpt-4")
            .with_temperature(0.5)
            .with_max_tokens(8192);

        assert_eq!(config.max_steps, 100);
        assert_eq!(config.model, "gpt-4");
        assert!((config.temperature - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.max_tokens, 8192);
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult::success("output");
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ToolResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.output, "output");
    }

    #[test]
    fn test_json_parser_json_format() {
        let parser = JsonToolCallParser;
        let response =
            r#"I will run this command: {"tool": "bash", "arguments": {"command": "ls -la"}}"#;

        let result = parser.parse(response).unwrap();
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.name, "bash");
        assert_eq!(call.arguments["command"], "ls -la");
    }

    #[test]
    fn test_json_parser_name_parameters_format() {
        let parser = JsonToolCallParser;
        let response = r#"Using tool: {"name": "read_file", "parameters": {"path": "/test.txt"}}"#;

        let result = parser.parse(response).unwrap();
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.name, "read_file");
        assert_eq!(call.arguments["path"], "/test.txt");
    }

    #[test]
    fn test_json_parser_function_format() {
        let parser = JsonToolCallParser;
        let response = r#"Running: bash({"command": "echo hello"})"#;

        let result = parser.parse(response).unwrap();
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.name, "bash");
        assert_eq!(call.arguments["command"], "echo hello");
    }

    #[test]
    fn test_json_parser_code_block_format() {
        let parser = JsonToolCallParser;
        let response = r#"
I will search for the pattern:

```json
{"tool": "search", "arguments": {"pattern": "fn main"}}
```
"#;

        let result = parser.parse(response).unwrap();
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.name, "search");
        assert_eq!(call.arguments["pattern"], "fn main");
    }

    #[test]
    fn test_json_parser_no_tool_call() {
        let parser = JsonToolCallParser;
        let response = "I'll analyze the situation first before taking action.";

        let result = parser.parse(response).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_step_result_serialization() {
        let step = StepResult {
            step: 0,
            llm_response: "test".to_string(),
            tool_call: Some(ToolCall {
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": "ls"}),
            }),
            tool_result: Some(ToolResult::success("output")),
            is_terminal: false,
        };

        let json = serde_json::to_string(&step).unwrap();
        let parsed: StepResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.step, 0);
        assert!(parsed.tool_call.is_some());
    }

    #[test]
    fn test_agent_result_serialization() {
        let result = AgentResult {
            success: true,
            summary: "Task completed".to_string(),
            steps_taken: 3,
            steps: vec![],
            final_message: "Done".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: AgentResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.steps_taken, 3);
    }
}
