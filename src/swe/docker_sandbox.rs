//! Docker sandbox for isolated repository operations.
//!
//! Provides an ephemeral Docker container per task where all repo cloning,
//! dependency installation, test execution, and patch validation happen.

use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

/// Validate a GitHub repository name (`owner/repo`).
/// Rejects values containing shell metacharacters.
fn validate_repo_name(repo: &str) -> Result<()> {
    if repo.is_empty() {
        anyhow::bail!("repository name must not be empty");
    }
    let parts: Vec<&str> = repo.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        anyhow::bail!(
            "repository name must be in 'owner/repo' format, got '{}'",
            repo
        );
    }
    for ch in repo.chars() {
        if !ch.is_alphanumeric() && ch != '/' && ch != '-' && ch != '_' && ch != '.' {
            anyhow::bail!(
                "repository name contains invalid character '{}': '{}'",
                ch,
                repo
            );
        }
    }
    Ok(())
}

/// Validate a git ref (commit SHA, branch name, or ref range like `a..b`).
/// Only allows alphanumeric chars, `-`, `_`, `.`, `/`, `~`, `^`, and `..`.
fn validate_git_ref(git_ref: &str) -> Result<()> {
    if git_ref.is_empty() {
        anyhow::bail!("git ref must not be empty");
    }
    for ch in git_ref.chars() {
        if !ch.is_alphanumeric() && !"-_.~/^".contains(ch) {
            anyhow::bail!("git ref contains invalid character '{}': '{}'", ch, git_ref);
        }
    }
    Ok(())
}

/// Validate a file path for use inside a container.
/// Rejects path traversal (`..`), absolute paths, and shell metacharacters.
fn validate_container_path(path: &str) -> Result<()> {
    if path.is_empty() {
        anyhow::bail!("container path must not be empty");
    }
    if path.starts_with('/') {
        anyhow::bail!("container path must be relative, got '{}'", path);
    }
    if path.contains("..") {
        anyhow::bail!("container path must not contain '..': '{}'", path);
    }
    for ch in path.chars() {
        if ch == '\''
            || ch == '"'
            || ch == '`'
            || ch == '$'
            || ch == ';'
            || ch == '|'
            || ch == '&'
            || ch == '\n'
            || ch == '\r'
            || ch == '\0'
        {
            anyhow::bail!(
                "container path contains shell metacharacter '{}': '{}'",
                ch,
                path
            );
        }
    }
    Ok(())
}

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
        validate_repo_name(repo)?;
        if !base_commit.is_empty() {
            validate_git_ref(base_commit)?;
        }

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
                "--memory=32g",
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
            "git clone --depth 500 https://github.com/{}.git /repo 2>&1",
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

        // Checkout base commit (with unshallow fallback)
        if !base_commit.is_empty() {
            let checkout = sandbox
                .exec(
                    &format!("cd /repo && git checkout {} --force 2>&1", base_commit),
                    60_000,
                )
                .await;
            if checkout.exit_code != 0 {
                tracing::info!(
                    container = %sandbox.container_name,
                    commit = base_commit,
                    "Shallow clone missed commit, fetching full history..."
                );
                let unshallow = sandbox
                    .exec("cd /repo && git fetch --unshallow 2>&1", 300_000)
                    .await;
                if unshallow.exit_code != 0 {
                    tracing::warn!(
                        container = %sandbox.container_name,
                        stderr = %unshallow.stderr,
                        "Unshallow fetch failed"
                    );
                }
                let retry = sandbox
                    .exec(
                        &format!("cd /repo && git checkout {} --force 2>&1", base_commit),
                        60_000,
                    )
                    .await;
                if retry.exit_code != 0 {
                    tracing::warn!(
                        container = %sandbox.container_name,
                        commit = base_commit,
                        stderr = %retry.stderr,
                        "Checkout failed even after unshallow (continuing on HEAD)"
                    );
                }
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
        validate_container_path(path)?;

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
        validate_container_path(path)?;

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

    /// Auto-detect and run the install command based on project files.
    ///
    /// Checks for common dependency management files and runs the appropriate
    /// install command. Returns the working install command if one succeeds,
    /// or `None` if all attempts fail.
    pub async fn auto_detect_install(&self, language: &str) -> Option<String> {
        let candidates = match language.to_lowercase().as_str() {
            "python" => vec![
                ("setup.py", "pip install -e . 2>&1"),
                ("pyproject.toml", "pip install -e . 2>&1"),
                ("requirements.txt", "pip install -r requirements.txt 2>&1"),
                ("setup.py", "python setup.py develop 2>&1"),
            ],
            "javascript" | "typescript" | "js" | "ts" => vec![
                ("package.json", "npm install 2>&1"),
                ("yarn.lock", "yarn install 2>&1"),
                ("pnpm-lock.yaml", "pnpm install 2>&1"),
            ],
            "go" | "golang" => vec![("go.mod", "go mod download 2>&1")],
            "rust" => vec![("Cargo.toml", "cargo fetch 2>&1")],
            "java" | "kotlin" => vec![
                ("pom.xml", "./mvnw -q -DskipTests package 2>&1"),
                ("build.gradle", "./gradlew build -x test 2>&1"),
                ("build.gradle.kts", "./gradlew build -x test 2>&1"),
            ],
            _ => vec![],
        };

        for (marker_file, install_cmd) in &candidates {
            let check = self
                .exec(&format!("test -f /repo/{}", marker_file), 5_000)
                .await;
            if check.exit_code == 0 {
                tracing::debug!(
                    container = %self.container_name,
                    marker = marker_file,
                    cmd = install_cmd,
                    "Found dependency file, attempting install"
                );
                let result = self
                    .exec(&format!("cd /repo && {}", install_cmd), 300_000)
                    .await;
                if result.exit_code == 0 {
                    // Strip the trailing " 2>&1" for the clean command
                    let clean_cmd = install_cmd.trim_end_matches(" 2>&1").to_string();
                    tracing::info!(
                        container = %self.container_name,
                        cmd = %clean_cmd,
                        "Auto-detected install command succeeded"
                    );
                    return Some(clean_cmd);
                }
                tracing::debug!(
                    container = %self.container_name,
                    cmd = install_cmd,
                    exit = result.exit_code,
                    "Install attempt failed, trying next"
                );
            }
        }

        None
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

    #[test]
    fn test_truncate_short_string() {
        let result = truncate("hello", 10);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = truncate("hello world this is long", 10);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 13);
    }

    #[test]
    fn test_truncate_exact_boundary() {
        let result = truncate("12345", 5);
        assert_eq!(result, "12345");
    }

    #[test]
    fn test_truncate_empty() {
        let result = truncate("", 10);
        assert_eq!(result, "");
    }

    #[test]
    fn test_sandbox_output_construction() {
        let output = SandboxOutput {
            stdout: "hello".to_string(),
            stderr: "error".to_string(),
            exit_code: 1,
        };
        assert_eq!(output.stdout, "hello");
        assert_eq!(output.stderr, "error");
        assert_eq!(output.exit_code, 1);
    }

    #[test]
    fn test_sandbox_output_defaults() {
        let output = SandboxOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(output.stdout.is_empty());
        assert_eq!(output.exit_code, 0);
    }
}
