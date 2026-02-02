//! Search tool for finding patterns in files.
//!
//! This tool allows the agent to search for text patterns within files
//! using grep or ripgrep.

use async_trait::async_trait;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

use super::{ExecutionContext, Tool, ToolError, ToolResult};

/// Maximum number of search results to return.
const MAX_RESULTS: usize = 100;

/// Timeout for search operations in seconds.
const SEARCH_TIMEOUT: u64 = 60;

/// Parameters for the search tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchParams {
    /// Pattern to search for.
    pattern: String,
    /// Optional path to search in (defaults to working directory).
    path: Option<String>,
    /// Whether to interpret pattern as a regex (defaults to false).
    #[serde(default)]
    regex: Option<bool>,
    /// Optional file type filter (e.g., "py", "rs", "js").
    file_type: Option<String>,
    /// Whether to search recursively (defaults to true).
    #[serde(default)]
    recursive: Option<bool>,
    /// Case-insensitive search (defaults to false).
    #[serde(default)]
    ignore_case: Option<bool>,
}

/// Tool for searching patterns in files.
pub struct SearchTool {
    docker: Option<Docker>,
}

impl SearchTool {
    /// Create a new SearchTool instance.
    pub fn new() -> Self {
        let docker = Docker::connect_with_local_defaults().ok();
        Self { docker }
    }

    /// Create a SearchTool with a specific Docker client.
    pub fn with_docker(docker: Docker) -> Self {
        Self {
            docker: Some(docker),
        }
    }

    /// Check if ripgrep is available in the container.
    async fn has_ripgrep(&self, docker: &Docker, container_id: &str, working_dir: &str) -> bool {
        match Self::exec_command(docker, container_id, &["which", "rg"], working_dir).await {
            Ok((_, _, exit_code)) => exit_code == 0,
            Err(_) => false,
        }
    }

    /// Execute a command in the container.
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

        let result = timeout(Duration::from_secs(SEARCH_TIMEOUT), exec_future)
            .await
            .map_err(|_| ToolError::Timeout {
                seconds: SEARCH_TIMEOUT,
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

    /// Build the search command based on available tools and parameters.
    fn build_search_command(&self, params: &SearchParams, use_ripgrep: bool) -> String {
        let pattern = shell_escape(&params.pattern);
        let path = params.path.as_deref().unwrap_or(".");
        let is_regex = params.regex.unwrap_or(false);
        let recursive = params.recursive.unwrap_or(true);
        let ignore_case = params.ignore_case.unwrap_or(false);

        if use_ripgrep {
            let mut cmd = String::from("rg");

            cmd.push_str(" --line-number");
            cmd.push_str(" --color=never");

            if !is_regex {
                cmd.push_str(" --fixed-strings");
            }

            if ignore_case {
                cmd.push_str(" --ignore-case");
            }

            if !recursive {
                cmd.push_str(" --max-depth=1");
            }

            if let Some(ref file_type) = params.file_type {
                cmd.push_str(&format!(" --type={}", file_type));
            }

            cmd.push_str(&format!(" --max-count={}", MAX_RESULTS));
            cmd.push_str(&format!(" '{}' '{}'", pattern, path));
            cmd
        } else {
            // Fallback to grep
            let mut cmd = String::from("grep");

            cmd.push_str(" -n"); // Line numbers

            if recursive {
                cmd.push_str(" -r");
            }

            if ignore_case {
                cmd.push_str(" -i");
            }

            if !is_regex {
                cmd.push_str(" -F"); // Fixed string
            }

            // Add file extension filter via include pattern
            if let Some(ref file_type) = params.file_type {
                cmd.push_str(&format!(" --include='*.{}'", file_type));
            }

            cmd.push_str(&format!(" '{}' '{}'", pattern, path));
            cmd.push_str(&format!(" | head -n {}", MAX_RESULTS));
            cmd
        }
    }
}

impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a string for safe use in shell commands.
fn shell_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "'\\''")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search for a pattern in files. Uses ripgrep if available, otherwise falls back to grep. Returns matching lines with file paths and line numbers."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to working directory)"
                },
                "regex": {
                    "type": "boolean",
                    "description": "Whether to interpret pattern as a regex (default: false)",
                    "default": false
                },
                "file_type": {
                    "type": "string",
                    "description": "File extension to filter (e.g., 'py', 'rs', 'js')"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to search recursively (default: true)",
                    "default": true
                },
                "ignore_case": {
                    "type": "boolean",
                    "description": "Whether to ignore case (default: false)",
                    "default": false
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ExecutionContext) -> Result<ToolResult, ToolError> {
        let params: SearchParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid parameters: {}", e)))?;

        if params.pattern.is_empty() {
            return Err(ToolError::InvalidParameters(
                "Pattern cannot be empty".to_string(),
            ));
        }

        let docker = self
            .docker
            .as_ref()
            .ok_or_else(|| ToolError::DockerError("Docker client not available".to_string()))?;

        // Check for ripgrep availability
        let use_ripgrep = self
            .has_ripgrep(docker, &ctx.container_id, &ctx.working_dir)
            .await;

        // Build and execute the search command
        let command = self.build_search_command(&params, use_ripgrep);

        let (stdout, stderr, exit_code) = Self::exec_command(
            docker,
            &ctx.container_id,
            &["sh", "-c", &command],
            &ctx.working_dir,
        )
        .await?;

        // grep/rg return exit code 1 when no matches found (not an error)
        if exit_code == 0 || exit_code == 1 {
            if stdout.is_empty() {
                Ok(ToolResult::success(format!(
                    "No matches found for pattern: {}",
                    params.pattern
                )))
            } else {
                let line_count = stdout.lines().count();
                let truncated_notice = if line_count >= MAX_RESULTS {
                    format!("\n\n(Results truncated at {} matches)", MAX_RESULTS)
                } else {
                    String::new()
                };

                Ok(ToolResult::success(format!(
                    "Found {} matches:\n\n{}{}",
                    line_count, stdout, truncated_notice
                )))
            }
        } else {
            Ok(ToolResult::failure(format!(
                "Search failed (exit code {}): {}",
                exit_code, stderr
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_tool_name() {
        let tool = SearchTool::new();
        assert_eq!(tool.name(), "search");
    }

    #[test]
    fn test_search_tool_description() {
        let tool = SearchTool::new();
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("ripgrep"));
    }

    #[test]
    fn test_search_tool_parameters_schema() {
        let tool = SearchTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["pattern"].is_object());
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["regex"].is_object());
        assert!(schema["properties"]["file_type"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&Value::String("pattern".to_string())));
    }

    #[test]
    fn test_search_params_deserialization() {
        let json = serde_json::json!({
            "pattern": "fn main",
            "path": "/src",
            "regex": true,
            "file_type": "rs",
            "recursive": true,
            "ignore_case": false
        });
        let params: SearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.pattern, "fn main");
        assert_eq!(params.path, Some("/src".to_string()));
        assert_eq!(params.regex, Some(true));
        assert_eq!(params.file_type, Some("rs".to_string()));
        assert_eq!(params.recursive, Some(true));
        assert_eq!(params.ignore_case, Some(false));
    }

    #[test]
    fn test_search_params_minimal() {
        let json = serde_json::json!({
            "pattern": "hello"
        });
        let params: SearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.pattern, "hello");
        assert!(params.path.is_none());
        assert!(params.regex.is_none());
    }

    #[test]
    fn test_build_search_command_ripgrep() {
        let tool = SearchTool::new();
        let params = SearchParams {
            pattern: "test".to_string(),
            path: Some("/src".to_string()),
            regex: Some(false),
            file_type: Some("rs".to_string()),
            recursive: Some(true),
            ignore_case: Some(true),
        };

        let cmd = tool.build_search_command(&params, true);
        assert!(cmd.contains("rg"));
        assert!(cmd.contains("--fixed-strings"));
        assert!(cmd.contains("--ignore-case"));
        assert!(cmd.contains("--type=rs"));
        assert!(cmd.contains("'test'"));
        assert!(cmd.contains("'/src'"));
    }

    #[test]
    fn test_build_search_command_grep() {
        let tool = SearchTool::new();
        let params = SearchParams {
            pattern: "test".to_string(),
            path: Some("/src".to_string()),
            regex: Some(false),
            file_type: Some("py".to_string()),
            recursive: Some(true),
            ignore_case: Some(true),
        };

        let cmd = tool.build_search_command(&params, false);
        assert!(cmd.contains("grep"));
        assert!(cmd.contains("-F")); // Fixed string
        assert!(cmd.contains("-i")); // Ignore case
        assert!(cmd.contains("-r")); // Recursive
        assert!(cmd.contains("--include='*.py'"));
    }

    #[test]
    fn test_build_search_command_non_recursive() {
        let tool = SearchTool::new();
        let params = SearchParams {
            pattern: "test".to_string(),
            path: None,
            regex: None,
            file_type: None,
            recursive: Some(false),
            ignore_case: None,
        };

        let cmd_rg = tool.build_search_command(&params, true);
        assert!(cmd_rg.contains("--max-depth=1"));

        let cmd_grep = tool.build_search_command(&params, false);
        assert!(!cmd_grep.contains("-r"));
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("it's"), "it'\\''s");
        assert_eq!(shell_escape("a\\b"), "a\\\\b");
        assert_eq!(shell_escape("a\nb"), "a\\nb");
    }

    #[tokio::test]
    async fn test_search_tool_empty_pattern() {
        let tool = SearchTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({ "pattern": "" });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParameters(_))));
    }

    #[tokio::test]
    async fn test_search_tool_no_docker() {
        let tool = SearchTool { docker: None };
        let ctx = ExecutionContext::new("test", "/workspace");
        let args = serde_json::json!({ "pattern": "test" });

        let result = tool.execute(args, &ctx).await;
        assert!(matches!(result, Err(ToolError::DockerError(_))));
    }
}
