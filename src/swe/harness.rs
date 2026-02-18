//! SWE-bench evaluation harness.
//!
//! Runs an external agent on mined SWE tasks inside Docker containers,
//! then verifies results by executing fail_to_pass / pass_to_pass test commands.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{info, warn};

use super::{validate_file_path, validate_git_ref, validate_repo_name, SweTask};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub agent_dir: PathBuf,
    pub agent_cmd: String,
    pub agent_timeout_secs: u64,
    pub test_timeout_secs: u64,
    pub docker_image: String,
    pub keep_containers: bool,
    pub parallel: usize,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            agent_dir: PathBuf::from("."),
            agent_cmd: "python -m baseagent".to_string(),
            agent_timeout_secs: 600,
            test_timeout_secs: 120,
            docker_image: "python:3.12-slim".to_string(),
            keep_containers: false,
            parallel: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessStatus {
    Resolved,
    Unresolved,
    AgentError,
    TestError,
    SetupError,
    SanityFail,
}

impl std::fmt::Display for HarnessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolved => write!(f, "resolved"),
            Self::Unresolved => write!(f, "unresolved"),
            Self::AgentError => write!(f, "agent_error"),
            Self::TestError => write!(f, "test_error"),
            Self::SetupError => write!(f, "setup_error"),
            Self::SanityFail => write!(f, "sanity_fail"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub passed: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessResult {
    pub task_id: String,
    pub repo: String,
    pub status: HarnessStatus,
    pub sanity_check: bool,
    pub fail_to_pass: Vec<TestResult>,
    pub pass_to_pass: Vec<TestResult>,
    pub agent_duration_secs: f64,
    pub total_duration_secs: f64,
    pub agent_output: String,
    pub error: Option<String>,
    pub container_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessSummary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
    pub agent_error: usize,
    pub test_error: usize,
    pub setup_error: usize,
    pub sanity_fail: usize,
    pub avg_agent_time_secs: f64,
    pub results: Vec<HarnessResult>,
}

// ---------------------------------------------------------------------------
// Docker helpers
// ---------------------------------------------------------------------------

async fn docker_exec(container: &str, cmd: &str, timeout_secs: u64) -> (i32, String, String) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        Command::new("docker")
            .args(["exec", container, "bash", "-c", cmd])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ),
        Ok(Err(e)) => (-1, String::new(), format!("exec error: {e}")),
        Err(_) => (
            -1,
            String::new(),
            format!("timed out after {timeout_secs}s"),
        ),
    }
}

async fn docker_rm(container: &str) {
    if let Err(e) = Command::new("docker")
        .args(["rm", "-f", container])
        .output()
        .await
    {
        tracing::debug!(container = container, error = %e, "Failed to remove container (may not exist)");
    }
}

async fn docker_write_file(container: &str, path: &str, content: &str) -> Result<()> {
    validate_file_path(path)?;

    use tokio::io::AsyncWriteExt;
    let tee_cmd = format!("cat > '/repo/{}'", path);
    let mut child = Command::new("docker")
        .args([
            "exec", "-i", "-w", "/repo", container, "bash", "-c", &tee_cmd,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(content.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        anyhow::bail!("write failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn container_name(task_id: &str) -> String {
    let safe = task_id.replace('/', "-").replace(' ', "_");
    format!("swe-harness-{safe}")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}... [truncated]", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// Per-task evaluation
// ---------------------------------------------------------------------------

async fn evaluate_task(task: &SweTask, config: &HarnessConfig) -> HarnessResult {
    let total_start = Instant::now();
    let cname = container_name(&task.id);
    let mut result = HarnessResult {
        task_id: task.id.clone(),
        repo: task.repo.clone(),
        status: HarnessStatus::SetupError,
        sanity_check: false,
        fail_to_pass: Vec::new(),
        pass_to_pass: Vec::new(),
        agent_duration_secs: 0.0,
        total_duration_secs: 0.0,
        agent_output: String::new(),
        error: None,
        container_id: Some(cname.clone()),
    };

    // 0. INPUT VALIDATION: reject shell-unsafe repo names and commit refs
    if let Err(e) = validate_repo_name(&task.repo) {
        result.error = Some(format!("Invalid repo name: {e}"));
        return result;
    }
    if !task.base_commit.is_empty() {
        if let Err(e) = validate_git_ref(&task.base_commit) {
            result.error = Some(format!("Invalid base commit: {e}"));
            return result;
        }
    }

    // 1. SETUP: start container
    info!(task_id = %task.id, "Starting container {}", cname);
    let agent_dir_abs = if let Ok(docker_dir) = std::env::var("DOCKER_AGENT_DIR") {
        PathBuf::from(docker_dir)
    } else {
        std::fs::canonicalize(&config.agent_dir).unwrap_or_else(|_| config.agent_dir.clone())
    };

    let docker_image = config.docker_image.clone();
    info!(task_id = %task.id, language = %task.language, image = %docker_image, "Selected Docker image");

    // Remove stale container if exists
    docker_rm(&cname).await;

    let openrouter_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    let openrouter_env = format!("OPENROUTER_API_KEY={}", openrouter_key);
    let start_output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &cname,
            "--network=host",
            "--memory=32g",
            "-e",
            &openrouter_env,
            "-v",
            &format!("{}:/agent:ro", agent_dir_abs.display()),
            "-w",
            "/repo",
            &docker_image,
            "sleep",
            "7200",
        ])
        .output()
        .await;

    match start_output {
        Ok(o) if o.status.success() => {
            info!(task_id = %task.id, "Container started");
        }
        Ok(o) => {
            result.error = Some(format!(
                "Failed to start container: {}",
                String::from_utf8_lossy(&o.stderr)
            ));
            return result;
        }
        Err(e) => {
            result.error = Some(format!("Docker not available: {e}"));
            return result;
        }
    }

    // Install system tools (always include python3 + pip for the agent)
    let (code, _, err) = docker_exec(
        &cname,
        "apt-get update -qq && apt-get install -y -qq git curl build-essential python3 python3-pip python3-venv > /dev/null 2>&1 && (command -v python >/dev/null 2>&1 || ln -sf $(command -v python3) /usr/local/bin/python)",
        180,
    )
    .await;
    if code != 0 {
        result.error = Some(format!(
            "Failed to install system deps: {}",
            truncate(&err, 500)
        ));
        return result;
    }

    // Clone repo (full clone for reliable checkout)
    let clone_cmd = format!("git clone https://github.com/{}.git /repo 2>&1", task.repo);
    let (code, _, err) = docker_exec(&cname, &clone_cmd, 600).await;
    if code != 0 {
        result.error = Some(format!("Clone failed: {}", truncate(&err, 500)));
        return result;
    }

    // Checkout base commit
    if !task.base_commit.is_empty() {
        let (code, _, err) = docker_exec(
            &cname,
            &format!("cd /repo && git checkout {} --force 2>&1", task.base_commit),
            60,
        )
        .await;
        if code != 0 {
            result.error = Some(format!("Checkout failed: {}", truncate(&err, 500)));
            return result;
        }
    }

    // Install project deps from install_config
    if let Some(install_cmd) = task.install_config.get("install") {
        if !install_cmd.is_empty() {
            info!(task_id = %task.id, "Installing deps: {}", install_cmd);
            let (code, _, err) =
                docker_exec(&cname, &format!("cd /repo && {} 2>&1", install_cmd), 300).await;
            if code != 0 {
                warn!(task_id = %task.id, "Install command failed (continuing): {}", truncate(&err, 200));
            }
        }
    }

    // Install agent requirements
    let (code, _, _) = docker_exec(
        &cname,
        "test -f /agent/requirements.txt && pip install --break-system-packages -q -r /agent/requirements.txt 2>&1 || true",
        180,
    )
    .await;
    if code != 0 {
        warn!(task_id = %task.id, "Agent requirements install returned non-zero (continuing)");
    }

    // Copy test files into container
    if let Some(test_files_json) = task.meta.get("test_files") {
        if let Ok(files) =
            serde_json::from_str::<Vec<super::test_generator::TestFile>>(test_files_json)
        {
            for tf in &files {
                if let Err(e) = validate_file_path(&tf.path) {
                    warn!(task_id = %task.id, path = %tf.path, error = %e, "Skipping test file with invalid path");
                    continue;
                }
                let mkdir_cmd = format!("mkdir -p \"$(dirname '/repo/{}')\"", tf.path);
                docker_exec(&cname, &mkdir_cmd, 10).await;
                let write_result = docker_write_file(&cname, &tf.path, &tf.content).await;
                if let Err(e) = write_result {
                    warn!(task_id = %task.id, path = %tf.path, "Failed to copy test file: {}", e);
                }
            }
            info!(task_id = %task.id, "Copied {} test files into container", files.len());
        }
    }

    // 2. SANITY CHECK: fail_to_pass must fail, pass_to_pass must pass
    info!(task_id = %task.id, "Running sanity checks...");

    for cmd_str in &task.fail_to_pass {
        let (code, _, _) = docker_exec(
            &cname,
            &format!("cd /repo && {}", cmd_str),
            config.test_timeout_secs,
        )
        .await;
        if code == 0 {
            info!(task_id = %task.id, "Sanity fail: fail_to_pass command already passes: {}", cmd_str);
            result.status = HarnessStatus::SanityFail;
            result.error = Some(format!(
                "fail_to_pass command already passes on base commit: {}",
                cmd_str
            ));
            if !config.keep_containers {
                docker_rm(&cname).await;
            }
            result.total_duration_secs = total_start.elapsed().as_secs_f64();
            return result;
        }
    }

    for cmd_str in &task.pass_to_pass {
        let (code, _, _) = docker_exec(
            &cname,
            &format!("cd /repo && {}", cmd_str),
            config.test_timeout_secs,
        )
        .await;
        if code != 0 {
            info!(task_id = %task.id, "Sanity fail: pass_to_pass command fails on base commit: {}", cmd_str);
            result.status = HarnessStatus::SanityFail;
            result.error = Some(format!(
                "pass_to_pass command fails on base commit: {}",
                cmd_str
            ));
            if !config.keep_containers {
                docker_rm(&cname).await;
            }
            result.total_duration_secs = total_start.elapsed().as_secs_f64();
            return result;
        }
    }

    result.sanity_check = true;
    info!(task_id = %task.id, "Sanity check passed");

    // 3. RUN AGENT
    info!(task_id = %task.id, "Running agent: {}", config.agent_cmd);
    let prompt_escaped = task.prompt.replace('\'', "'\\''");
    let agent_full_cmd = format!(
        "cd /repo && {} --prompt '{}' --workdir /repo 2>&1",
        config.agent_cmd, prompt_escaped
    );

    let agent_start = Instant::now();
    let (agent_code, agent_stdout, agent_stderr) =
        docker_exec(&cname, &agent_full_cmd, config.agent_timeout_secs).await;
    result.agent_duration_secs = agent_start.elapsed().as_secs_f64();
    result.agent_output = truncate(&format!("{agent_stdout}\n{agent_stderr}"), 10_000);

    if agent_code != 0 && agent_stderr.contains("timed out") {
        result.status = HarnessStatus::AgentError;
        result.error = Some(format!(
            "Agent timed out after {}s",
            config.agent_timeout_secs
        ));
        if !config.keep_containers {
            docker_rm(&cname).await;
        }
        result.total_duration_secs = total_start.elapsed().as_secs_f64();
        return result;
    }

    if agent_code != 0 {
        warn!(task_id = %task.id, exit_code = agent_code, "Agent exited with non-zero code (continuing to test)");
    }

    // 4. VERIFY: run all test commands
    info!(task_id = %task.id, "Verifying test results...");

    let mut all_f2p_pass = true;
    for cmd_str in &task.fail_to_pass {
        let cmd_start = Instant::now();
        let (code, stdout, stderr) = docker_exec(
            &cname,
            &format!("cd /repo && {}", cmd_str),
            config.test_timeout_secs,
        )
        .await;
        let passed = code == 0;
        if !passed {
            all_f2p_pass = false;
        }
        result.fail_to_pass.push(TestResult {
            command: cmd_str.clone(),
            exit_code: code,
            stdout: truncate(&stdout, 2000),
            stderr: truncate(&stderr, 2000),
            passed,
            duration_ms: cmd_start.elapsed().as_millis() as u64,
        });
    }

    let mut all_p2p_pass = true;
    for cmd_str in &task.pass_to_pass {
        let cmd_start = Instant::now();
        let (code, stdout, stderr) = docker_exec(
            &cname,
            &format!("cd /repo && {}", cmd_str),
            config.test_timeout_secs,
        )
        .await;
        let passed = code == 0;
        if !passed {
            all_p2p_pass = false;
        }
        result.pass_to_pass.push(TestResult {
            command: cmd_str.clone(),
            exit_code: code,
            stdout: truncate(&stdout, 2000),
            stderr: truncate(&stderr, 2000),
            passed,
            duration_ms: cmd_start.elapsed().as_millis() as u64,
        });
    }

    // Determine final status
    if all_f2p_pass && all_p2p_pass {
        result.status = HarnessStatus::Resolved;
        info!(task_id = %task.id, "RESOLVED");
    } else {
        result.status = HarnessStatus::Unresolved;
        let f2p_passed = result.fail_to_pass.iter().filter(|t| t.passed).count();
        let p2p_passed = result.pass_to_pass.iter().filter(|t| t.passed).count();
        info!(
            task_id = %task.id,
            "UNRESOLVED (f2p: {}/{}, p2p: {}/{})",
            f2p_passed, result.fail_to_pass.len(),
            p2p_passed, result.pass_to_pass.len()
        );
    }

    // 5. CLEANUP
    if !config.keep_containers {
        docker_rm(&cname).await;
    }

    result.total_duration_secs = total_start.elapsed().as_secs_f64();
    result
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load SweTask from a workspace.yaml file.
pub fn load_task(workspace_yaml: &Path) -> Result<SweTask> {
    let content = std::fs::read_to_string(workspace_yaml)?;
    let task: SweTask = serde_yaml::from_str(&content)?;
    Ok(task)
}

/// Discover all workspace.yaml files under a directory.
pub fn discover_tasks(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    fn walk(dir: &Path, paths: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, paths);
                } else if p
                    .file_name()
                    .map(|f| f == "workspace.yaml")
                    .unwrap_or(false)
                {
                    paths.push(p);
                }
            }
        }
    }
    walk(input_dir, &mut paths);
    paths.sort();
    Ok(paths)
}

/// Run the full harness on all tasks in a directory.
pub async fn run_harness(input_dir: &Path, config: &HarnessConfig) -> Result<HarnessSummary> {
    let yaml_paths = discover_tasks(input_dir)?;
    if yaml_paths.is_empty() {
        anyhow::bail!("No workspace.yaml files found in {}", input_dir.display());
    }

    info!(
        "Discovered {} tasks in {}",
        yaml_paths.len(),
        input_dir.display()
    );

    let mut tasks = Vec::new();
    for path in &yaml_paths {
        match load_task(path) {
            Ok(task) => tasks.push(task),
            Err(e) => warn!("Failed to load {}: {}", path.display(), e),
        }
    }

    if tasks.is_empty() {
        anyhow::bail!("No valid tasks found");
    }

    info!(
        "Loaded {} valid tasks, running with parallelism={}",
        tasks.len(),
        config.parallel
    );

    let mut results = Vec::new();

    // Process in chunks of `parallel` size
    for chunk in tasks.chunks(config.parallel) {
        let mut handles = Vec::new();
        for task in chunk {
            let task = task.clone();
            let cfg = config.clone();
            handles.push(tokio::spawn(
                async move { evaluate_task(&task, &cfg).await },
            ));
        }
        for handle in handles {
            match handle.await {
                Ok(r) => results.push(r),
                Err(e) => warn!("Task panicked: {e}"),
            }
        }
    }

    // Build summary
    let total = results.len();
    let resolved = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::Resolved))
        .count();
    let unresolved = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::Unresolved))
        .count();
    let agent_error = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::AgentError))
        .count();
    let test_error = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::TestError))
        .count();
    let setup_error = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::SetupError))
        .count();
    let sanity_fail = results
        .iter()
        .filter(|r| matches!(r.status, HarnessStatus::SanityFail))
        .count();

    let agent_times: Vec<f64> = results
        .iter()
        .filter(|r| r.agent_duration_secs > 0.0)
        .map(|r| r.agent_duration_secs)
        .collect();
    let avg_agent_time = if agent_times.is_empty() {
        0.0
    } else {
        agent_times.iter().sum::<f64>() / agent_times.len() as f64
    };

    Ok(HarnessSummary {
        total,
        resolved,
        unresolved,
        agent_error,
        test_error,
        setup_error,
        sanity_fail,
        avg_agent_time_secs: (avg_agent_time * 10.0).round() / 10.0,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_config_default() {
        let config = HarnessConfig::default();
        assert_eq!(config.agent_timeout_secs, 600);
        assert_eq!(config.test_timeout_secs, 120);
        assert_eq!(config.docker_image, "python:3.12-slim");
        assert!(!config.keep_containers);
        assert_eq!(config.parallel, 1);
        assert_eq!(config.agent_cmd, "python -m baseagent");
    }

    #[test]
    fn test_harness_status_display() {
        assert_eq!(format!("{}", HarnessStatus::Resolved), "resolved");
        assert_eq!(format!("{}", HarnessStatus::Unresolved), "unresolved");
        assert_eq!(format!("{}", HarnessStatus::AgentError), "agent_error");
        assert_eq!(format!("{}", HarnessStatus::TestError), "test_error");
        assert_eq!(format!("{}", HarnessStatus::SetupError), "setup_error");
        assert_eq!(format!("{}", HarnessStatus::SanityFail), "sanity_fail");
    }

    #[test]
    fn test_container_name_basic() {
        let name = container_name("owner/repo-123");
        assert_eq!(name, "swe-harness-owner-repo-123");
    }

    #[test]
    fn test_container_name_with_spaces() {
        let name = container_name("owner/repo name");
        assert_eq!(name, "swe-harness-owner-repo_name");
    }

    #[test]
    fn test_container_name_slashes() {
        let name = container_name("org/sub/repo");
        assert_eq!(name, "swe-harness-org-sub-repo");
    }

    #[test]
    fn test_truncate_short() {
        let result = truncate("hello", 10);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world this is a long string", 10);
        assert!(result.len() <= 25); // 10 + "... [truncated]"
        assert!(result.ends_with("... [truncated]"));
    }

    #[test]
    fn test_truncate_exact_boundary() {
        let result = truncate("12345", 5);
        assert_eq!(result, "12345");
    }

    #[test]
    fn test_truncate_unicode() {
        let result = truncate("héllo wörld", 5);
        assert!(result.ends_with("... [truncated]"));
    }

    #[test]
    fn test_discover_tasks_empty_dir() {
        let tmp = std::env::temp_dir().join("swe_forge_test_discover_empty");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let paths = discover_tasks(&tmp).unwrap();
        assert!(paths.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_tasks_finds_workspace_yaml() {
        let tmp = std::env::temp_dir().join("swe_forge_test_discover_yaml");
        let _ = std::fs::remove_dir_all(&tmp);
        let sub = tmp.join("task-1");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("workspace.yaml"), "id: test").unwrap();

        let paths = discover_tasks(&tmp).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("workspace.yaml"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_harness_result_serialization() {
        let result = HarnessResult {
            task_id: "test-1".to_string(),
            repo: "owner/repo".to_string(),
            status: HarnessStatus::Resolved,
            sanity_check: true,
            fail_to_pass: vec![],
            pass_to_pass: vec![],
            agent_duration_secs: 10.5,
            total_duration_secs: 15.0,
            agent_output: "output".to_string(),
            error: None,
            container_id: Some("cname".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"task_id\":\"test-1\""));
        assert!(json.contains("\"resolved\""));
    }

    #[test]
    fn test_harness_summary_serialization() {
        let summary = HarnessSummary {
            total: 5,
            resolved: 3,
            unresolved: 1,
            agent_error: 0,
            test_error: 0,
            setup_error: 1,
            sanity_fail: 0,
            avg_agent_time_secs: 12.5,
            results: vec![],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"total\":5"));
        assert!(json.contains("\"resolved\":3"));
    }

    #[test]
    fn test_test_result_serialization() {
        let tr = TestResult {
            command: "pytest test.py".to_string(),
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
            passed: true,
            duration_ms: 1500,
        };
        let json = serde_json::to_string(&tr).unwrap();
        assert!(json.contains("\"passed\":true"));
        assert!(json.contains("\"duration_ms\":1500"));
    }
}
