//! Sandbox environment for isolated agent execution.
//!
//! Provides Docker-based isolation for running agents safely.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for the sandbox environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker image to use.
    pub image: String,
    /// Memory limit in bytes.
    pub memory_limit: u64,
    /// CPU limit (number of cores).
    pub cpu_limit: f64,
    /// Timeout for the entire execution.
    pub timeout: Duration,
    /// Network mode ("none", "bridge", "host").
    pub network_mode: String,
    /// Whether to mount the task directory read-only.
    pub task_readonly: bool,
    /// Additional volume mounts.
    pub volumes: Vec<VolumeMount>,
    /// Environment variables.
    pub env_vars: Vec<(String, String)>,
}

impl SandboxConfig {
    /// Creates a new sandbox configuration with defaults.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            memory_limit: 32 * 1024 * 1024 * 1024, // 32GB
            cpu_limit: 0.0,
            timeout: Duration::from_secs(1800), // 30 minutes
            network_mode: "bridge".to_string(),
            task_readonly: true,
            volumes: Vec::new(),
            env_vars: Vec::new(),
        }
    }

    /// Sets the memory limit in MB.
    pub fn with_memory_mb(mut self, mb: u64) -> Self {
        self.memory_limit = mb * 1024 * 1024;
        self
    }

    /// Sets the CPU limit.
    pub fn with_cpu_limit(mut self, cores: f64) -> Self {
        self.cpu_limit = cores;
        self
    }

    /// Sets the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Disables network access.
    pub fn without_network(mut self) -> Self {
        self.network_mode = "none".to_string();
        self
    }

    /// Adds a volume mount.
    pub fn with_volume(mut self, mount: VolumeMount) -> Self {
        self.volumes.push(mount);
        self
    }

    /// Adds an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self::new("python:3.11-slim")
    }
}

/// Volume mount configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host path.
    pub host_path: PathBuf,
    /// Container path.
    pub container_path: PathBuf,
    /// Whether the mount is read-only.
    pub readonly: bool,
}

impl VolumeMount {
    /// Creates a new read-write volume mount.
    pub fn new(host: impl Into<PathBuf>, container: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host.into(),
            container_path: container.into(),
            readonly: false,
        }
    }

    /// Creates a read-only volume mount.
    pub fn readonly(host: impl Into<PathBuf>, container: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host.into(),
            container_path: container.into(),
            readonly: true,
        }
    }

    /// Returns the Docker mount string format.
    pub fn to_docker_mount(&self) -> String {
        let ro = if self.readonly { ":ro" } else { "" };
        format!(
            "{}:{}{}",
            self.host_path.display(),
            self.container_path.display(),
            ro
        )
    }
}

/// Sandbox for running agents in isolation.
pub struct Sandbox {
    /// Unique identifier for this sandbox.
    pub id: String,
    /// Configuration for the sandbox.
    pub config: SandboxConfig,
    /// Working directory inside the container.
    pub working_dir: PathBuf,
    /// Output directory on the host.
    pub output_dir: PathBuf,
    /// Whether the sandbox is currently active.
    active: bool,
}

impl Sandbox {
    /// Creates a new sandbox with the given configuration.
    pub fn new(config: SandboxConfig, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            id: format!("swe-forge-sandbox-{}", Uuid::new_v4()),
            config,
            working_dir: PathBuf::from("/workspace"),
            output_dir: output_dir.into(),
            active: false,
        }
    }

    /// Creates a sandbox with default configuration.
    pub fn with_defaults(output_dir: impl Into<PathBuf>) -> Self {
        Self::new(SandboxConfig::default(), output_dir)
    }

    /// Sets up the sandbox (creates directories, prepares Docker).
    pub async fn setup(&mut self, task_dir: &Path) -> Result<(), SandboxError> {
        info!("Setting up sandbox {}", self.id);

        // Create output directory
        std::fs::create_dir_all(&self.output_dir).map_err(|e| {
            SandboxError::Setup(format!("Failed to create output dir: {}", e))
        })?;

        // Copy task files to output directory (agent works on a copy)
        copy_dir_recursive(task_dir, &self.output_dir).map_err(|e| {
            SandboxError::Setup(format!("Failed to copy task files: {}", e))
        })?;

        self.active = true;
        debug!("Sandbox {} is ready", self.id);
        Ok(())
    }

    /// Gets the command to run inside the sandbox.
    pub fn docker_run_args(&self, command: &[String]) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            self.id.clone(),
        ];

        // Resource limits
        let gb = self.config.memory_limit / (1024 * 1024 * 1024);
        if gb > 0 {
            args.push(format!("--memory={}g", gb));
        } else {
            args.push(format!("--memory={}m", self.config.memory_limit / (1024 * 1024)));
        }
        if self.config.cpu_limit > 0.0 {
            args.push(format!("--cpus={}", self.config.cpu_limit));
        }

        args.extend([
            // Network
            format!("--network={}", self.config.network_mode),
            // Working directory
            "-w".to_string(),
            self.working_dir.to_string_lossy().to_string(),
        ]);

        // Add volume mounts
        args.push("-v".to_string());
        args.push(format!(
            "{}:{}",
            self.output_dir.display(),
            self.working_dir.display()
        ));

        for volume in &self.config.volumes {
            args.push("-v".to_string());
            args.push(volume.to_docker_mount());
        }

        // Add environment variables
        for (key, value) in &self.config.env_vars {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Image
        args.push(self.config.image.clone());

        // Command
        args.extend(command.iter().cloned());

        args
    }

    /// Cleans up the sandbox.
    pub async fn cleanup(&mut self) -> Result<(), SandboxError> {
        if !self.active {
            return Ok(());
        }

        info!("Cleaning up sandbox {}", self.id);

        // Try to stop and remove the container if it's still running
        let stop_result = tokio::process::Command::new("docker")
            .args(["stop", &self.id])
            .output()
            .await;

        if let Err(e) = stop_result {
            warn!("Failed to stop container {}: {}", self.id, e);
        }

        let rm_result = tokio::process::Command::new("docker")
            .args(["rm", "-f", &self.id])
            .output()
            .await;

        if let Err(e) = rm_result {
            warn!("Failed to remove container {}: {}", self.id, e);
        }

        self.active = false;
        Ok(())
    }

    /// Returns true if the sandbox is active.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.active {
            warn!("Sandbox {} was not cleaned up properly", self.id);
        }
    }
}

/// Error types for sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Setup failed: {0}")]
    Setup(String),

    #[error("Execution failed: {0}")]
    Execution(String),

    #[error("Cleanup failed: {0}")]
    Cleanup(String),

    #[error("Docker error: {0}")]
    Docker(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Recursively copies a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            std::fs::copy(&path, &dest_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.image, "python:3.11-slim");
        assert_eq!(config.cpu_limit, 0.0);
        assert_eq!(config.network_mode, "bridge");
    }

    #[test]
    fn test_sandbox_config_builder() {
        let config = SandboxConfig::new("ubuntu:22.04")
            .with_memory_mb(2048)
            .with_cpu_limit(1.0)
            .without_network()
            .with_env("MY_VAR", "value");

        assert_eq!(config.image, "ubuntu:22.04");
        assert_eq!(config.memory_limit, 2048 * 1024 * 1024);
        assert_eq!(config.cpu_limit, 1.0);
        assert_eq!(config.network_mode, "none");
        assert_eq!(config.env_vars.len(), 1);
    }

    #[test]
    fn test_volume_mount() {
        let mount = VolumeMount::new("/host/path", "/container/path");
        assert_eq!(mount.to_docker_mount(), "/host/path:/container/path");

        let ro_mount = VolumeMount::readonly("/host/ro", "/container/ro");
        assert_eq!(ro_mount.to_docker_mount(), "/host/ro:/container/ro:ro");
    }

    #[test]
    fn test_sandbox_docker_args() {
        let config = SandboxConfig::new("test:latest")
            .with_memory_mb(1024)
            .with_cpu_limit(1.0);

        let sandbox = Sandbox::new(config, "/tmp/output");
        let args = sandbox.docker_run_args(&["bash".to_string(), "-c".to_string(), "echo hello".to_string()]);

        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"test:latest".to_string()));
        assert!(args.contains(&"--memory=1g".to_string()));
        assert!(args.contains(&"--cpus=1".to_string()));
    }

    #[test]
    fn test_sandbox_docker_args_no_cpu() {
        let config = SandboxConfig::new("test:latest")
            .with_memory_mb(1024)
            .with_cpu_limit(0.0);

        let sandbox = Sandbox::new(config, "/tmp/output");
        let args = sandbox.docker_run_args(&["bash".to_string()]);

        assert!(args.contains(&"--memory=1g".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--cpus=")));
    }
}
