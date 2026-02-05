//! File manipulation tools for reading, writing, and editing files.
//!
//! This module provides three file-related tools:
//! - `ReadFileTool`: Read file contents with optional line range
//! - `WriteFileTool`: Create or overwrite files
//! - `EditFileTool`: Find and replace content in existing files

use async_trait::async_trait;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

use super::{ExecutionContext, Tool, ToolError, ToolResult};

/// Maximum file size that can be read (1MB).
const MAX_FILE_SIZE: usize = 1_048_576;

/// Escape a path for safe use in shell commands.
/// Uses printf-based escaping to handle all special characters including single quotes.
fn escape_path_for_shell(path: &str) -> String {
    // Validate path doesn't contain null bytes
    if path.contains('\0') {
        return String::new(); // Will trigger validation error
    }

    // Use double-quoting with proper escaping for all shell special characters
    // This handles single quotes, double quotes, backslashes, $, `, etc.
    let mut escaped = String::with_capacity(path.len() * 2);
    for c in path.chars() {
        match c {
            '"' | '\\' | '$' | '`' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    format!("\"{}\"", escaped)
}

/// Validate a file path for safety.
fn validate_path(path: &str) -> Result<(), ToolError> {
    // Check for empty path
    if path.trim().is_empty() {
        return Err(ToolError::InvalidParameters(
            "Path cannot be empty".to_string(),
        ));
    }

    // Check for null bytes (could be used for injection)
    if path.contains('\0') {
        return Err(ToolError::InvalidParameters(
            "Path contains invalid null character".to_string(),
        ));
    }

    // Check for path traversal attempts outside reasonable bounds
    // Allow relative paths but prevent obvious escape attempts
    if path.contains("../../../") {
        return Err(ToolError::InvalidParameters(
            "Path contains suspicious traversal sequence".to_string(),
        ));
    }

    Ok(())
}

/// Timeout for file operations in seconds.
const FILE_OP_TIMEOUT: u64 = 30;

/// Execute a command in a container and return the output.
async fn exec_command(
    docker: &Docker,
    container_id: &str,
    command: &[&str],
    working_dir: &str,
) -> Result<(String, String, i64), ToolError> {
    let cmd_vec: Vec<&str> = command.to_vec();

    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd_vec),
        working_dir: Some(working_dir),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| ToolError::DockerError(format!("Failed to create exec: {}", e)))?;

    let exec_future = docker.start_exec(&exec.id, None);

    let result = timeout(Duration::from_secs(FILE_OP_TIMEOUT), exec_future)
        .await
        .map_err(|_| ToolError::Timeout {
            seconds: FILE_OP_TIMEOUT,
        })?
        .map_err(|e| ToolError::DockerError(format!("Failed to start exec: {}", e)))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    match result {
        StartExecResults::Attached { mut output, .. } => {
            while let Some(chunk) = output.next().await {
                match chunk {
                    Ok(bollard::container::LogOutput::StdOut { message }) => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(bollard::container::LogOutput::StdErr { message }) => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        return Err(ToolError::ExecutionFailed(format!(
                            "Error reading output: {}",
                            e
                        )));
                    }
                }
            }
        }
        StartExecResults::Detached => {
            return Err(ToolError::ExecutionFailed(
                "Unexpected detached execution".to_string(),
            ));
        }
    }

    let inspect = docker
        .inspect_exec(&exec.id)
        .await
        .map_err(|e| ToolError::DockerError(format!("Failed to inspect exec: {}", e)))?;

    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

// ============================================================================
// ReadFileTool
// ============================================================================

/// Parameters for the read_file tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReadFileParams {
    /// Path to the file to read.
    path: String,
    /// Optional starting line number (1-indexed).
    start_line: Option<u32>,
    /// Optional ending line number (1-indexed, inclusive).
    end_line: Option<u32>,
}

/// Tool for reading file contents from the container.
pub struct ReadFileTool {
    docker: Option<Docker>,
}

impl ReadFileTool {
    /// Create a new ReadFileTool instance.
    pub fn new() -> Self {
        let docker = Docker::connect_with_local_defaults().ok();
        Self { docker }
    }

    /// Create a ReadFileTool with a specific Docker client.
    pub fn with_docker(docker: Docker) -> Self {
        Self {
            docker: Some(docker),
        }
    }
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content with line numbers. Optionally specify a range of lines to read."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Starting line number (1-indexed, optional)",
                    "minimum": 1
                },
                "end_line": {
                    "type": "integer",
                    "description": "Ending line number (1-indexed, inclusive, optional)",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError> {
        let params: ReadFileParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid parameters: {}", e)))?;

        // Validate path for safety
        validate_path(&params.path)?;

        // Validate line range
        if let (Some(start), Some(end)) = (params.start_line, params.end_line) {
            if start > end {
                return Err(ToolError::InvalidParameters(format!(
                    "start_line ({}) cannot be greater than end_line ({})",
                    start, end
                )));
            }
        }

        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| ToolError::DockerError("Docker client not available".to_string()))?;

        // Escape path for safe shell usage
        let safe_path = escape_path_for_shell(&params.path);

        // Build the command based on line range
        let command = if let (Some(start), Some(end)) = (params.start_line, params.end_line) {
            format!(
                "sed -n '{},$p' {} | head -n {} | nl -ba -v {} -w 6",
                start,
                safe_path,
                end - start + 1,
                start
            )
        } else if let Some(start) = params.start_line {
            format!(
                "sed -n '{},$p' {} | nl -ba -v {} -w 6",
                start, safe_path, start
            )
        } else if let Some(end) = params.end_line {
            format!("head -n {} {} | nl -ba -w 6", end, safe_path)
        } else {
            format!("cat {} | nl -ba -w 6", safe_path)
        };

        // Check file size first
        let size_cmd = format!("stat -c %s {} 2>/dev/null || echo 0", safe_path);
        let (size_out, _, _) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &size_cmd],
            &ctx.working_dir,
        )
        .await?;

        let file_size: usize = size_out.trim().parse().unwrap_or(0);
        if file_size > MAX_FILE_SIZE {
            return Err(ToolError::ResourceLimitExceeded(format!(
                "File size ({} bytes) exceeds maximum allowed ({} bytes)",
                file_size, MAX_FILE_SIZE
            )));
        }

        // Read the file
        let (stdout, stderr, exit_code) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &command],
            &ctx.working_dir,
        )
        .await?;

        if exit_code != 0 {
            if stderr.contains("No such file") || stderr.contains("cannot open") {
                return Ok(ToolResult::failure(format!(
                    "File not found: {}",
                    params.path
                )));
            }
            return Ok(ToolResult::failure(format!(
                "Failed to read file: {}",
                stderr
            )));
        }

        Ok(ToolResult::success(stdout))
    }
}

// ============================================================================
// WriteFileTool
// ============================================================================

/// Parameters for the write_file tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteFileParams {
    /// Path to the file to write.
    path: String,
    /// Content to write to the file.
    content: String,
}

/// Tool for creating or overwriting files in the container.
pub struct WriteFileTool {
    docker: Option<Docker>,
}

impl WriteFileTool {
    /// Create a new WriteFileTool instance.
    pub fn new() -> Self {
        let docker = Docker::connect_with_local_defaults().ok();
        Self { docker }
    }

    /// Create a WriteFileTool with a specific Docker client.
    pub fn with_docker(docker: Docker) -> Self {
        Self {
            docker: Some(docker),
        }
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file with the specified content. Creates parent directories if they don't exist."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError> {
        let params: WriteFileParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid parameters: {}", e)))?;

        // Validate path for safety
        validate_path(&params.path)?;

        // Check content size
        if params.content.len() > MAX_FILE_SIZE {
            return Err(ToolError::ResourceLimitExceeded(format!(
                "Content size ({} bytes) exceeds maximum allowed ({} bytes)",
                params.content.len(),
                MAX_FILE_SIZE
            )));
        }

        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| ToolError::DockerError("Docker client not available".to_string()))?;

        // Escape paths for safe shell usage
        let safe_path = escape_path_for_shell(&params.path);

        // Create parent directories
        let dir_path = std::path::Path::new(&params.path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let safe_dir_path = escape_path_for_shell(&dir_path);

        let mkdir_cmd = format!("mkdir -p {}", safe_dir_path);
        let (_, stderr, exit_code) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &mkdir_cmd],
            &ctx.working_dir,
        )
        .await?;

        if exit_code != 0 {
            return Ok(ToolResult::failure(format!(
                "Failed to create directories: {}",
                stderr
            )));
        }

        // Write the file using base64 to handle special characters
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &params.content);
        let write_cmd = format!("echo '{}' | base64 -d > {}", encoded, safe_path);

        let (_, stderr, exit_code) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &write_cmd],
            &ctx.working_dir,
        )
        .await?;

        if exit_code != 0 {
            return Ok(ToolResult::failure(format!(
                "Failed to write file: {}",
                stderr
            )));
        }

        Ok(ToolResult::success(format!(
            "Successfully wrote {} bytes to {}",
            params.content.len(),
            params.path
        )))
    }
}

// ============================================================================
// EditFileTool
// ============================================================================

/// Parameters for the edit_file tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditFileParams {
    /// Path to the file to edit.
    path: String,
    /// Content to find and replace.
    old_content: String,
    /// New content to replace with.
    new_content: String,
}

/// Tool for editing existing files by find-and-replace.
pub struct EditFileTool {
    docker: Option<Docker>,
}

impl EditFileTool {
    /// Create a new EditFileTool instance.
    pub fn new() -> Self {
        let docker = Docker::connect_with_local_defaults().ok();
        Self { docker }
    }

    /// Create an EditFileTool with a specific Docker client.
    pub fn with_docker(docker: Docker) -> Self {
        Self {
            docker: Some(docker),
        }
    }
}

impl Default for EditFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit an existing file by finding and replacing content. The old_content must match exactly."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_content": {
                    "type": "string",
                    "description": "Exact content to find and replace"
                },
                "new_content": {
                    "type": "string",
                    "description": "New content to replace with"
                }
            },
            "required": ["path", "old_content", "new_content"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError> {
        let params: EditFileParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid parameters: {}", e)))?;

        // Validate path for safety
        validate_path(&params.path)?;

        if params.old_content.is_empty() {
            return Err(ToolError::InvalidParameters(
                "old_content cannot be empty".to_string(),
            ));
        }

        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| ToolError::DockerError("Docker client not available".to_string()))?;

        // Escape path for safe shell usage
        let safe_path = escape_path_for_shell(&params.path);

        // First, read the file
        let read_cmd = format!("cat {}", safe_path);
        let (content, stderr, exit_code) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &read_cmd],
            &ctx.working_dir,
        )
        .await?;

        if exit_code != 0 {
            return Ok(ToolResult::failure(format!(
                "Failed to read file: {}",
                stderr
            )));
        }

        // Check if old_content exists in file
        if !content.contains(&params.old_content) {
            return Ok(ToolResult::failure(format!(
                "Could not find the specified content to replace in {}. Make sure old_content matches exactly (including whitespace).",
                params.path
            )));
        }

        // Count occurrences
        let occurrence_count = content.matches(&params.old_content).count();
        if occurrence_count > 1 {
            return Ok(ToolResult::failure(format!(
                "Found {} occurrences of the specified content. Please provide a more specific old_content that matches exactly once.",
                occurrence_count
            )));
        }

        // Perform the replacement
        let new_content = content.replacen(&params.old_content, &params.new_content, 1);

        // Write back using base64 to handle special characters
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &new_content);
        let write_cmd = format!("echo '{}' | base64 -d > {}", encoded, safe_path);

        let (_, stderr, exit_code) = exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &write_cmd],
            &ctx.working_dir,
        )
        .await?;

        if exit_code != 0 {
            return Ok(ToolResult::failure(format!(
                "Failed to write edited file: {}",
                stderr
            )));
        }

        Ok(ToolResult::success(format!(
            "Successfully edited {}",
            params.path
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ReadFileTool tests
    #[test]
    fn test_read_file_tool_name() {
        let tool = ReadFileTool::new();
        assert_eq!(tool.name(), "read_file");
    }

    #[test]
    fn test_read_file_tool_schema() {
        let tool = ReadFileTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["start_line"].is_object());
        assert!(schema["properties"]["end_line"].is_object());
    }

    #[test]
    fn test_read_file_params_deserialization() {
        let json = serde_json::json!({
            "path": "/test/file.txt",
            "start_line": 10,
            "end_line": 20
        });
        let params: ReadFileParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.path, "/test/file.txt");
        assert_eq!(params.start_line, Some(10));
        assert_eq!(params.end_line, Some(20));
    }

    #[tokio::test]
    async fn test_read_file_empty_path() {
        let tool = ReadFileTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({ "path": "  " });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }

    #[tokio::test]
    async fn test_read_file_invalid_range() {
        let tool = ReadFileTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({
            "path": "/test.txt",
            "start_line": 20,
            "end_line": 10
        });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }

    // WriteFileTool tests
    #[test]
    fn test_write_file_tool_name() {
        let tool = WriteFileTool::new();
        assert_eq!(tool.name(), "write_file");
    }

    #[test]
    fn test_write_file_tool_schema() {
        let tool = WriteFileTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["content"].is_object());
    }

    #[tokio::test]
    async fn test_write_file_empty_path() {
        let tool = WriteFileTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({ "path": "", "content": "test" });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }

    // EditFileTool tests
    #[test]
    fn test_edit_file_tool_name() {
        let tool = EditFileTool::new();
        assert_eq!(tool.name(), "edit_file");
    }

    #[test]
    fn test_edit_file_tool_schema() {
        let tool = EditFileTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["old_content"].is_object());
        assert!(schema["properties"]["new_content"].is_object());
    }

    #[tokio::test]
    async fn test_edit_file_empty_old_content() {
        let tool = EditFileTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({
            "path": "/test.txt",
            "old_content": "",
            "new_content": "new"
        });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }
}
