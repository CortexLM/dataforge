//! Docker sandbox for isolated repository operations.
//!
//! Provides an ephemeral Docker container per task where all repo cloning,
//! dependency installation, test execution, and patch validation happen.

use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

/// Shell command output from inside the container.
pub struct SandboxOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// An ephemeral Docker container for isolated repository operations.
pub struct DockerSandbox {
    container_name: String,
}

/// Pick a Docker image appropriate for the given language.
pub fn image_for_language(language: &str) -> &'static str {
    match language.to_lowercase().as_str() {
        "python" => "python:3.12-slim",
        "javascript" | "typescript" | "js" | "ts" => "node:20-slim",
        "go" | "golang" => "golang:1.22",
        "rust" => "rust:1.75-slim",
        "java" | "kotlin" => "eclipse-temurin:21-jdk",
        _ => "ubuntu:22.04",
    }
}

impl DockerSandbox {
    /// Start a new container, clone the repo at the given base commit.
    /// `image_override` takes precedence over language-based auto-selection.
    pub async fn start(
        repo: &str,
        base_commit: &str,
        language: &str,
        image_override: Option<&str>,
    ) -> Result<Self> {
        let image = image_override.unwrap_or_else(|| image_for_language(language));
        let safe_name = repo.replace('/', "-").replace(' ', "_");
        let container_name = format!(
            "swe-mine-{}-{}",
            safe_name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                % 1_000_000
        );

        // Remove stale container if it exists
        let _ = Command::new("docker")
            .args(["rm", "-f", &container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let run_output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &container_name,
                "--network=host",
                "--memory=4g",
                "--cpus=4",
                "-w",
                "/repo",
                image,
                "sleep",
                "7200",
            ])
            .output()
            .await?;

        if !run_output.status.success() {
            anyhow::bail!(
                "Failed to start Docker container '{}': {}",
                container_name,
                String::from_utf8_lossy(&run_output.stderr)
            );
        }

        let sandbox = Self { container_name };

        // Install basic system tools
        let install = sandbox
            .exec(
                "apt-get update -qq && apt-get install -y -qq git curl build-essential > /dev/null 2>&1",
                120_000,
            )
            .await;
        if install.exit_code != 0 {
            tracing::warn!(
                container = %sandbox.container_name,
                stderr = %install.stderr,
                "System deps install failed (continuing)"
            );
        }

        // Clone the repository
        let clone_cmd = format!(
            "git clone --depth 50 https://github.com/{}.git /repo 2>&1",
            repo
        );
        let clone = sandbox.exec(&clone_cmd, 180_000).await;
        if clone.exit_code != 0 {
            sandbox.destroy().await;
            anyhow::bail!(
                "Failed to clone {} in container: {}",
                repo,
                truncate(&clone.stderr, 500)
            );
        }

        // Checkout base commit
        if !base_commit.is_empty() {
            let checkout = sandbox
                .exec(
                    &format!("cd /repo && git checkout {} --force 2>&1", base_commit),
                    60_000,
                )
                .await;
            if checkout.exit_code != 0 {
                tracing::warn!(
                    container = %sandbox.container_name,
                    commit = base_commit,
                    stderr = %checkout.stderr,
                    "Checkout failed (continuing on HEAD)"
                );
            }
        }

        tracing::info!(
            container = %sandbox.container_name,
            image = image,
            repo = repo,
            "Docker sandbox ready"
        );

        Ok(sandbox)
    }

    /// Execute a shell command inside the container.
    pub async fn exec(&self, cmd: &str, timeout_ms: u64) -> SandboxOutput {
        let timeout_secs = (timeout_ms / 1000).max(1);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            Command::new("docker")
                .args([
                    "exec",
                    "-w",
                    "/repo",
                    &self.container_name,
                    "bash",
                    "-c",
                    cmd,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => SandboxOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            },
            Ok(Err(e)) => SandboxOutput {
                stdout: String::new(),
                stderr: format!("Docker exec error: {}", e),
                exit_code: -1,
            },
            Err(_) => SandboxOutput {
                stdout: String::new(),
                stderr: format!("Command timed out after {}s", timeout_secs),
                exit_code: -1,
            },
        }
    }

    /// Write a file inside the container by piping content via stdin.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        // First ensure the parent directory exists
        let mkdir_cmd = format!("mkdir -p \"$(dirname '/repo/{}')\"", path);
        self.exec(&mkdir_cmd, 10_000).await;

        // Use docker exec -i to pipe content via stdin
        let tee_cmd = format!("cat > '/repo/{}'", path);
        let mut child = Command::new("docker")
            .args([
                "exec",
                "-i",
                "-w",
                "/repo",
                &self.container_name,
                "bash",
                "-c",
                &tee_cmd,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(ref mut stdin) = child.stdin {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(content.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to write file '{}' in container: {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Read a file from inside the container.
    pub async fn read_file(&self, path: &str) -> Result<String> {
        let cmd = format!("cat '/repo/{}'", path);
        let result = self.exec(&cmd, 10_000).await;
        if result.exit_code != 0 {
            anyhow::bail!(
                "Failed to read file '{}' in container: {}",
                path,
                result.stderr
            );
        }
        Ok(result.stdout)
    }

    /// Destroy the container.
    pub async fn destroy(&self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        tracing::debug!(container = %self.container_name, "Docker sandbox destroyed");
    }

    /// Get the container name (useful for logging).
    pub fn name(&self) -> &str {
        &self.container_name
    }
}

/// Ensure the sandbox is destroyed when dropped (best-effort sync cleanup).
impl Drop for DockerSandbox {
    fn drop(&mut self) {
        let name = self.container_name.clone();
        // Fire-and-forget: spawn a blocking task so we don't need async in Drop
        std::thread::spawn(move || {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", &name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }
}

fn truncate(s: &str, max: usize) -> String {
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
    fn test_image_for_language() {
        assert_eq!(image_for_language("python"), "python:3.12-slim");
        assert_eq!(image_for_language("Python"), "python:3.12-slim");
        assert_eq!(image_for_language("javascript"), "node:20-slim");
        assert_eq!(image_for_language("typescript"), "node:20-slim");
        assert_eq!(image_for_language("go"), "golang:1.22");
        assert_eq!(image_for_language("rust"), "rust:1.75-slim");
        assert_eq!(image_for_language("java"), "eclipse-temurin:21-jdk");
        assert_eq!(image_for_language("unknown"), "ubuntu:22.04");
    }
}
