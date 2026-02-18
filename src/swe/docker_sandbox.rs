//! Docker sandbox for isolated repository operations.
//!
//! Provides an ephemeral Docker container per task where all repo cloning,
//! dependency installation, test execution, and patch validation happen.

use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

use crate::swe::tool_server::TOOL_SERVER_PY;
use crate::swe::{validate_file_path, validate_git_ref, validate_repo_name};

/// Shell command output from inside the container.
pub struct SandboxOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// An ephemeral Docker container for isolated repository operations.
pub struct DockerSandbox {
    container_name: String,
    /// Unique port for the tool server (needed because --network=host shares port space).
    tool_port: u16,
}

/// Pick a Docker image appropriate for the given language.
pub fn image_for_language(_language: &str) -> &'static str {
    "python:3.12-slim"
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
        let ts_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            % 1_000_000;
        let container_name = format!("swe-mine-{}-{}", safe_name, ts_suffix);
        // Derive unique tool server port from timestamp (range 10000-59999)
        let tool_port = 10_000 + (ts_suffix as u16 % 50_000);

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

        let sandbox = Self {
            container_name,
            tool_port,
        };

        // Install git (only hard dependency; agent installs everything else)
        let install = sandbox
            .exec(
                "apt-get update -qq && apt-get install -y -qq git > /dev/null 2>&1",
                120_000,
            )
            .await;
        if install.exit_code != 0 {
            tracing::warn!(
                container = %sandbox.container_name,
                stderr = %install.stderr,
                "git install failed (continuing)"
            );
        }

        // Clone the repository (full clone for reliable checkout)
        let clone_cmd = format!("git clone https://github.com/{}.git /repo 2>&1", repo);
        let clone = sandbox.exec(&clone_cmd, 600_000).await;
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

        // Inject and start the tool server
        sandbox.start_tool_server().await;

        tracing::info!(
            container = %sandbox.container_name,
            image = image,
            repo = repo,
            "Docker sandbox ready"
        );

        Ok(sandbox)
    }

    /// Write and start the Python tool server inside the container.
    async fn start_tool_server(&self) {
        // Write the server script
        let mkdir = self.exec("mkdir -p /tools", 10_000).await;
        if mkdir.exit_code != 0 {
            tracing::warn!(container = %self.container_name, "Failed to create /tools dir");
            return;
        }

        // Use write_file to inject the server script
        if let Err(e) = self
            .write_file_abs("/tools/server.py", TOOL_SERVER_PY)
            .await
        {
            tracing::warn!(container = %self.container_name, error = %e, "Failed to write tool server");
            return;
        }

        // Start server in background with unique port (--network=host shares port space)
        let start_cmd = format!(
            "nohup python3 -u /tools/server.py --port {} --cwd /repo > /tools/server.log 2>&1 &",
            self.tool_port
        );
        let start = self.exec(&start_cmd, 5_000).await;
        if start.exit_code != 0 {
            tracing::warn!(
                container = %self.container_name,
                stderr = %start.stderr,
                "Tool server start may have failed"
            );
        }

        // Health check: use python3 urllib (curl is not installed in slim images)
        for attempt in 0..6 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let health = self.tool_server_health().await;
            if health {
                tracing::debug!(container = %self.container_name, attempt = attempt, "Tool server healthy");
                return;
            }
        }
        // Log server output for debugging
        let log = self.exec("cat /tools/server.log 2>/dev/null", 5_000).await;
        tracing::warn!(
            container = %self.container_name,
            server_log = %log.stdout,
            "Tool server health check failed after 3s, tools may not work"
        );
    }

    /// Check if the tool server is healthy via python3 urllib inside the container.
    async fn tool_server_health(&self) -> bool {
        let check_cmd = format!(
            "import urllib.request; urllib.request.urlopen('http://localhost:{}/health')",
            self.tool_port
        );
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(2_000),
            Command::new("docker")
                .args([
                    "exec",
                    "-w",
                    "/repo",
                    &self.container_name,
                    "python3",
                    "-c",
                    &check_cmd,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        )
        .await;
        matches!(result, Ok(Ok(status)) if status.success())
    }

    /// Call a tool on the HTTP tool server running inside the container.
    /// Pipes the JSON args via stdin to avoid shell escaping issues.
    pub async fn tool_request(&self, tool_name: &str, args_json: &str) -> SandboxOutput {
        for ch in tool_name.chars() {
            if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_') {
                return SandboxOutput {
                    stdout: String::new(),
                    stderr: format!("Invalid tool name: {}", tool_name),
                    exit_code: -1,
                };
            }
        }

        let script = format!(
            "import sys, urllib.request, json\n\
             data = sys.stdin.read()\n\
             req = urllib.request.Request(\
               'http://localhost:{}/{}', \
               data=data.encode(), \
               headers={{'Content-Type': 'application/json'}})\n\
             try:\n\
               resp = urllib.request.urlopen(req, timeout=60)\n\
               print(resp.read().decode())\n\
             except Exception as e:\n\
               print(json.dumps({{'error': str(e)}}))",
            self.tool_port, tool_name
        );

        let result = tokio::time::timeout(std::time::Duration::from_millis(65_000), async {
            let mut child = Command::new("docker")
                .args([
                    "exec",
                    "-i",
                    "-w",
                    "/repo",
                    &self.container_name,
                    "python3",
                    "-c",
                    &script,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            if let Some(ref mut stdin) = child.stdin {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(args_json.as_bytes()).await?;
                stdin.shutdown().await?;
            }

            child.wait_with_output().await
        })
        .await;

        match result {
            Ok(Ok(output)) => SandboxOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            },
            Ok(Err(e)) => SandboxOutput {
                stdout: String::new(),
                stderr: format!("Tool request error: {}", e),
                exit_code: -1,
            },
            Err(_) => SandboxOutput {
                stdout: String::new(),
                stderr: "Tool request timed out after 65s".to_string(),
                exit_code: -1,
            },
        }
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

    /// Write a file to an absolute path inside the container.
    ///
    /// Only allows paths under known safe prefixes (`/tools/`).
    async fn write_file_abs(&self, abs_path: &str, content: &str) -> Result<()> {
        if !abs_path.starts_with("/tools/") {
            anyhow::bail!(
                "write_file_abs only allows paths under /tools/, got '{}'",
                abs_path
            );
        }
        for ch in abs_path.chars() {
            if matches!(
                ch,
                '\'' | '"'
                    | '`'
                    | '$'
                    | '!'
                    | '&'
                    | '|'
                    | ';'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
                    | '\\'
                    | '\0'
                    | '\n'
                    | '\r'
            ) {
                anyhow::bail!(
                    "invalid character in absolute path '{}': shell metacharacters not allowed",
                    abs_path
                );
            }
        }
        if abs_path.contains("..") {
            anyhow::bail!(
                "absolute path '{}' contains '..' (path traversal not allowed)",
                abs_path
            );
        }

        let mkdir_cmd = format!("mkdir -p \"$(dirname '{}')\"", abs_path);
        self.exec(&mkdir_cmd, 10_000).await;

        let tee_cmd = format!("cat > '{}'", abs_path);
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
                abs_path,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Write a file inside the container by piping content via stdin.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        validate_file_path(path)?;

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
        validate_file_path(path)?;

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
        assert_eq!(image_for_language("javascript"), "python:3.12-slim");
        assert_eq!(image_for_language("go"), "python:3.12-slim");
        assert_eq!(image_for_language("unknown"), "python:3.12-slim");
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
