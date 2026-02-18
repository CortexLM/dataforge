//! Pre-export workspace validation.
//!
//! Performs a complete end-to-end validation of a `SweTask` before it is
//! exported to disk. This catches tasks that would fail when run through
//! the harness (setup_error, sanity_fail) by verifying:
//!
//! 1. The repository can be cloned and the base commit checked out
//! 2. The install command succeeds
//! 3. `fail_to_pass` commands **fail** on the base commit
//! 4. `pass_to_pass` commands **pass** on the base commit
//! 5. After applying the PR patch, `fail_to_pass` commands **pass**
//! 6. After applying the PR patch, `pass_to_pass` commands still **pass**
//! 7. The prompt is feasible (non-empty, sufficient length, no test leaks)

use super::docker_sandbox::DockerSandbox;
use super::test_generator::TestFile;
use super::SweTask;

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
}

impl WorkspaceValidator {
    /// Create a new validator.
    pub fn new(image_override: Option<String>) -> Self {
        Self { image_override }
    }

    /// Run full end-to-end validation on a task.
    ///
    /// Creates a fresh Docker container, clones the repo, runs install,
    /// verifies test semantics on base and patched commits, then destroys
    /// the container.
    pub async fn validate(&self, task: &SweTask) -> Result<ValidationOutcome, anyhow::Error> {
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

        result
    }

    async fn run_validation(
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
                    "Runtime install failed during validation (continuing)"
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
                            "Install command failed (exit={}): {}",
                            install_result.exit_code,
                            truncate_str(&install_result.stderr, 500),
                        ),
                    });
                }
                tracing::debug!(
                    container = %sandbox.name(),
                    "Install command succeeded"
                );
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
                    let _ = sandbox.write_file(&tf.path, &tf.content).await;
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
            "Workspace validation PASSED"
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
}
