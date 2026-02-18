//! Agentic test generator with dual-commit validation.
//!
//! The LLM writes tests against the base commit inside a Docker container,
//! then the generator automatically verifies them against the PR commit
//! (patch applied) to ensure fail_to_pass tests actually pass after the fix.

use anyhow::Result;
use regex::Regex;
use std::sync::Arc;

use crate::llm::{
    GenerationRequest, LlmProvider, Message, ToolCallInfo, ToolChoice, ToolDefinition,
};
use crate::swe::docker_sandbox::DockerSandbox;
use crate::swe::SweTask;

const MAX_AGENT_TURNS: usize = 200;
const MAX_VALIDATION_RETRIES: usize = 3;

const SYSTEM_PROMPT: &str = r#"You are a test engineer writing verification tests for GitHub pull requests for the SWE-bench benchmark.

CONTEXT: You write tests that verify whether a coding agent correctly reproduced a PR's changes.
- fail_to_pass: tests that FAIL on the base commit (before PR), PASS after the PR is applied.
- pass_to_pass: tests that PASS on both the base commit and after the PR.

You have three tools:
- `shell`: execute a command in the cloned repository (at the BASE commit, before the PR).
- `write_file`: create or overwrite a file in the repository (for writing test files).
- `submit_tests`: return your final validated test commands AND the test files you wrote.

ENVIRONMENT: You are running in a bare `python:3.12-slim` Docker container with ONLY `git` and `python3` pre-installed.
You MUST install all required tools, runtimes, and dependencies yourself via `shell` before doing anything else.

WORKFLOW:
1. SETUP: Detect the project language and install what you need via `shell`:
   - Python: `pip install -e .` or `pip install -r requirements.txt`
   - JavaScript/TypeScript: `apt-get update && apt-get install -y nodejs npm && npm install`
   - Go: `apt-get update && apt-get install -y golang`
   - Rust: `apt-get update && apt-get install -y curl build-essential && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && . $HOME/.cargo/env && cargo fetch`
   - Java: `apt-get update && apt-get install -y default-jdk`
   - Or whatever the repo needs. Check the repo's README, Makefile, Dockerfile, etc.
2. Use `shell` to explore the repo: project structure, existing tests, build system, dependencies.
3. Read the PR diff carefully: understand WHAT changed and WHY.
4. Find existing test suites covering code ADJACENT to the PR changes -- add them as pass_to_pass.
5. Write NEW test files that exercise the BEHAVIOR introduced by the PR.
6. Run your tests via `shell` to validate: fail_to_pass MUST fail, pass_to_pass MUST pass on base.
6b. VERIFY pass_to_pass: Run each pass_to_pass command via `shell` and confirm exit code 0.
    If it fails, choose a different existing test or use a build command instead.
7. Call `submit_tests` with everything.

MANDATORY RULES FOR TEST QUALITY:

1. BEHAVIORAL TESTS ONLY
   - Every fail_to_pass test MUST exercise runtime behavior: import modules, call functions,
     instantiate classes, make HTTP requests, run CLI commands, check return values.
   - Tests must fail with SEMANTIC errors (ImportError, AttributeError, TypeError, AssertionError
     on return values, HTTP 404, missing CLI subcommand) -- NOT "string not found in file".

2. FORBIDDEN PATTERNS (your submission will be REJECTED if you use these):
   - Reading source files and asserting on their text content
     (no open()/readFileSync()/fs.readFile then asserting strings exist in the source).
   - Checking that specific variable names, function names, or import statements exist in source code.
   - Using grep/cat/awk on source files as the test mechanism.
   - Any test whose only assertion is "this string exists in this file".
   - File existence checks as the sole test (assert path.exists() alone is not enough).

3. REGRESSION COVERAGE (pass_to_pass)
   - Include at least 1 pass_to_pass command running existing project tests that cover code
     adjacent to the PR changes.
   - If the project has a test suite, find relevant existing test commands and include them.
   - If the PR changes function_a() in a module, test that function_b() still works (pass_to_pass).
   - If the PR changes a class method, verify other methods on the same class are unaffected.
   - CRITICAL: pass_to_pass commands MUST use EXISTING test infrastructure that already works
     on the base commit. Run the command yourself via `shell` BEFORE submitting to verify it passes.
   - Do NOT create new test files for pass_to_pass. Use the project's existing test commands.
   - If no existing tests exist adjacent to the PR, use a simple build command (e.g., `cargo build`,
     `npm run build`, `go build ./...`) as pass_to_pass instead.

4. ROBUSTNESS & EDGE CASES (derive from the PR diff):
   - If the PR adds input validation: test with null, empty, oversized, malformed inputs.
   - If the PR adds error handling: test the error paths, not just the happy path.
   - If the PR adds a new function: test boundary values, not just the example case.
   - For bug fixes: test the specific bug scenario AND at least one related edge case.
   - For new features: test the happy path AND at least one error/boundary case.
   - For refactors: test that the new API behaves correctly AND old behavior is preserved.

5. COMPLETENESS
   - Write fail_to_pass tests that cover ALL distinct behaviors added by the PR, not just one.
   - If the PR adds 3 new endpoints, test all 3, not just the first.
   - If the PR fixes a bug in 2 places, test both fix locations.
   - Tests must be specific enough that a lazy agent who only partially implements the PR fails.

6. ANTI-HARDCODING
   - Test with DIFFERENT inputs than those shown in the PR description or diff.
   - If the PR adds a function that computes something, test with values NOT in the diff.
   - This catches agents that hardcode return values instead of implementing real logic.

ANTI-PATTERNS THAT WILL BE REJECTED:
- `assert "class Foo" in Path("src/foo.py").read_text()` -> REJECTED
- `source = open("src/foo.py").read(); assert "def bar" in source` -> REJECTED
- `assert fs.readFileSync("src/foo.ts").includes("someString")` -> REJECTED
- `const src = fs.readFileSync(...); assert(src.includes(...))` -> REJECTED
- Tests with fewer than 2 meaningful assertions -> REJECTED"#;

fn shell_tool() -> ToolDefinition {
    ToolDefinition::function(
        "shell",
        "Execute a shell command in the repository. Returns stdout, stderr, and exit code.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
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
                    "description": "Commands that FAIL on base commit, PASS after PR patch"
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

enum ValidationResult {
    Accepted,
    Rejected(String),
}

pub struct TestGenerator {
    llm: Arc<dyn LlmProvider>,
    image_override: Option<String>,
}

impl TestGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            llm,
            image_override: None,
        }
    }

    pub fn with_image(llm: Arc<dyn LlmProvider>, image: Option<String>) -> Self {
        Self {
            llm,
            image_override: image,
        }
    }

    pub async fn ensure_tests(&self, task: &mut SweTask, language: &str) -> Result<()> {
        if task.has_tests() {
            return Ok(());
        }

        tracing::info!(task_id = %task.id, repo = %task.repo, "Starting agentic test generation (Docker)");

        let sandbox = DockerSandbox::start(
            &task.repo,
            &task.base_commit,
            language,
            self.image_override.as_deref(),
        )
        .await?;

        let result = self.run_agent_loop(&sandbox, task, language).await;

        // Always destroy the container, even on error
        sandbox.destroy().await;

        result
    }

    async fn run_agent_loop(
        &self,
        sandbox: &DockerSandbox,
        task: &mut SweTask,
        language: &str,
    ) -> Result<()> {
        let (build_cmds, test_cmds) = SweTask::test_commands_for_language(language);
        let patch_preview = truncate_utf8(&task.patch, 4000);

        let user_msg = format!(
            "Repository: {repo}\nLanguage: {lang}\nPR description: {prompt}\n\n\
             Suggested build: {build}\nSuggested test: {test}\n\n\
             Diff (truncated):\n```\n{patch}\n```\n\n\
             The repo is cloned at /repo. Explore it, write behavioral tests, then submit.\n\n\
             REMEMBER:\n\
             - Your fail_to_pass tests will be verified against the PR patch. \
             They MUST pass once the patch is applied, or they will be rejected.\n\
             - Do NOT read source files and assert on their content. Test runtime behavior only.\n\
             - Include pass_to_pass tests from existing test suites adjacent to the changed code.\n\
             - Test edge cases and use DIFFERENT inputs than those in the diff (anti-hardcoding).",
            repo = task.repo,
            lang = language,
            prompt = truncate_utf8(&task.prompt, 1000),
            build = build_cmds.join(" && "),
            test = test_cmds.join(" && "),
            patch = patch_preview,
        );

        let tools = vec![shell_tool(), write_file_tool(), submit_tool()];
        let mut messages = vec![Message::system(SYSTEM_PROMPT), Message::user(user_msg)];
        let mut written_files: Vec<TestFile> = Vec::new();
        let mut validation_retries = 0;

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

            if let Some(ref tool_calls) = choice.message.tool_calls {
                messages.push(Message::assistant_with_tool_calls(
                    choice.message.content.clone(),
                    tool_calls.clone(),
                ));

                for tc in tool_calls {
                    let result = self
                        .handle_tool_call(tc, sandbox, &task.id, turn, &mut written_files)
                        .await;

                    match result {
                        ToolResult::ShellOutput(output) => {
                            messages.push(Message::tool_result(&tc.id, output));
                        }
                        ToolResult::Submit(submit) => {
                            let mut all_files = written_files.clone();
                            for f in &submit.test_files {
                                if !all_files.iter().any(|wf| wf.path == f.path) {
                                    all_files.push(f.clone());
                                }
                            }

                            if submit.fail_to_pass.is_empty() {
                                if validation_retries < MAX_VALIDATION_RETRIES {
                                    validation_retries += 1;
                                    tracing::warn!(
                                        task_id = %task.id,
                                        retry = validation_retries,
                                        "Rejecting empty fail_to_pass"
                                    );
                                    messages.push(Message::tool_result(
                                        &tc.id,
                                        "REJECTED: fail_to_pass must contain at least one test command. \
                                         Write a test that FAILS on the base commit and PASSES after the PR patch is applied.".to_string(),
                                    ));
                                    continue;
                                }
                                messages.push(Message::tool_result(
                                    &tc.id,
                                    "REJECTED: fail_to_pass is still empty after retries."
                                        .to_string(),
                                ));
                                continue;
                            }

                            // --- Heuristic: reject string-matching tests ---
                            if let Some(rejection) = reject_string_matching_tests(&all_files) {
                                if validation_retries < MAX_VALIDATION_RETRIES {
                                    validation_retries += 1;
                                    tracing::warn!(
                                        task_id = %task.id,
                                        retry = validation_retries,
                                        "Rejecting string-matching tests"
                                    );
                                    messages.push(Message::tool_result(
                                        &tc.id,
                                        format!(
                                            "REJECTED: {rejection}\n\n\
                                             Rewrite your tests to check RUNTIME BEHAVIOR, not file contents. \
                                             Import modules, call functions, check return values. \
                                             Do NOT use open()/readFileSync() to read source and assert strings."
                                        ),
                                    ));
                                    continue;
                                }
                                tracing::warn!(
                                    task_id = %task.id,
                                    "String-matching tests after max retries, REJECTING"
                                );
                                messages.push(Message::tool_result(
                                    &tc.id,
                                    "REJECTED: Your tests still use forbidden source-reading patterns after multiple retries. \
                                     Rewrite completely: import modules, call functions, check return values. \
                                     Do NOT read source files.".to_string(),
                                ));
                                continue;
                            }

                            // --- Dual-commit validation: apply patch, re-run tests ---
                            let patch_validation = self
                                .validate_on_pr_commit(sandbox, &task.patch, &submit, &all_files)
                                .await;

                            match patch_validation {
                                ValidationResult::Rejected(reason) => {
                                    if validation_retries < MAX_VALIDATION_RETRIES {
                                        validation_retries += 1;
                                        tracing::warn!(
                                            task_id = %task.id,
                                            retry = validation_retries,
                                            reason = %reason,
                                            "Dual-commit validation failed, asking LLM to retry"
                                        );
                                        messages.push(Message::tool_result(
                                            &tc.id,
                                            format!(
                                                "REJECTED: {reason}\n\n\
                                                 Your tests were verified against the actual PR patch and failed validation. \
                                                 Please rewrite your tests so that fail_to_pass tests PASS after the PR diff is applied."
                                            ),
                                        ));
                                        continue;
                                    }
                                    tracing::warn!(
                                        task_id = %task.id,
                                        "Dual-commit validation failed after max retries, REJECTING"
                                    );
                                    messages.push(Message::tool_result(
                                        &tc.id,
                                        format!(
                                            "REJECTED: {reason}\n\nYour tests failed dual-commit validation after multiple retries. \
                                             Rewrite your tests completely."
                                        ),
                                    ));
                                    continue;
                                }
                                ValidationResult::Accepted => {
                                    tracing::info!(
                                        task_id = %task.id,
                                        "Dual-commit validation PASSED"
                                    );
                                }
                            }

                            // Verify pass_to_pass commands actually pass on base commit
                            let mut p2p_ok = true;
                            for p2p_cmd in &submit.pass_to_pass {
                                let p2p_result = sandbox.exec(p2p_cmd, 60_000).await;
                                if p2p_result.exit_code != 0 {
                                    p2p_ok = false;
                                    if validation_retries < MAX_VALIDATION_RETRIES {
                                        validation_retries += 1;
                                        tracing::warn!(
                                            task_id = %task.id,
                                            retry = validation_retries,
                                            cmd = %p2p_cmd,
                                            exit = p2p_result.exit_code,
                                            "pass_to_pass command fails on base commit"
                                        );
                                        messages.push(Message::tool_result(
                                            &tc.id,
                                            format!(
                                                "REJECTED: pass_to_pass command '{}' fails on base commit (exit={}). \
                                                 pass_to_pass commands MUST pass on the base commit. \
                                                 Use existing test commands that work, or use a build command instead.",
                                                p2p_cmd, p2p_result.exit_code,
                                            ),
                                        ));
                                        break;
                                    }
                                }
                            }
                            if !p2p_ok {
                                continue;
                            }

                            // Validate test scripts before accepting
                            if let Some(issue) = validate_test_scripts(&all_files) {
                                if validation_retries < MAX_VALIDATION_RETRIES {
                                    validation_retries += 1;
                                    tracing::warn!(
                                        task_id = %task.id,
                                        retry = validation_retries,
                                        issue = %issue,
                                        "Test script validation failed"
                                    );
                                    messages.push(Message::tool_result(
                                        &tc.id,
                                        format!(
                                            "REJECTED: {issue}\n\nFix the issues and resubmit."
                                        ),
                                    ));
                                    continue;
                                }
                                tracing::warn!(
                                    task_id = %task.id,
                                    "Test script validation failed after max retries, accepting anyway"
                                );
                            }

                            tracing::info!(
                                task_id = %task.id, turn = turn,
                                f2p = submit.fail_to_pass.len(),
                                p2p = submit.pass_to_pass.len(),
                                files = all_files.len(),
                                "Agent submitted tests"
                            );
                            task.fail_to_pass = submit.fail_to_pass;
                            task.pass_to_pass = submit.pass_to_pass;
                            if !all_files.is_empty() {
                                if let Ok(json) = serde_json::to_string(&all_files) {
                                    task.meta.insert("test_files".to_string(), json);
                                }
                            }
                            task.meta.insert(
                                "test_generation".to_string(),
                                "agentic-docker".to_string(),
                            );
                            return Ok(());
                        }
                        ToolResult::Error(err) => {
                            messages.push(Message::tool_result(&tc.id, err));
                        }
                    }
                }
                continue;
            }

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
            "Agentic test generation failed for {}: exhausted {} turns without submitting",
            task.id,
            MAX_AGENT_TURNS
        )
    }

    /// Validate submitted tests by applying the PR patch and re-running them inside the sandbox.
    async fn validate_on_pr_commit(
        &self,
        sandbox: &DockerSandbox,
        patch: &str,
        submit: &SubmitArgs,
        test_files: &[TestFile],
    ) -> ValidationResult {
        if patch.trim().is_empty() {
            tracing::warn!("Empty patch, skipping dual-commit validation");
            return ValidationResult::Accepted;
        }

        // Ensure test files are written (they may have been cleaned by git checkout)
        for tf in test_files {
            if let Err(e) = sandbox.write_file(&tf.path, &tf.content).await {
                tracing::warn!(path = %tf.path, error = %e, "Failed to write test file for validation");
            }
        }

        // Write the PR patch and apply it
        if let Err(e) = sandbox.write_file(".swe_forge_pr.patch", patch).await {
            tracing::warn!(error = %e, "Failed to write patch file");
            return ValidationResult::Accepted;
        }

        let apply_result = sandbox
            .exec("git apply --allow-empty .swe_forge_pr.patch 2>&1", 30_000)
            .await;

        if apply_result.exit_code != 0 {
            let apply_3way = sandbox
                .exec("git apply --3way .swe_forge_pr.patch 2>&1", 30_000)
                .await;
            if apply_3way.exit_code != 0 {
                tracing::warn!(
                    stderr = %apply_3way.stderr,
                    "Patch apply failed, rejecting task"
                );
                sandbox.exec("git checkout -- . 2>/dev/null", 10_000).await;
                return ValidationResult::Rejected(
                    "PR patch could not be applied to the base commit. The test cannot be validated.".to_string()
                );
            }
        }

        // Re-run fail_to_pass: must now PASS
        for cmd in &submit.fail_to_pass {
            let result = sandbox.exec(cmd, 60_000).await;
            if result.exit_code != 0 {
                sandbox.exec("git checkout -- . 2>/dev/null", 10_000).await;
                sandbox.exec("git clean -fd 2>/dev/null", 10_000).await;
                for tf in test_files {
                    let _ = sandbox.write_file(&tf.path, &tf.content).await;
                }
                return ValidationResult::Rejected(format!(
                    "fail_to_pass test '{}' still FAILS after the PR patch is applied (exit={}, stderr={}). \
                     This means your test does not actually test what the PR changes.",
                    cmd,
                    result.exit_code,
                    truncate_utf8(&result.stderr, 500),
                ));
            }
        }

        // Re-run pass_to_pass: must still PASS
        for cmd in &submit.pass_to_pass {
            let result = sandbox.exec(cmd, 60_000).await;
            if result.exit_code != 0 {
                sandbox.exec("git checkout -- . 2>/dev/null", 10_000).await;
                sandbox.exec("git clean -fd 2>/dev/null", 10_000).await;
                for tf in test_files {
                    let _ = sandbox.write_file(&tf.path, &tf.content).await;
                }
                return ValidationResult::Rejected(format!(
                    "pass_to_pass test '{}' FAILS after the PR patch (exit={}, stderr={}). \
                     This is a regression in your test.",
                    cmd,
                    result.exit_code,
                    truncate_utf8(&result.stderr, 500),
                ));
            }
        }

        // Revert to base commit for cleanliness
        sandbox.exec("git checkout -- . 2>/dev/null", 10_000).await;
        sandbox.exec("git clean -fd 2>/dev/null", 10_000).await;
        for tf in test_files {
            let _ = sandbox.write_file(&tf.path, &tf.content).await;
        }

        ValidationResult::Accepted
    }

    async fn handle_tool_call(
        &self,
        tc: &ToolCallInfo,
        sandbox: &DockerSandbox,
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
                let result = sandbox.exec(&args.command, args.timeout_ms).await;
                tracing::debug!(
                    task_id = task_id, turn = turn,
                    cmd = %args.command, exit = result.exit_code,
                    "Agent shell (Docker)"
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
                match sandbox.write_file(&args.path, &args.content).await {
                    Ok(_) => {
                        tracing::debug!(
                            task_id = task_id, turn = turn,
                            path = %args.path, bytes = args.content.len(),
                            "Agent wrote file (Docker)"
                        );
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
}

enum ToolResult {
    ShellOutput(String),
    Submit(SubmitArgs),
    Error(String),
}

/// Scan test files for string-matching anti-patterns and return a rejection reason if found.
fn reject_string_matching_tests(files: &[TestFile]) -> Option<String> {
    let patterns: &[(&str, &str)] = &[
        // Python source-reading patterns
        (
            r#"open\([^)]*\)\.read"#,
            "open().read() used to read source files",
        ),
        (
            r#"Path\([^)]*\)\.read_text"#,
            "Path().read_text() used to read source files",
        ),
        (
            r#"\.read\(\)[^;]*assert.*\bin\b"#,
            ".read() + assert...in (string-matching)",
        ),
        // JavaScript/TypeScript source-reading patterns
        (
            r#"readFileSync\("#,
            "readFileSync() used to read source files",
        ),
        (r#"readFile\("#, "readFile() used to read source files"),
        // Combined read + assert patterns
        (
            r#"assert.*\bin\s+(source|content|text|code|file_content|src|contents)"#,
            "assert...in source/content (string-matching on file content)",
        ),
        (
            r#"\.(includes|contains)\(['""]"#,
            ".includes()/.contains() on source content",
        ),
    ];

    let mut violations = Vec::new();

    for file in files {
        let content = &file.content;
        for &(pattern, description) in patterns {
            if let Ok(re) = Regex::new(pattern) {
                let matches: Vec<_> = re.find_iter(content).collect();
                if !matches.is_empty() {
                    // Check if this is actually testing source files (not testing output/response)
                    // Heuristic: if the file also contains behavioral test patterns, it's likely mixed
                    let has_behavioral = content.contains("import ")
                        || content.contains("require(")
                        || content.contains("from ")
                        || content.contains("fetch(")
                        || content.contains("request(");

                    // Count string-matching assertions vs total assertions
                    let total_asserts = content.matches("assert").count()
                        + content.matches("expect(").count()
                        + content.matches("Assert.").count();
                    let string_match_count = matches.len();

                    // Reject if >50% of assertions are string-matching, or if there are no behavioral patterns
                    if total_asserts > 0
                        && (string_match_count * 2 > total_asserts || !has_behavioral)
                    {
                        violations.push(format!(
                            "File '{}': {} ({} of {} assertions)",
                            file.path, description, string_match_count, total_asserts
                        ));
                    }
                }
            }
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(format!(
            "Your tests use forbidden source-reading patterns:\n- {}",
            violations.join("\n- ")
        ))
    }
}

/// Validate generated test scripts for structural issues.
///
/// Checks that shell scripts have a valid shebang line and that test files
/// referenced in shell commands actually exist in the submitted file set.
/// Returns `Some(reason)` if validation fails, `None` if all checks pass.
fn validate_test_scripts(files: &[TestFile]) -> Option<String> {
    let mut issues = Vec::new();
    let known_paths: std::collections::HashSet<&str> =
        files.iter().map(|f| f.path.as_str()).collect();

    for file in files {
        let is_shell = file.path.ends_with(".sh") || file.path.ends_with(".bash");

        if is_shell {
            let trimmed = file.content.trim_start();
            if !trimmed.starts_with("#!") {
                issues.push(format!(
                    "Shell script '{}' is missing a shebang line (e.g. #!/bin/bash)",
                    file.path
                ));
            }

            if file.content.trim().is_empty() {
                issues.push(format!("Shell script '{}' is empty", file.path));
            }
        }

        for line in file.content.lines() {
            let trimmed = line.trim();
            // Detect references to test files like `python tests/test_foo.py`
            // or `bash tests/run.sh` that aren't in the submitted set
            for token in trimmed.split_whitespace() {
                if (token.starts_with("tests/") || token.starts_with("./tests/"))
                    && (token.ends_with(".py")
                        || token.ends_with(".js")
                        || token.ends_with(".ts")
                        || token.ends_with(".sh"))
                {
                    let normalized = token.strip_prefix("./").unwrap_or(token);
                    if !known_paths.contains(normalized) {
                        issues.push(format!(
                            "File '{}' references '{}' which was not submitted",
                            file.path, normalized
                        ));
                    }
                }
            }
        }
    }

    if issues.is_empty() {
        None
    } else {
        Some(format!(
            "Test script validation issues:\n- {}",
            issues.join("\n- ")
        ))
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
