//! Agentic test generator inspired by OpenAI Codex CLI.
//!
//! The LLM has access to `shell` and `submit_tests` function calls.
//! It clones the repo at the base commit, explores the codebase, runs tests,
//! and iteratively validates commands until submitting final results.

use anyhow::Result;
use std::process::Stdio;
use std::sync::Arc;

use crate::llm::{
    GenerationRequest, LlmProvider, Message, ToolCallInfo, ToolChoice, ToolDefinition,
};
use crate::swe::SweTask;

const MAX_AGENT_TURNS: usize = 200;

const SYSTEM_PROMPT: &str = r#"You are a test engineer validating GitHub pull requests for SWE-bench.

You have two tools:
- `shell`: execute a command in the cloned repository (at the BASE commit, before the PR).
- `submit_tests`: return your final validated test commands.

Your goal:
1. Use `shell` to explore the repo (ls, find, cat) and understand the project structure.
2. Figure out the correct build and test commands for this project.
3. Run the test suite via `shell` to confirm it passes on the base commit.
4. Design fail_to_pass tests: commands that FAIL on the base commit but PASS after the PR.
5. Design pass_to_pass tests: commands that PASS both before and after (regression tests).
6. Validate ALL commands via `shell` before submitting.

RULES:
- Every command you submit MUST be tested via `shell` first.
- fail_to_pass tests MUST fail (exit != 0) when you run them on the base commit.
- pass_to_pass tests MUST pass (exit == 0) when you run them on the base commit.
- Use the project's real test framework (pytest, cargo test, go test, npm test, etc.).
- You can target specific test files/functions, not just the whole suite.
- Call `submit_tests` when you have validated commands ready."#;

fn shell_tool() -> ToolDefinition {
    ToolDefinition::function(
        "shell",
        "Execute a shell command in the repository. Returns stdout, stderr, and exit code.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute (e.g. 'ls -la', 'cat src/main.rs', 'cargo test')"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 30000)"
                }
            },
            "required": ["command"]
        }),
    )
}

fn submit_tool() -> ToolDefinition {
    ToolDefinition::function(
        "submit_tests",
        "Submit the final validated test commands.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "fail_to_pass": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Commands that FAIL on base commit, PASS after PR"
                },
                "pass_to_pass": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Commands that PASS on both base and PR commit"
                }
            },
            "required": ["fail_to_pass", "pass_to_pass"]
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
struct ShellArgs {
    command: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}
fn default_timeout() -> u64 {
    30_000
}

#[derive(Debug, serde::Deserialize)]
struct SubmitArgs {
    #[serde(default)]
    fail_to_pass: Vec<String>,
    #[serde(default)]
    pass_to_pass: Vec<String>,
}

pub struct TestGenerator {
    llm: Arc<dyn LlmProvider>,
}

impl TestGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Run the agentic test generation loop.
    pub async fn ensure_tests(&self, task: &mut SweTask, language: &str) -> Result<()> {
        if task.has_tests() {
            return Ok(());
        }

        tracing::info!(task_id = %task.id, repo = %task.repo, "Starting agentic test generation");

        let repo_dir = self.clone_repo(&task.repo, &task.base_commit).await?;
        let repo_path = repo_dir.path().to_path_buf();

        let (build_cmds, test_cmds) = SweTask::test_commands_for_language(language);
        let patch_preview = truncate_utf8(&task.patch, 4000);

        let user_msg = format!(
            "Repository: {repo}\nLanguage: {lang}\nPR description: {prompt}\n\n\
             Suggested build: {build}\nSuggested test: {test}\n\n\
             Diff (truncated):\n```\n{patch}\n```\n\n\
             The repo is cloned at {path}. Explore it, validate test commands, then submit.",
            repo = task.repo,
            lang = language,
            prompt = truncate_utf8(&task.prompt, 1000),
            build = build_cmds.join(" && "),
            test = test_cmds.join(" && "),
            patch = patch_preview,
            path = repo_path.display(),
        );

        let tools = vec![shell_tool(), submit_tool()];
        let mut messages = vec![Message::system(SYSTEM_PROMPT), Message::user(user_msg)];

        for turn in 0..MAX_AGENT_TURNS {
            let request = GenerationRequest {
                model: String::new(),
                messages: messages.clone(),
                temperature: Some(0.2),
                max_tokens: Some(2000),
                top_p: None,
                response_format: None,
                tools: Some(tools.clone()),
                tool_choice: Some(ToolChoice::Mode("auto".to_string())),
            };

            let response = self.llm.generate(request).await?;
            let choice = match response.choices.first() {
                Some(c) => c.clone(),
                None => break,
            };

            // Check if the model made tool calls
            if let Some(ref tool_calls) = choice.message.tool_calls {
                // Re-add the assistant message with tool_calls so the API sees the full history
                messages.push(Message::assistant_with_tool_calls(
                    choice.message.content.clone(),
                    tool_calls.clone(),
                ));

                // Process each tool call
                for tc in tool_calls {
                    let result = self
                        .handle_tool_call(tc, &repo_path, &task.id, turn)
                        .await;

                    match result {
                        ToolResult::ShellOutput(output) => {
                            messages.push(Message::tool_result(&tc.id, output));
                        }
                        ToolResult::Submit(submit) => {
                            tracing::info!(
                                task_id = %task.id, turn = turn,
                                f2p = submit.fail_to_pass.len(),
                                p2p = submit.pass_to_pass.len(),
                                "Agent submitted validated tests"
                            );
                            task.fail_to_pass = submit.fail_to_pass;
                            task.pass_to_pass = submit.pass_to_pass;
                            task.meta
                                .insert("test_generation".to_string(), "agentic".to_string());
                            return Ok(());
                        }
                        ToolResult::Error(err) => {
                            messages.push(Message::tool_result(&tc.id, err));
                        }
                    }
                }
                continue;
            }

            // Model returned plain text (no tool call) -- nudge it
            if !choice.message.content.trim().is_empty() {
                messages.push(Message::assistant(choice.message.content.clone()));
                messages.push(Message::user(
                    "Use the `shell` tool to explore the repo and run tests, then call `submit_tests`."
                ));
                continue;
            }

            break;
        }

        anyhow::bail!(
            "Agentic test generation failed for {}: agent exhausted {} turns without submitting tests",
            task.id, MAX_AGENT_TURNS
        )
    }

    async fn handle_tool_call(
        &self,
        tc: &ToolCallInfo,
        repo_path: &std::path::Path,
        task_id: &str,
        turn: usize,
    ) -> ToolResult {
        match tc.function.name.as_str() {
            "shell" => {
                let args: ShellArgs = match serde_json::from_str(&tc.function.arguments) {
                    Ok(a) => a,
                    Err(e) => return ToolResult::Error(format!("Invalid shell args: {}", e)),
                };
                let result =
                    execute_shell(&args.command, repo_path, args.timeout_ms).await;
                tracing::debug!(
                    task_id = task_id, turn = turn,
                    cmd = %args.command, exit = result.exit_code,
                    "Agent shell"
                );
                ToolResult::ShellOutput(format!(
                    "exit_code: {}\nstdout:\n{}\nstderr:\n{}",
                    result.exit_code,
                    truncate_utf8(&result.stdout, 3000),
                    truncate_utf8(&result.stderr, 1500),
                ))
            }
            "submit_tests" => match serde_json::from_str::<SubmitArgs>(&tc.function.arguments) {
                Ok(s) => ToolResult::Submit(s),
                Err(e) => ToolResult::Error(format!("Invalid submit_tests args: {}", e)),
            },
            other => ToolResult::Error(format!("Unknown tool: {}", other)),
        }
    }

    async fn clone_repo(&self, repo: &str, base_commit: &str) -> Result<tempfile::TempDir> {
        let tmp = tempfile::tempdir()?;
        let url = format!("https://github.com/{}.git", repo);
        let path = tmp.path();

        let status = tokio::process::Command::new("git")
            .args(["clone", "--depth", "50", &url, "."])
            .current_dir(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("Failed to clone {}", repo);
        }

        if !base_commit.is_empty() {
            let _ = tokio::process::Command::new("git")
                .args(["checkout", base_commit])
                .current_dir(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }

        Ok(tmp)
    }
}

enum ToolResult {
    ShellOutput(String),
    Submit(SubmitArgs),
    Error(String),
}

struct ShellOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

async fn execute_shell(command: &str, cwd: &std::path::Path, timeout_ms: u64) -> ShellOutput {
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        tokio::process::Command::new("sh")
            .args(["-c", command])
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => ShellOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        },
        Ok(Err(e)) => ShellOutput {
            stdout: String::new(),
            stderr: format!("Execution error: {}", e),
            exit_code: -1,
        },
        Err(_) => ShellOutput {
            stdout: String::new(),
            stderr: format!("Command timed out after {}ms", timeout_ms),
            exit_code: -1,
        },
    }
}

fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}
