//! Docker API wrapper using the bollard crate.
//!
//! This module provides a high-level interface to Docker operations
//! for container lifecycle management.

use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, LogOutput, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;

use crate::error::DockerError;
use crate::execution::resources::ExecutionLimits;

/// Configuration for creating a new container.
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// Unique name/ID for the container.
    pub name: String,
    /// Docker image to use.
    pub image: String,
    /// Command to run in the container.
    pub cmd: Option<Vec<String>>,
    /// Environment variables.
    pub env: Vec<String>,
    /// Working directory inside the container.
    pub working_dir: Option<String>,
    /// Resource limits for the container.
    pub limits: ExecutionLimits,
    /// Volume mounts (host:container format).
    pub volumes: Vec<String>,
    /// User to run as (e.g., "1000:1000").
    pub user: Option<String>,
    /// Network mode (e.g., "none", "bridge", "host").
    pub network_mode: Option<String>,
}

impl ContainerConfig {
    /// Creates a new container configuration with the given name and image.
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            cmd: None,
            env: Vec::new(),
            working_dir: None,
            limits: ExecutionLimits::default(),
            volumes: Vec::new(),
            user: None,
            network_mode: Some("bridge".to_string()),
        }
    }

    /// Sets the difficulty level, which determines resource limits.
    pub fn with_difficulty(mut self, difficulty: &str) -> Self {
        self.limits = crate::execution::get_execution_limits(difficulty);
        self
    }

    /// Sets explicit resource limits.
    pub fn with_limits(mut self, limits: ExecutionLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Sets the command to run in the container.
    pub fn with_cmd(mut self, cmd: Vec<String>) -> Self {
        self.cmd = Some(cmd);
        self
    }

    /// Adds environment variables.
    pub fn with_env(mut self, env: Vec<String>) -> Self {
        self.env = env;
        self
    }

    /// Sets the working directory.
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Adds volume mounts.
    pub fn with_volumes(mut self, volumes: Vec<String>) -> Self {
        self.volumes = volumes;
        self
    }

    /// Sets the user to run as.
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Sets the network mode.
    pub fn with_network_mode(mut self, mode: impl Into<String>) -> Self {
        self.network_mode = Some(mode.into());
        self
    }
}

/// Status of a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerStatusInfo {
    /// Container is created but not started.
    Created,
    /// Container is running.
    Running,
    /// Container is paused.
    Paused,
    /// Container is restarting.
    Restarting,
    /// Container has exited.
    Exited { exit_code: i64 },
    /// Container is being removed.
    Removing,
    /// Container is dead.
    Dead,
    /// Unknown status.
    Unknown(String),
}

/// Result of executing a command in a container.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Exit code of the command.
    pub exit_code: i64,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

/// Docker client wrapper for container operations.
pub struct DockerClient {
    docker: Docker,
}

impl DockerClient {
    /// Creates a new Docker client connecting to the local Docker daemon.
    ///
    /// # Errors
    ///
    /// Returns `DockerError::DaemonUnavailable` if the Docker daemon is not accessible.
    pub fn new() -> Result<Self, DockerError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| DockerError::DaemonUnavailable(format!("Failed to connect: {e}")))?;

        Ok(Self { docker })
    }

    /// Creates a new Docker client from an existing bollard Docker instance.
    pub fn from_docker(docker: Docker) -> Self {
        Self { docker }
    }

    /// Creates a new container with the given configuration.
    ///
    /// # Returns
    ///
    /// The container ID on success.
    pub async fn create_container(&self, config: ContainerConfig) -> Result<String, DockerError> {
        let host_config = HostConfig {
            memory: Some(config.limits.memory_bytes()),
            cpu_period: Some(config.limits.cpu_period()),
            cpu_quota: Some(config.limits.cpu_quota()),
            pids_limit: Some(config.limits.max_processes as i64),
            network_mode: config.network_mode.clone(),
            binds: if config.volumes.is_empty() {
                None
            } else {
                Some(config.volumes.clone())
            },
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.image.clone()),
            cmd: config.cmd.clone().map(|c| c.into_iter().collect()),
            env: if config.env.is_empty() {
                None
            } else {
                Some(config.env.clone())
            },
            working_dir: config.working_dir.clone(),
            user: config.user.clone(),
            host_config: Some(host_config),
            tty: Some(true),
            attach_stdin: Some(false),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: config.name.clone(),
            platform: None,
        };

        let response = self
            .docker
            .create_container(Some(options), container_config)
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to create container: {e}")))?;

        Ok(response.id)
    }

    /// Starts a container by ID.
    pub async fn start_container(&self, id: &str) -> Result<(), DockerError> {
        self.docker
            .start_container(id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to start container: {e}")))?;

        Ok(())
    }

    /// Stops a container by ID.
    ///
    /// Sends SIGTERM and waits up to 10 seconds before sending SIGKILL.
    pub async fn stop_container(&self, id: &str) -> Result<(), DockerError> {
        let options = StopContainerOptions { t: 10 };

        self.docker
            .stop_container(id, Some(options))
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to stop container: {e}")))?;

        Ok(())
    }

    /// Removes a container by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID
    /// * `force` - Force removal even if running
    pub async fn remove_container(&self, id: &str, force: bool) -> Result<(), DockerError> {
        let options = RemoveContainerOptions {
            force,
            v: true, // Remove volumes
            ..Default::default()
        };

        self.docker
            .remove_container(id, Some(options))
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to remove container: {e}")))?;

        Ok(())
    }

    /// Executes a command inside a running container.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID
    /// * `cmd` - Command to execute as array of strings
    ///
    /// # Returns
    ///
    /// `ExecResult` containing exit code, stdout, and stderr.
    pub async fn exec_command(&self, id: &str, cmd: &[&str]) -> Result<ExecResult, DockerError> {
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.to_vec()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(id, exec_options)
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to create exec: {e}")))?;

        let start_result = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to start exec: {e}")))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = start_result {
            while let Some(chunk) = output.next().await {
                match chunk {
                    Ok(LogOutput::StdOut { message }) => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(LogOutput::StdErr { message }) => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        return Err(DockerError::RunFailed(format!("Error reading output: {e}")));
                    }
                }
            }
        }

        // Get exit code from exec inspect
        let exec_info = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| DockerError::RunFailed(format!("Failed to inspect exec: {e}")))?;

        let exit_code = exec_info.exit_code.unwrap_or(-1);

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Gets logs from a container.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID
    ///
    /// # Returns
    ///
    /// Combined stdout and stderr logs as a string.
    pub async fn get_logs(&self, id: &str) -> Result<String, DockerError> {
        let options = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow: false,
            timestamps: false,
            ..Default::default()
        };

        let mut logs = self.docker.logs(id, Some(options));
        let mut output = String::new();

        while let Some(chunk) = logs.next().await {
            match chunk {
                Ok(LogOutput::StdOut { message }) => {
                    output.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::StdErr { message }) => {
                    output.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(DockerError::RunFailed(format!("Error reading logs: {e}")));
                }
            }
        }

        Ok(output)
    }

    /// Gets the status of a container.
    pub async fn container_status(&self, id: &str) -> Result<ContainerStatusInfo, DockerError> {
        let info = self
            .docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| {
                if e.to_string().contains("No such container") {
                    DockerError::ContainerNotFound { id: id.to_string() }
                } else {
                    DockerError::RunFailed(format!("Failed to inspect container: {e}"))
                }
            })?;

        let state = info
            .state
            .ok_or_else(|| DockerError::RunFailed("Container has no state".to_string()))?;

        let status = state.status.map(|s| s.to_string()).unwrap_or_default();

        match status.as_str() {
            "created" => Ok(ContainerStatusInfo::Created),
            "running" => Ok(ContainerStatusInfo::Running),
            "paused" => Ok(ContainerStatusInfo::Paused),
            "restarting" => Ok(ContainerStatusInfo::Restarting),
            "removing" => Ok(ContainerStatusInfo::Removing),
            "exited" => Ok(ContainerStatusInfo::Exited {
                exit_code: state.exit_code.unwrap_or(-1),
            }),
            "dead" => Ok(ContainerStatusInfo::Dead),
            other => Ok(ContainerStatusInfo::Unknown(other.to_string())),
        }
    }

    /// Pulls a Docker image from a registry.
    ///
    /// # Arguments
    ///
    /// * `image` - Image name with optional tag (e.g., "python:3.11-slim")
    pub async fn pull_image(&self, image: &str) -> Result<(), DockerError> {
        let options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            result.map_err(|e| DockerError::BuildFailed(format!("Failed to pull image: {e}")))?;
        }

        Ok(())
    }

    /// Checks if an image exists locally.
    pub async fn image_exists(&self, image: &str) -> bool {
        self.docker.inspect_image(image).await.is_ok()
    }

    /// Waits for a container to finish executing.
    ///
    /// # Returns
    ///
    /// The exit code of the container.
    pub async fn wait_container(&self, id: &str) -> Result<i64, DockerError> {
        use bollard::container::WaitContainerOptions;

        let options = WaitContainerOptions {
            condition: "not-running",
        };

        let mut stream = self.docker.wait_container(id, Some(options));

        if let Some(result) = stream.next().await {
            let wait_response = result
                .map_err(|e| DockerError::RunFailed(format!("Error waiting for container: {e}")))?;

            return Ok(wait_response.status_code);
        }

        // If stream is empty, check container state
        let status = self.container_status(id).await?;
        match status {
            ContainerStatusInfo::Exited { exit_code } => Ok(exit_code),
            _ => Err(DockerError::RunFailed(
                "Container did not exit normally".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_config_builder() {
        let config = ContainerConfig::new("test-container", "python:3.11-slim")
            .with_difficulty("hard")
            .with_cmd(vec![
                "python".to_string(),
                "-c".to_string(),
                "print(1)".to_string(),
            ])
            .with_env(vec!["FOO=bar".to_string()])
            .with_working_dir("/workspace")
            .with_user("1000:1000")
            .with_network_mode("none");

        assert_eq!(config.name, "test-container");
        assert_eq!(config.image, "python:3.11-slim");
        assert_eq!(config.limits.memory_mb, 2048);
        assert_eq!(config.limits.cpu_cores, 2.0);
        assert_eq!(config.cmd.unwrap().len(), 3);
        assert_eq!(config.env.len(), 1);
        assert_eq!(config.working_dir.unwrap(), "/workspace");
        assert_eq!(config.user.unwrap(), "1000:1000");
        assert_eq!(config.network_mode.unwrap(), "none");
    }

    #[test]
    fn test_container_config_with_limits() {
        let limits = ExecutionLimits::new(4096, 4.0, 20, 500, 3600);
        let config = ContainerConfig::new("test", "ubuntu:22.04").with_limits(limits);

        assert_eq!(config.limits.memory_mb, 4096);
        assert_eq!(config.limits.cpu_cores, 4.0);
        assert_eq!(config.limits.disk_gb, 20);
    }

    #[test]
    fn test_exec_result() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "Hello, World!".to_string(),
            stderr: String::new(),
        };

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Hello"));
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_container_status_info() {
        let running = ContainerStatusInfo::Running;
        let exited = ContainerStatusInfo::Exited { exit_code: 0 };
        let unknown = ContainerStatusInfo::Unknown("custom".to_string());

        assert_eq!(running, ContainerStatusInfo::Running);
        assert!(matches!(
            exited,
            ContainerStatusInfo::Exited { exit_code: 0 }
        ));
        assert!(matches!(unknown, ContainerStatusInfo::Unknown(_)));
    }
}
