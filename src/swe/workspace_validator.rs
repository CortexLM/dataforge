//! Pre-export workspace validation.
//!
//! Performs a complete end-to-end validation of a `SweTask` before it is
//! exported to disk. This catches tasks that would fail when run through
//! the harness (setup_error, sanity_fail) by verifying:
//!
//! 1. The repository can be cloned and the base commit checked out
//! 2. The install command succeeds (with LLM-powered retry if available)
//! 3. `fail_to_pass` commands **fail** on the base commit
//! 4. `pass_to_pass` commands **pass** on the base commit
//! 5. After applying the PR patch, `fail_to_pass` commands **pass**
//! 6. After applying the PR patch, `pass_to_pass` commands still **pass**
//! 7. The prompt is feasible (non-empty, sufficient length, no test leaks)
//! 8. A final fresh-container re-validation replays everything from scratch

use std::sync::Arc;

use super::docker_sandbox::DockerSandbox;
use super::test_generator::TestFile;
use super::SweTask;
use crate::llm::{GenerationRequest, LlmProvider, Message, ToolChoice, ToolDefinition};

/// Maximum number of LLM-powered install fix retries.
const MAX_INSTALL_RETRIES: usize = 3;

/// Result of workspace validation.
#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    /// All checks passed; task is safe to export.
    Passed,
    /// One or more checks failed; task should be rejected.
    Rejected { reason: String },
}

/// Pre-export workspace validator.
pub struct WorkspaceValidator {
    image_override: Option<String>,
    llm: Option<Arc<dyn LlmProvider>>,
}

impl WorkspaceValidator {
    /// Create a new validator with an optional LLM provider for install-fix retries.
    pub fn new(image_override: Option<String>, llm: Option<Arc<dyn LlmProvider>>) -> Self {
        Self {
            image_override,
            llm,
        }
    }

    /// Run full end-to-end validation on a task.
    ///
    /// Creates a fresh Docker container, clones the repo, runs install,
    /// verifies test semantics on base and patched commits, then destroys
    /// the container. If the LLM provider is available, failed installs
    /// are retried with LLM-generated fix suggestions. A final fresh-container
    /// re-validation ensures reproducibility.
    pub async fn validate(&self, task: &mut SweTask) -> Result<ValidationOutcome, anyhow::Error> {
        // --- Prompt feasibility ---
        if let Some(reason) = check_prompt_feasibility(task) {
            return Ok(ValidationOutcome::Rejected { reason });
        }

        // Must have at least one fail_to_pass
        if task.fail_to_pass.is_empty() {
            return Ok(ValidationOutcome::Rejected {
                reason: "No fail_to_pass test commands".to_string(),
            });
        }

        // --- Docker environment ---
        let sandbox = match DockerSandbox::start(
            &task.repo,
            &task.base_commit,
            &task.language,
            self.image_override.as_deref(),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!("Failed to start validation container: {e}"),
                });
            }
        };

        let result = self.run_validation(&sandbox, task).await;

        // Always destroy the container
        sandbox.destroy().await;

        // If the first validation passed, do a final fresh-container re-validation
        if matches!(result, Ok(ValidationOutcome::Passed)) {
            return self.fresh_container_revalidation(task).await;
        }

        result
    }

    async fn run_validation(
        &self,
        sandbox: &DockerSandbox,
        task: &mut SweTask,
    ) -> Result<ValidationOutcome, anyhow::Error> {
        // --- Install language runtime if needed ---
        let runtime_install = match task.language.to_lowercase().as_str() {
            "go" | "golang" => Some("apt-get update -qq && apt-get install -y -qq golang > /dev/null 2>&1"),
            "javascript" | "typescript" | "js" | "ts" => Some("apt-get update -qq && apt-get install -y -qq nodejs npm > /dev/null 2>&1"),
            "rust" => Some("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1 && . $HOME/.cargo/env"),
            "java" => Some("apt-get update -qq && apt-get install -y -qq default-jdk maven > /dev/null 2>&1"),
            _ => None,
        };
        if let Some(cmd) = runtime_install {
            let rt_result = sandbox.exec(&format!("{} 2>&1", cmd), 300_000).await;
            if rt_result.exit_code != 0 {
                tracing::warn!(
                    task_id = %task.id,
                    language = %task.language,
                    exit = rt_result.exit_code,
                    "Runtime install failed during validation (continuing)"
                );
            }
        }

        // --- Install ---
        if let Some(install_cmd) = task.install_config.get("install") {
            if !install_cmd.is_empty() && !install_cmd.starts_with('#') {
                let install_cmd_owned = install_cmd.clone();
                let install_result = sandbox
                    .exec(&format!("cd /repo && {} 2>&1", install_cmd_owned), 300_000)
                    .await;
                if install_result.exit_code != 0 {
                    let error_output = format!(
                        "stdout: {}\nstderr: {}",
                        truncate_str(&install_result.stdout, 500),
                        truncate_str(&install_result.stderr, 500),
                    );

                    // Try LLM-powered fix if available
                    if let Some(ref llm) = self.llm {
                        tracing::warn!(
                            task_id = %task.id,
                            exit = install_result.exit_code,
                            "Install command failed, attempting LLM-powered fix"
                        );
                        match self
                            .fix_install_with_llm(
                                sandbox,
                                task,
                                &install_cmd_owned,
                                &error_output,
                                llm,
                            )
                            .await
                        {
                            Ok(true) => {
                                tracing::info!(
                                    task_id = %task.id,
                                    "LLM-powered install fix succeeded"
                                );
                            }
                            Ok(false) => {
                                return Ok(ValidationOutcome::Rejected {
                                    reason: format!(
                                        "Install command failed after {} LLM retries (exit={}): {}",
                                        MAX_INSTALL_RETRIES,
                                        install_result.exit_code,
                                        truncate_str(&install_result.stderr, 500),
                                    ),
                                });
                            }
                            Err(e) => {
                                return Ok(ValidationOutcome::Rejected {
                                    reason: format!(
                                        "Install command failed (exit={}) and LLM fix errored: {}",
                                        install_result.exit_code, e,
                                    ),
                                });
                            }
                        }
                    } else {
                        return Ok(ValidationOutcome::Rejected {
                            reason: format!(
                                "Install command failed (exit={}): {}",
                                install_result.exit_code,
                                truncate_str(&install_result.stderr, 500),
                            ),
                        });
                    }
                } else {
                    tracing::debug!(
                        container = %sandbox.name(),
                        "Install command succeeded"
                    );
                }
            }
        }

        // --- Copy test files ---
        if let Some(test_files_json) = task.meta.get("test_files") {
            if let Ok(files) = serde_json::from_str::<Vec<TestFile>>(test_files_json) {
                for tf in &files {
                    if let Err(e) = sandbox.write_file(&tf.path, &tf.content).await {
                        tracing::warn!(
                            path = %tf.path,
                            error = %e,
                            "Failed to write test file during validation"
                        );
                    }
                }
            }
        }

        // --- Base commit: fail_to_pass must FAIL ---
        for cmd in &task.fail_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code == 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "fail_to_pass command already passes on base commit: {}",
                        cmd,
                    ),
                });
            }
        }

        // --- Base commit: pass_to_pass must PASS ---
        for cmd in &task.pass_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "pass_to_pass command fails on base commit (exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        // --- Apply patch ---
        if task.patch.trim().is_empty() {
            return Ok(ValidationOutcome::Rejected {
                reason: "Empty patch".to_string(),
            });
        }

        if let Err(e) = sandbox
            .write_file(".swe_forge_validation.patch", &task.patch)
            .await
        {
            return Ok(ValidationOutcome::Rejected {
                reason: format!("Failed to write patch file: {e}"),
            });
        }

        let apply_result = sandbox
            .exec(
                "cd /repo && git apply --allow-empty .swe_forge_validation.patch 2>&1",
                30_000,
            )
            .await;

        if apply_result.exit_code != 0 {
            let apply_3way = sandbox
                .exec(
                    "cd /repo && git apply --3way .swe_forge_validation.patch 2>&1",
                    30_000,
                )
                .await;
            if apply_3way.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Patch could not be applied: {}",
                        truncate_str(&apply_3way.stderr, 500),
                    ),
                });
            }
        }

        // Re-write test files (patch may have clobbered them)
        if let Some(test_files_json) = task.meta.get("test_files") {
            if let Ok(files) = serde_json::from_str::<Vec<TestFile>>(test_files_json) {
                for tf in &files {
                    if let Err(e) = sandbox.write_file(&tf.path, &tf.content).await {
                        tracing::warn!(path = %tf.path, error = %e, "Failed to re-write test file after patch");
                    }
                }
            }
        }

        // --- Patched commit: fail_to_pass must now PASS ---
        for cmd in &task.fail_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "fail_to_pass command still fails after patch (exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        // --- Patched commit: pass_to_pass must still PASS ---
        for cmd in &task.pass_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "pass_to_pass command fails after patch (regression, exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        tracing::info!(
            task_id = %task.id,
            "Workspace validation PASSED (initial)"
        );

        Ok(ValidationOutcome::Passed)
    }

    /// Attempt to fix a failed install command using the LLM.
    ///
    /// Sends the failed command and error output to the LLM, which suggests
    /// corrected install commands via function calling. Retries up to
    /// `MAX_INSTALL_RETRIES` times, feeding each new error back to the LLM.
    async fn fix_install_with_llm(
        &self,
        sandbox: &DockerSandbox,
        task: &mut SweTask,
        failed_cmd: &str,
        error_output: &str,
        llm: &Arc<dyn LlmProvider>,
    ) -> Result<bool, anyhow::Error> {
        let fix_tool = ToolDefinition::function(
            "fix_install",
            "Provide corrected install commands for the project.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "install_commands": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Corrected shell commands to install all dependencies. Each command should be self-contained. Include apt-get for system deps if needed."
                    }
                },
                "required": ["install_commands"]
            }),
        );

        let mut current_error = error_output.to_string();
        let mut current_cmd = failed_cmd.to_string();

        for retry in 0..MAX_INSTALL_RETRIES {
            tracing::warn!(
                task_id = %task.id,
                retry = retry + 1,
                max_retries = MAX_INSTALL_RETRIES,
                "Attempting LLM install fix"
            );

            let prompt = format!(
                "The install command failed in a fresh Docker container (python:3.12-slim) for repository '{repo}'.\n\
                 Language: {lang}\n\
                 Failed command: {cmd}\n\
                 Error output:\n```\n{error}\n```\n\n\
                 Suggest a corrected set of install commands. Consider:\n\
                 - The project might need system packages (apt-get install -y build-essential libffi-dev etc.)\n\
                 - It might need a different install command (pip install -e \".[dev]\", pip install -r requirements-dev.txt, etc.)\n\
                 - It might need specific build tools or compilers\n\
                 - Each command must be complete and self-contained\n\
                 - Commands will be run with `cd /repo && <command>`",
                repo = task.repo,
                lang = task.language,
                cmd = current_cmd,
                error = truncate_str(&current_error, 2000),
            );

            let request = GenerationRequest {
                model: String::new(),
                messages: vec![
                    Message::system(
                        "You are a DevOps expert. Fix failed install commands for software projects. \
                         Use the fix_install tool to provide corrected commands.",
                    ),
                    Message::user(prompt),
                ],
                temperature: Some(0.2),
                max_tokens: Some(1000),
                top_p: None,
                response_format: None,
                tools: Some(vec![fix_tool.clone()]),
                tool_choice: Some(ToolChoice::force("fix_install")),
            };

            let response = llm.generate(request).await?;
            let choice = match response.choices.first() {
                Some(c) => c,
                None => continue,
            };

            let tool_calls = match &choice.message.tool_calls {
                Some(tc) => tc,
                None => continue,
            };

            let tc = match tool_calls.first() {
                Some(tc) => tc,
                None => continue,
            };

            #[derive(serde::Deserialize)]
            struct FixInstallArgs {
                #[serde(default)]
                install_commands: Vec<String>,
            }

            let args: FixInstallArgs = match serde_json::from_str(&tc.function.arguments) {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(
                        task_id = %task.id,
                        error = %e,
                        "Failed to parse LLM fix_install response"
                    );
                    continue;
                }
            };

            if args.install_commands.is_empty() {
                continue;
            }

            let combined = args.install_commands.join(" && ");
            tracing::debug!(
                task_id = %task.id,
                retry = retry + 1,
                cmd = %combined,
                "Trying LLM-suggested install commands"
            );

            let result = sandbox
                .exec(&format!("cd /repo && {} 2>&1", combined), 300_000)
                .await;

            if result.exit_code == 0 {
                task.install_config.insert("install".to_string(), combined);
                task.meta.insert(
                    "install_source".to_string(),
                    "llm-validator-fix".to_string(),
                );
                return Ok(true);
            }

            current_cmd = combined;
            current_error = format!(
                "stdout: {}\nstderr: {}",
                truncate_str(&result.stdout, 500),
                truncate_str(&result.stderr, 500),
            );
            tracing::warn!(
                task_id = %task.id,
                retry = retry + 1,
                exit = result.exit_code,
                "LLM-suggested install commands also failed"
            );
        }

        Ok(false)
    }

    /// Final fresh-container re-validation.
    ///
    /// Destroys the current state and creates a brand new Docker container,
    /// replays the (possibly corrected) install commands and all test checks
    /// from scratch. This ensures install commands are reproducible and not
    /// dependent on leftover state from the LLM retry loop.
    async fn fresh_container_revalidation(
        &self,
        task: &SweTask,
    ) -> Result<ValidationOutcome, anyhow::Error> {
        tracing::info!(
            task_id = %task.id,
            "Starting final fresh-container re-validation"
        );

        let sandbox = match DockerSandbox::start(
            &task.repo,
            &task.base_commit,
            &task.language,
            self.image_override.as_deref(),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!("Fresh re-validation: failed to start container: {e}"),
                });
            }
        };

        let result = self.run_fresh_validation(&sandbox, task).await;
        sandbox.destroy().await;
        result
    }

    /// Run validation in a fresh container (no LLM retries — just replay and check).
    async fn run_fresh_validation(
        &self,
        sandbox: &DockerSandbox,
        task: &SweTask,
    ) -> Result<ValidationOutcome, anyhow::Error> {
        // --- Install language runtime if needed ---
        let runtime_install = match task.language.to_lowercase().as_str() {
            "go" | "golang" => Some("apt-get update -qq && apt-get install -y -qq golang > /dev/null 2>&1"),
            "javascript" | "typescript" | "js" | "ts" => Some("apt-get update -qq && apt-get install -y -qq nodejs npm > /dev/null 2>&1"),
            "rust" => Some("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1 && . $HOME/.cargo/env"),
            "java" => Some("apt-get update -qq && apt-get install -y -qq default-jdk maven > /dev/null 2>&1"),
            _ => None,
        };
        if let Some(cmd) = runtime_install {
            let rt_result = sandbox.exec(&format!("{} 2>&1", cmd), 300_000).await;
            if rt_result.exit_code != 0 {
                tracing::warn!(
                    task_id = %task.id,
                    language = %task.language,
                    exit = rt_result.exit_code,
                    "Runtime install failed during fresh re-validation (continuing)"
                );
            }
        }

        // --- Install ---
        if let Some(install_cmd) = task.install_config.get("install") {
            if !install_cmd.is_empty() && !install_cmd.starts_with('#') {
                let install_result = sandbox
                    .exec(&format!("cd /repo && {} 2>&1", install_cmd), 300_000)
                    .await;
                if install_result.exit_code != 0 {
                    return Ok(ValidationOutcome::Rejected {
                        reason: format!(
                            "Fresh re-validation: install command failed (exit={}): {}",
                            install_result.exit_code,
                            truncate_str(&install_result.stderr, 500),
                        ),
                    });
                }
            }
        }

        // --- Copy test files ---
        if let Some(test_files_json) = task.meta.get("test_files") {
            if let Ok(files) = serde_json::from_str::<Vec<TestFile>>(test_files_json) {
                for tf in &files {
                    if let Err(e) = sandbox.write_file(&tf.path, &tf.content).await {
                        tracing::warn!(
                            path = %tf.path,
                            error = %e,
                            "Failed to write test file during fresh re-validation"
                        );
                    }
                }
            }
        }

        // --- Base commit: fail_to_pass must FAIL ---
        for cmd in &task.fail_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code == 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Fresh re-validation: fail_to_pass already passes on base commit: {}",
                        cmd,
                    ),
                });
            }
        }

        // --- Base commit: pass_to_pass must PASS ---
        for cmd in &task.pass_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Fresh re-validation: pass_to_pass fails on base commit (exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        // --- Apply patch ---
        if let Err(e) = sandbox
            .write_file(".swe_forge_validation.patch", &task.patch)
            .await
        {
            return Ok(ValidationOutcome::Rejected {
                reason: format!("Fresh re-validation: failed to write patch file: {e}"),
            });
        }

        let apply_result = sandbox
            .exec(
                "cd /repo && git apply --allow-empty .swe_forge_validation.patch 2>&1",
                30_000,
            )
            .await;

        if apply_result.exit_code != 0 {
            let apply_3way = sandbox
                .exec(
                    "cd /repo && git apply --3way .swe_forge_validation.patch 2>&1",
                    30_000,
                )
                .await;
            if apply_3way.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Fresh re-validation: patch could not be applied: {}",
                        truncate_str(&apply_3way.stderr, 500),
                    ),
                });
            }
        }

        // Re-write test files (patch may have clobbered them)
        if let Some(test_files_json) = task.meta.get("test_files") {
            if let Ok(files) = serde_json::from_str::<Vec<TestFile>>(test_files_json) {
                for tf in &files {
                    if let Err(e) = sandbox.write_file(&tf.path, &tf.content).await {
                        tracing::warn!(path = %tf.path, error = %e, "Failed to re-write test file after patch in fresh re-validation");
                    }
                }
            }
        }

        // --- Patched commit: fail_to_pass must now PASS ---
        for cmd in &task.fail_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Fresh re-validation: fail_to_pass still fails after patch (exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        // --- Patched commit: pass_to_pass must still PASS ---
        for cmd in &task.pass_to_pass {
            let result = sandbox.exec(&format!("cd /repo && {}", cmd), 120_000).await;
            if result.exit_code != 0 {
                return Ok(ValidationOutcome::Rejected {
                    reason: format!(
                        "Fresh re-validation: pass_to_pass fails after patch (exit={}): {}",
                        result.exit_code, cmd,
                    ),
                });
            }
        }

        tracing::info!(
            task_id = %task.id,
            "Workspace validation PASSED (fresh re-validation)"
        );

        Ok(ValidationOutcome::Passed)
    }
}

/// Check prompt feasibility without Docker.
///
/// Returns `Some(reason)` if the prompt is not feasible, `None` if OK.
pub fn check_prompt_feasibility(task: &SweTask) -> Option<String> {
    if task.prompt.trim().is_empty() {
        return Some("Prompt is empty".to_string());
    }

    if task.prompt.trim().len() < 100 {
        return Some(format!(
            "Prompt too short ({} chars, minimum 100)",
            task.prompt.trim().len(),
        ));
    }

    // Check for test leaks in prompt
    let prompt_lower = task.prompt.to_lowercase();
    for cmd in &task.fail_to_pass {
        if prompt_lower.contains(&cmd.to_lowercase()) {
            return Some(format!(
                "Prompt contains fail_to_pass command: {}",
                truncate_str(cmd, 100),
            ));
        }
    }

    // Check for test file name leaks
    if let Some(test_files_json) = task.meta.get("test_files") {
        if let Ok(files) = serde_json::from_str::<Vec<TestFile>>(test_files_json) {
            for tf in &files {
                let basename = std::path::Path::new(&tf.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !basename.is_empty() && prompt_lower.contains(&basename.to_lowercase()) {
                    return Some(format!("Prompt contains test file name: {}", basename,));
                }
            }
        }
    }

    None
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_feasibility_empty() {
        let mut task = SweTask::new("test-1", "owner/repo");
        task.prompt = String::new();
        assert!(check_prompt_feasibility(&task).is_some());
    }

    #[test]
    fn prompt_feasibility_too_short() {
        let mut task = SweTask::new("test-2", "owner/repo");
        task.prompt = "Fix the bug.".to_string();
        let result = check_prompt_feasibility(&task);
        assert!(result.is_some());
        assert!(result.unwrap().contains("too short"));
    }

    #[test]
    fn prompt_feasibility_ok() {
        let mut task = SweTask::new("test-3", "owner/repo");
        task.prompt = "This is a sufficiently long prompt that describes a real software engineering problem requiring changes to multiple files and careful understanding of the codebase architecture.".to_string();
        assert!(check_prompt_feasibility(&task).is_none());
    }

    #[test]
    fn prompt_feasibility_test_leak() {
        let mut task = SweTask::new("test-4", "owner/repo");
        task.prompt = "This is a sufficiently long prompt that describes a real software engineering problem. Run python -m pytest tests/test_foo.py to verify your changes work correctly.".to_string();
        task.fail_to_pass = vec!["python -m pytest tests/test_foo.py".to_string()];
        let result = check_prompt_feasibility(&task);
        assert!(result.is_some());
        assert!(result.unwrap().contains("fail_to_pass"));
    }

    #[test]
    fn prompt_feasibility_file_name_leak() {
        let mut task = SweTask::new("test-5", "owner/repo");
        task.prompt = "This is a sufficiently long prompt that describes a real software engineering problem. Make sure test_special_feature.py passes after your changes.".to_string();
        task.meta.insert(
            "test_files".to_string(),
            serde_json::to_string(&vec![TestFile {
                path: "tests/test_special_feature.py".to_string(),
                content: "pass".to_string(),
            }])
            .unwrap(),
        );
        let result = check_prompt_feasibility(&task);
        assert!(result.is_some());
        assert!(result.unwrap().contains("test file name"));
    }

    #[test]
    fn validation_outcome_debug() {
        let passed = ValidationOutcome::Passed;
        let rejected = ValidationOutcome::Rejected {
            reason: "test".to_string(),
        };
        assert!(format!("{:?}", passed).contains("Passed"));
        assert!(format!("{:?}", rejected).contains("test"));
    }

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_long() {
        let result = truncate_str("hello world this is long", 10);
        assert!(result.len() <= 14); // 10 + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn validator_new_without_llm() {
        let v = WorkspaceValidator::new(None, None);
        assert!(v.llm.is_none());
        assert!(v.image_override.is_none());
    }

    #[test]
    fn validator_new_with_image() {
        let v = WorkspaceValidator::new(Some("custom:latest".to_string()), None);
        assert_eq!(v.image_override.as_deref(), Some("custom:latest"));
    }
}
