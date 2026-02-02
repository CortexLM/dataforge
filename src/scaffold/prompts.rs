//! System prompts for the scaffold agent.
//!
//! This module contains the system prompts used by the agent loop to
//! instruct the LLM on how to use the available tools.

/// Main system prompt for the agent.
pub const AGENT_SYSTEM_PROMPT: &str = r#"You are an autonomous agent that can interact with a Linux environment through tool calls. Your goal is to complete the assigned task efficiently and correctly.

## Available Tools

You have access to the following tools:

1. **bash** - Execute shell commands
   - Use for running programs, installing packages, checking status, etc.
   - Commands run in the container's working directory
   - Has a configurable timeout (default: 30s)

2. **read_file** - Read file contents
   - Returns file content with line numbers
   - Can read specific line ranges with start_line and end_line
   - Maximum file size: 1MB

3. **write_file** - Create or overwrite files
   - Creates parent directories automatically
   - Use for creating new files or completely replacing existing ones
   - Maximum content size: 1MB

4. **edit_file** - Edit existing files
   - Find and replace specific content
   - old_content must match exactly (including whitespace)
   - Use for making targeted changes to existing files

5. **search** - Search for patterns in files
   - Uses ripgrep if available, otherwise grep
   - Returns matching lines with file paths and line numbers
   - Can filter by file type and search recursively

## Guidelines

1. **Think before acting**: Plan your approach before executing commands.

2. **Verify your work**: After making changes, verify they were applied correctly.

3. **Handle errors gracefully**: If a command fails, analyze the error and try a different approach.

4. **Be efficient**: Minimize unnecessary tool calls. Combine operations when possible.

5. **Read before writing**: When editing files, first read the relevant section to understand the context.

6. **Use appropriate tools**:
   - Use `bash` for running commands and scripts
   - Use `read_file` to examine file contents
   - Use `write_file` to create new files or completely replace existing ones
   - Use `edit_file` for targeted modifications to existing files
   - Use `search` to find patterns across multiple files

## Response Format

When you need to use a tool, call it with the appropriate parameters. After receiving the result, analyze it and decide on the next action.

When the task is complete, provide a summary of what was accomplished.

## Important Notes

- All paths are relative to the working directory unless absolute
- Commands have timeouts to prevent hanging
- Large outputs may be truncated
- File operations are atomic where possible
"#;

/// Build the system prompt with task context.
///
/// # Arguments
///
/// * `task_description` - Description of the task to complete
/// * `working_dir` - The working directory in the container
///
/// # Returns
///
/// The complete system prompt including task context.
pub fn build_system_prompt(task_description: &str, working_dir: &str) -> String {
    format!(
        r#"{AGENT_SYSTEM_PROMPT}

## Current Task

{task_description}

## Environment

- Working directory: {working_dir}
- Shell: /bin/sh
"#
    )
}

/// Build the tool definitions prompt for LLM function calling.
///
/// # Arguments
///
/// * `tools_json` - JSON array of tool definitions
///
/// # Returns
///
/// A formatted string describing available tools.
pub fn build_tool_prompt(tools_json: &serde_json::Value) -> String {
    let tools = tools_json
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|tool| {
                    let function = tool.get("function")?;
                    let name = function.get("name")?.as_str()?;
                    let description = function.get("description")?.as_str()?;
                    let params = function.get("parameters")?;

                    Some(format!(
                        "### {}\n\n{}\n\nParameters:\n```json\n{}\n```",
                        name,
                        description,
                        serde_json::to_string_pretty(params).unwrap_or_default()
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        })
        .unwrap_or_default();

    format!("## Tool Definitions\n\n{}", tools)
}

/// Prompt for asking the agent to summarize its work.
pub const SUMMARY_PROMPT: &str = r#"Please provide a brief summary of the work you completed:

1. What was the main task?
2. What steps did you take?
3. What was the final result?
4. Were there any issues or notes to be aware of?

Keep the summary concise but informative.
"#;

/// Prompt for when the agent hits the step limit.
pub const STEP_LIMIT_PROMPT: &str = r#"You have reached the maximum number of steps allowed for this task. Please provide:

1. A summary of what you accomplished so far
2. What remains to be done
3. Any blockers or issues encountered

The task will be marked as incomplete.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_system_prompt_not_empty() {
        assert!(!AGENT_SYSTEM_PROMPT.is_empty());
        assert!(AGENT_SYSTEM_PROMPT.contains("bash"));
        assert!(AGENT_SYSTEM_PROMPT.contains("read_file"));
        assert!(AGENT_SYSTEM_PROMPT.contains("write_file"));
        assert!(AGENT_SYSTEM_PROMPT.contains("edit_file"));
        assert!(AGENT_SYSTEM_PROMPT.contains("search"));
    }

    #[test]
    fn test_build_system_prompt() {
        let prompt = build_system_prompt("Fix the bug in main.rs", "/workspace");
        assert!(prompt.contains(AGENT_SYSTEM_PROMPT));
        assert!(prompt.contains("Fix the bug in main.rs"));
        assert!(prompt.contains("/workspace"));
    }

    #[test]
    fn test_build_tool_prompt() {
        let tools = serde_json::json!([
            {
                "type": "function",
                "function": {
                    "name": "test_tool",
                    "description": "A test tool",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "arg1": {"type": "string"}
                        }
                    }
                }
            }
        ]);

        let prompt = build_tool_prompt(&tools);
        assert!(prompt.contains("test_tool"));
        assert!(prompt.contains("A test tool"));
        assert!(prompt.contains("arg1"));
    }

    #[test]
    fn test_build_tool_prompt_empty() {
        let tools = serde_json::json!([]);
        let prompt = build_tool_prompt(&tools);
        assert!(prompt.contains("Tool Definitions"));
    }

    #[test]
    fn test_summary_prompt_not_empty() {
        assert!(!SUMMARY_PROMPT.is_empty());
        assert!(SUMMARY_PROMPT.contains("summary"));
    }

    #[test]
    fn test_step_limit_prompt_not_empty() {
        assert!(!STEP_LIMIT_PROMPT.is_empty());
        assert!(STEP_LIMIT_PROMPT.contains("maximum"));
    }
}
