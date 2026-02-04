//! Container lifecycle management for Docker execution.
//!
//! This module provides a high-level abstraction for managing container
//! lifecycle including creation, execution, and cleanup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DockerError;
use crate::execution::docker_client::{ContainerConfig, ContainerStatusInfo, DockerClient};

// Re-export ExecResult from docker_client for convenience
pub use crate::execution::docker_client::ExecResult;

/// Status of a managed container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    /// Container is pending creation.
    Pending,
    /// Container is being created.
    Creating,
    /// Container is running.
    Running,
    /// Container completed successfully.
    Completed,
    /// Container failed with an error message.
    Failed(String),
    /// Container exceeded its timeout.
    Timeout,
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerStatus::Pending => write!(f, "pending"),
            ContainerStatus::Creating => write!(f, "creating"),
            ContainerStatus::Running => write!(f, "running"),
            ContainerStatus::Completed => write!(f, "completed"),
            ContainerStatus::Failed(msg) => write!(f, "failed: {}", msg),
            ContainerStatus::Timeout => write!(f, "timeout"),
        }
    }
}

/// A managed Docker container with lifecycle tracking.
#[derive(Debug)]
pub struct Container {
    /// Docker container ID.
    id: String,
    /// Current status of the container.
    status: ContainerStatus,
    /// Configuration used to create the container.
    config: ContainerConfig,
    /// Timestamp when the container was created.
    created_at: DateTime<Utc>,
}

impl Container {
    /// Creates a new container with the given configuration.
    ///
    /// This creates the container in Docker but does not start it.
    /// The container starts in `Creating` status and transitions to
    /// `Pending` when successfully created.
    ///
    /// # Arguments
    ///
    /// * `client` - Docker client for API operations
    /// * `config` - Container configuration
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if container creation fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use dataforge::execution::{DockerClient, Container, ContainerConfig};
    ///
    /// let client = DockerClient::new()?;
    /// let config = ContainerConfig::new("task-123", "python:3.11-slim");
    /// let container = Container::new(&client, config).await?;
    /// ```
    pub async fn new(client: &DockerClient, config: ContainerConfig) -> Result<Self, DockerError> {
        let created_at = Utc::now();

        // Ensure image exists
        if !client.image_exists(&config.image).await {
            client.pull_image(&config.image).await?;
        }

        let id = client.create_container(config.clone()).await?;

        Ok(Self {
            id,
            status: ContainerStatus::Pending,
            config,
            created_at,
        })
    }

    /// Creates a container struct from an existing Docker container ID.
    ///
    /// This is useful for reconnecting to containers that were created
    /// in a previous session.
    ///
    /// # Arguments
    ///
    /// * `client` - Docker client for API operations
    /// * `id` - Existing container ID
    /// * `config` - Configuration (for reference, not applied)
    pub async fn from_existing(
        client: &DockerClient,
        id: impl Into<String>,
        config: ContainerConfig,
    ) -> Result<Self, DockerError> {
        let id = id.into();

        // Verify container exists and get its status
        let status_info = client.container_status(&id).await?;
        let status = match status_info {
            ContainerStatusInfo::Created => ContainerStatus::Pending,
            ContainerStatusInfo::Running => ContainerStatus::Running,
            ContainerStatusInfo::Exited { exit_code } => {
                if exit_code == 0 {
                    ContainerStatus::Completed
                } else {
                    ContainerStatus::Failed(format!("Exited with code {}", exit_code))
                }
            }
            ContainerStatusInfo::Dead => ContainerStatus::Failed("Container is dead".to_string()),
            _ => ContainerStatus::Pending,
        };

        Ok(Self {
            id,
            status,
            config,
            created_at: Utc::now(), // We don't know the original creation time
        })
    }

    /// Starts the container.
    ///
    /// Transitions the container from `Pending` to `Running` status.
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if:
    /// - Container is not in `Pending` status
    /// - Docker API call fails
    pub async fn start(&mut self, client: &DockerClient) -> Result<(), DockerError> {
        if self.status != ContainerStatus::Pending {
            return Err(DockerError::RunFailed(format!(
                "Cannot start container in {} state",
                self.status
            )));
        }

        self.status = ContainerStatus::Creating;

        match client.start_container(&self.id).await {
            Ok(()) => {
                self.status = ContainerStatus::Running;
                Ok(())
            }
            Err(e) => {
                self.status = ContainerStatus::Failed(format!("Start failed: {}", e));
                Err(e)
            }
        }
    }

    /// Executes a command inside the running container.
    ///
    /// # Arguments
    ///
    /// * `client` - Docker client for API operations
    /// * `cmd` - Command to execute as array of strings
    ///
    /// # Returns
    ///
    /// `ExecResult` containing exit code, stdout, and stderr.
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if:
    /// - Container is not running
    /// - Command execution fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = container.exec(&client, &["python", "-c", "print('hello')"]).await?;
    /// println!("Output: {}", result.stdout);
    /// ```
    pub async fn exec(
        &self,
        client: &DockerClient,
        cmd: &[&str],
    ) -> Result<ExecResult, DockerError> {
        if self.status != ContainerStatus::Running {
            return Err(DockerError::RunFailed(format!(
                "Cannot exec in container with {} state",
                self.status
            )));
        }

        client.exec_command(&self.id, cmd).await
    }

    /// Stops the container if running.
    ///
    /// This sends SIGTERM to the container and waits for graceful shutdown.
    pub async fn stop(&mut self, client: &DockerClient) -> Result<(), DockerError> {
        if self.status != ContainerStatus::Running {
            return Ok(()); // Already stopped
        }

        client.stop_container(&self.id).await?;

        // Check final status
        let status_info = client.container_status(&self.id).await?;
        self.status = match status_info {
            ContainerStatusInfo::Exited { exit_code } => {
                if exit_code == 0 {
                    ContainerStatus::Completed
                } else {
                    ContainerStatus::Failed(format!("Exited with code {}", exit_code))
                }
            }
            _ => ContainerStatus::Completed,
        };

        Ok(())
    }

    /// Cleans up the container by stopping and removing it.
    ///
    /// This is the final step in the container lifecycle and should
    /// be called when the container is no longer needed.
    ///
    /// # Arguments
    ///
    /// * `client` - Docker client for API operations
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if cleanup fails, though it attempts
    /// to force removal if graceful cleanup fails.
    pub async fn cleanup(&mut self, client: &DockerClient) -> Result<(), DockerError> {
        // Try graceful stop first if running
        if self.status == ContainerStatus::Running {
            if let Err(e) = client.stop_container(&self.id).await {
                // Log error but continue with force removal
                tracing::warn!("Failed to stop container gracefully: {}", e);
            }
        }

        // Force remove the container
        client.remove_container(&self.id, true).await?;

        // Update status based on previous state
        match &self.status {
            ContainerStatus::Running | ContainerStatus::Pending | ContainerStatus::Creating => {
                self.status = ContainerStatus::Completed;
            }
            // Keep Failed/Timeout/Completed status
            _ => {}
        }

        Ok(())
    }

    /// Marks the container as timed out and cleans it up.
    pub async fn mark_timeout(&mut self, client: &DockerClient) -> Result<(), DockerError> {
        self.status = ContainerStatus::Timeout;
        self.cleanup(client).await
    }

    /// Marks the container as failed with a message.
    pub fn mark_failed(&mut self, message: impl Into<String>) {
        self.status = ContainerStatus::Failed(message.into());
    }

    /// Gets the logs from the container.
    pub async fn logs(&self, client: &DockerClient) -> Result<String, DockerError> {
        client.get_logs(&self.id).await
    }

    /// Waits for the container to finish executing.
    ///
    /// # Returns
    ///
    /// The exit code of the container.
    pub async fn wait(&mut self, client: &DockerClient) -> Result<i64, DockerError> {
        let exit_code = client.wait_container(&self.id).await?;

        self.status = if exit_code == 0 {
            ContainerStatus::Completed
        } else {
            ContainerStatus::Failed(format!("Exited with code {}", exit_code))
        };

        Ok(exit_code)
    }

    /// Returns the container ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the current status.
    pub fn status(&self) -> &ContainerStatus {
        &self.status
    }

    /// Returns the container configuration.
    pub fn config(&self) -> &ContainerConfig {
        &self.config
    }

    /// Returns when the container was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the configured timeout in seconds.
    pub fn timeout_seconds(&self) -> u64 {
        self.config.limits.timeout_seconds
    }

    /// Checks if the container is in a terminal state (completed, failed, timeout).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            ContainerStatus::Completed | ContainerStatus::Failed(_) | ContainerStatus::Timeout
        )
    }

    /// Checks if the container is running.
    pub fn is_running(&self) -> bool {
        self.status == ContainerStatus::Running
    }

    /// Syncs the local status with the actual Docker container status.
    pub async fn sync_status(&mut self, client: &DockerClient) -> Result<(), DockerError> {
        let status_info = client.container_status(&self.id).await?;

        self.status = match status_info {
            ContainerStatusInfo::Created => ContainerStatus::Pending,
            ContainerStatusInfo::Running => ContainerStatus::Running,
            ContainerStatusInfo::Paused => ContainerStatus::Running, // Treat paused as running
            ContainerStatusInfo::Restarting => ContainerStatus::Running,
            ContainerStatusInfo::Exited { exit_code } => {
                if exit_code == 0 {
                    ContainerStatus::Completed
                } else {
                    ContainerStatus::Failed(format!("Exited with code {}", exit_code))
                }
            }
            ContainerStatusInfo::Removing => ContainerStatus::Completed,
            ContainerStatusInfo::Dead => ContainerStatus::Failed("Container is dead".to_string()),
            ContainerStatusInfo::Unknown(s) => {
                ContainerStatus::Failed(format!("Unknown status: {}", s))
            }
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::ExecutionLimits;

    #[test]
    fn test_container_status_display() {
        assert_eq!(format!("{}", ContainerStatus::Pending), "pending");
        assert_eq!(format!("{}", ContainerStatus::Creating), "creating");
        assert_eq!(format!("{}", ContainerStatus::Running), "running");
        assert_eq!(format!("{}", ContainerStatus::Completed), "completed");
        assert_eq!(format!("{}", ContainerStatus::Timeout), "timeout");
        assert_eq!(
            format!("{}", ContainerStatus::Failed("error".to_string())),
            "failed: error"
        );
    }

    #[test]
    fn test_container_status_equality() {
        assert_eq!(ContainerStatus::Running, ContainerStatus::Running);
        assert_ne!(ContainerStatus::Running, ContainerStatus::Completed);
        assert_eq!(
            ContainerStatus::Failed("a".to_string()),
            ContainerStatus::Failed("a".to_string())
        );
        assert_ne!(
            ContainerStatus::Failed("a".to_string()),
            ContainerStatus::Failed("b".to_string())
        );
    }

    #[test]
    fn test_is_terminal() {
        // Terminal states
        assert!(ContainerStatus::Completed.to_string().contains("completed"));
        assert!(ContainerStatus::Timeout.to_string().contains("timeout"));

        // Non-terminal states
        assert!(!ContainerStatus::Pending.to_string().contains("terminal"));
        assert!(!ContainerStatus::Running.to_string().contains("terminal"));
    }

    #[test]
    fn test_container_config_limits() {
        let limits = ExecutionLimits::new(2048, 2.0, 10, 200, 1800);
        let config = ContainerConfig::new("test", "python:3.11").with_limits(limits);

        assert_eq!(config.limits.memory_mb, 2048);
        assert_eq!(config.limits.timeout_seconds, 1800);
    }
}
