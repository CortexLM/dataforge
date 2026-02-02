//! Tool definitions and registry for the scaffold system.
//!
//! This module defines the `Tool` trait and provides a registry for managing
//! available tools that can be invoked by the LLM agent.

pub mod bash;
pub mod file;
pub mod search;

pub use bash::BashTool;
pub use file::{EditFileTool, ReadFileTool, WriteFileTool};
pub use search::SearchTool;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur during tool execution.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Invalid parameters provided to the tool.
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    /// Tool execution failed.
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    /// Tool execution timed out.
    #[error("Execution timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    /// Tool is not available in the current context.
    #[error("Tool not available: {0}")]
    NotAvailable(String),

    /// Docker-related error during execution.
    #[error("Docker error: {0}")]
    DockerError(String),

    /// File system error.
    #[error("Filesystem error: {0}")]
    FilesystemError(String),

    /// Permission denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Resource limit exceeded.
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool execution was successful.
    pub success: bool,
    /// Output from the tool execution.
    pub output: String,
    /// Error message if execution failed.
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful tool result.
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    /// Create a failed tool result.
    pub fn failure(error: impl Into<String>) -> Self {
        let error_str = error.into();
        Self {
            success: false,
            output: String::new(),
            error: Some(error_str),
        }
    }

    /// Create a result with both output and error (partial success).
    pub fn partial(output: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
            error: Some(error.into()),
        }
    }
}

/// Context for tool execution, providing access to Docker and environment.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Container ID for Docker execution.
    pub container_id: String,
    /// Working directory within the container.
    pub working_dir: String,
    /// Default timeout for commands in seconds.
    pub default_timeout: u64,
}

impl ExecutionContext {
    /// Create a new execution context.
    pub fn new(container_id: impl Into<String>, working_dir: impl Into<String>) -> Self {
        Self {
            container_id: container_id.into(),
            working_dir: working_dir.into(),
            default_timeout: 30,
        }
    }

    /// Set the default timeout for commands.
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.default_timeout = timeout_seconds;
        self
    }
}

/// Trait for tools that can be executed by the agent.
///
/// Tools provide specific capabilities to the LLM agent, such as executing
/// shell commands, reading/writing files, and searching content.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the unique name of the tool.
    fn name(&self) -> &str;

    /// Returns a description of what the tool does.
    fn description(&self) -> &str;

    /// Returns the JSON schema for the tool's parameters.
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with the given arguments and context.
    ///
    /// # Arguments
    ///
    /// * `args` - JSON object containing the tool parameters
    /// * `ctx` - Execution context with container and environment info
    ///
    /// # Returns
    ///
    /// A `ToolResult` containing the execution output or error.
    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError>;
}

/// Registry for managing available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry with the default set of tools.
    pub fn with_default_tools() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(BashTool::new()));
        registry.register(Arc::new(ReadFileTool::new()));
        registry.register(Arc::new(WriteFileTool::new()));
        registry.register(Arc::new(EditFileTool::new()));
        registry.register(Arc::new(SearchTool::new()));
        registry
    }

    /// Register a new tool in the registry.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tool names.
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Generate a JSON schema for all registered tools.
    ///
    /// Returns a JSON array of tool definitions suitable for LLM function calling.
    pub fn to_json_schema(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters_schema()
                    }
                })
            })
            .collect();

        Value::Array(tools)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("output text");
        assert!(result.success);
        assert_eq!(result.output, "output text");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_failure() {
        let result = ToolResult::failure("error message");
        assert!(!result.success);
        assert!(result.output.is_empty());
        assert_eq!(result.error, Some("error message".to_string()));
    }

    #[test]
    fn test_tool_result_partial() {
        let result = ToolResult::partial("partial output", "warning");
        assert!(!result.success);
        assert_eq!(result.output, "partial output");
        assert_eq!(result.error, Some("warning".to_string()));
    }

    #[test]
    fn test_execution_context_new() {
        let ctx = ExecutionContext::new("container-123", "/workspace");
        assert_eq!(ctx.container_id, "container-123");
        assert_eq!(ctx.working_dir, "/workspace");
        assert_eq!(ctx.default_timeout, 30);
    }

    #[test]
    fn test_execution_context_with_timeout() {
        let ctx = ExecutionContext::new("container-123", "/workspace").with_timeout(60);
        assert_eq!(ctx.default_timeout, 60);
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_tool_registry_with_default_tools() {
        let registry = ToolRegistry::with_default_tools();
        assert!(!registry.is_empty());
        assert!(registry.get("bash").is_some());
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("search").is_some());
    }

    #[test]
    fn test_tool_registry_to_json_schema() {
        let registry = ToolRegistry::with_default_tools();
        let schema = registry.to_json_schema();
        assert!(schema.is_array());
        let arr = schema.as_array().expect("schema should be an array");
        assert!(!arr.is_empty());

        // Check first tool has expected structure
        let first_tool = &arr[0];
        assert_eq!(first_tool["type"], "function");
        assert!(first_tool["function"]["name"].is_string());
        assert!(first_tool["function"]["description"].is_string());
        assert!(first_tool["function"]["parameters"].is_object());
    }
}
