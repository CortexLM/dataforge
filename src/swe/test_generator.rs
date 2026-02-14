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

const SYSTEM_PROMPT: &str = r#"You are a test engineer writing real test code for GitHub pull requests for SWE-bench.

You have three tools:
- `shell`: execute a command in the cloned repository (at the BASE commit, before the PR).
- `write_file`: create or overwrite a file in the repository (for writing test files).
- `submit_tests`: return your final validated test commands AND the test files you wrote.

Your goal:
1. Use `shell` to explore the repo structure: find existing tests, understand the framework, read source code.
2. Read the PR diff carefully to understand what changed.
3. WRITE actual test files using `write_file`:
   - Write test code that exercises the behavior introduced by the PR.
   - Use the project's real test framework (pytest, jest, go test, cargo test, etc.).
   - Follow the project's existing test patterns and directory structure.
4. Run your test files via `shell` to verify:
   - fail_to_pass tests MUST fail (exit != 0) on the base commit (the PR code is not there yet).
   - pass_to_pass tests MUST pass (exit == 0) on the base commit.
5. Call `submit_tests` with the commands AND the test file contents.

RULES:
- You MUST write real test files with actual assertions, not just shell commands.
- Every test file must be written with `write_file` before running it.
- Every command you submit MUST be validated via `shell` first.
- fail_to_pass commands MUST exit non-zero when you run them (on the base commit).
- pass_to_pass commands MUST exit zero when you run them (on the base commit).
- For fail_to_pass: write tests that check for behavior/functions/features added by the PR. They fail because the code doesn't exist yet.
- For pass_to_pass: run existing tests or write tests for existing functionality that must not break.
- Test files should be in the standard test directory for the project (tests/, test/, __tests__/, etc.).
- Use targeted test commands (e.g. `pytest tests/test_new_feature.py`, not just `pytest`)."#;

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

fn write_file_tool() -> ToolDefinition {
    ToolDefinition::function(
        "write_file",
        "Create or overwrite a file in the repository. Use this to write test files.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path in the repo (e.g. 'tests/test_new_feature.py')"
                },
                "content": {
                    "type": "string",
                    "description": "Full file content to write"
                }
            },
            "required": ["path", "content"]
        }),
    )
}

fn submit_tool() -> ToolDefinition {
    ToolDefinition::function(
        "submit_tests",
        "Submit the final validated test commands and the test files you wrote.",
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
                },
                "test_files": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Relative file path"},
                            "content": {"type": "string", "description": "Full file content"}
                        },
                        "required": ["path", "content"]
                    },
                    "description": "Test files written during this session"
                }
            },
            "required": ["fail_to_pass", "pass_to_pass", "test_files"]
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
struct WriteFileArgs {
    path: String,
    content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, serde::Deserialize)]
struct SubmitArgs {
    #[serde(default)]
    fail_to_pass: Vec<String>,
    #[serde(default)]
    pass_to_pass: Vec<String>,
    #[serde(default)]
    test_files: Vec<TestFile>,
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

        let tools = vec![shell_tool(), write_file_tool(), submit_tool()];
        let mut messages = vec![Message::system(SYSTEM_PROMPT), Message::user(user_msg)];
        let mut written_files: Vec<TestFile> = Vec::new();

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
                        .handle_tool_call(tc, &repo_path, &task.id, turn, &mut written_files)
                        .await;

                    match result {
                        ToolResult::ShellOutput(output) => {
                            messages.push(Message::tool_result(&tc.id, output));
                        }
                        ToolResult::Submit(submit) => {
                            // Merge written_files from the session with any from submit
                            let mut all_files = written_files.clone();
                            for f in &submit.test_files {
                                if !all_files.iter().any(|wf| wf.path == f.path) {
                                    all_files.push(f.clone());
                                }
                            }

                            tracing::info!(
                                task_id = %task.id, turn = turn,
                                f2p = submit.fail_to_pass.len(),
                                p2p = submit.pass_to_pass.len(),
                                files = all_files.len(),
                                "Agent submitted validated tests"
                            );
                            task.fail_to_pass = submit.fail_to_pass;
                            task.pass_to_pass = submit.pass_to_pass;
                            // Store test files as JSON in meta
                            if !all_files.is_empty() {
                                if let Ok(json) = serde_json::to_string(&all_files) {
                                    task.meta.insert("test_files".to_string(), json);
                                }
                            }
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
        written_files: &mut Vec<TestFile>,
    ) -> ToolResult {
        match tc.function.name.as_str() {
            "shell" => {
                let args: ShellArgs = match serde_json::from_str(&tc.function.arguments) {
                    Ok(a) => a,
                    Err(e) => return ToolResult::Error(format!("Invalid shell args: {}", e)),
                };
                let result = execute_shell(&args.command, repo_path, args.timeout_ms).await;
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
            "write_file" => {
                let args: WriteFileArgs = match serde_json::from_str(&tc.function.arguments) {
                    Ok(a) => a,
                    Err(e) => return ToolResult::Error(format!("Invalid write_file args: {}", e)),
                };
                let full_path = repo_path.join(&args.path);
                if let Some(parent) = full_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&full_path, &args.content) {
                    Ok(_) => {
                        tracing::debug!(
                            task_id = task_id, turn = turn,
                            path = %args.path, bytes = args.content.len(),
                            "Agent wrote file"
                        );
                        // Track the file
                        if let Some(existing) =
                            written_files.iter_mut().find(|f| f.path == args.path)
                        {
                            existing.content = args.content;
                        } else {
                            written_files.push(TestFile {
                                path: args.path.clone(),
                                content: args.content,
                            });
                        }
                        ToolResult::ShellOutput(format!("File written: {}", args.path))
                    }
                    Err(e) => ToolResult::Error(format!("Failed to write {}: {}", args.path, e)),
                }
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
