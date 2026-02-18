//! Resource management for Docker containers in swe_forge.
//!
//! This module provides resource limits, volume configuration, and container
//! configuration utilities based on task difficulty levels.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Network isolation mode for containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// No network access at all.
    None,
    /// Internal network only (containers can communicate).
    #[default]
    Internal,
    /// Bridge network with potential external access.
    Bridge,
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkMode::None => write!(f, "none"),
            NetworkMode::Internal => write!(f, "internal"),
            NetworkMode::Bridge => write!(f, "bridge"),
        }
    }
}

/// Resource limits for a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Number of CPU cores (e.g., 1.0, 2.0, 4.0).
    pub cpu_count: f64,
    /// Memory limit in bytes.
    pub memory_bytes: u64,
    /// Storage limit in bytes.
    pub storage_bytes: u64,
    /// Maximum number of processes.
    pub pids_limit: u32,
    /// Network isolation mode.
    pub network_mode: NetworkMode,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_count: 0.0,
            memory_bytes: 32 * 1024 * 1024 * 1024, // 32 GB
            storage_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            pids_limit: 200,
            network_mode: NetworkMode::Internal,
        }
    }
}

impl ResourceLimits {
    /// Get memory limit as a human-readable string.
    pub fn memory_string(&self) -> String {
        let mb = self.memory_bytes / (1024 * 1024);
        if mb >= 1024 {
            format!("{}G", mb / 1024)
        } else {
            format!("{}M", mb)
        }
    }

    /// Get storage limit as a human-readable string.
    pub fn storage_string(&self) -> String {
        let gb = self.storage_bytes / (1024 * 1024 * 1024);
        format!("{}G", gb)
    }
}

/// Predefined resource limits for easy difficulty tasks.
pub const EASY_LIMITS: ResourceLimits = ResourceLimits {
    cpu_count: 0.0,
    memory_bytes: 32 * 1024 * 1024 * 1024, // 32 GB
    storage_bytes: 1024 * 1024 * 1024,     // 1 GB
    pids_limit: 100,
    network_mode: NetworkMode::None,
};

/// Predefined resource limits for medium difficulty tasks.
pub const MEDIUM_LIMITS: ResourceLimits = ResourceLimits {
    cpu_count: 0.0,
    memory_bytes: 32 * 1024 * 1024 * 1024, // 32 GB
    storage_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
    pids_limit: 200,
    network_mode: NetworkMode::Internal,
};

/// Predefined resource limits for hard difficulty tasks.
pub const HARD_LIMITS: ResourceLimits = ResourceLimits {
    cpu_count: 0.0,
    memory_bytes: 32 * 1024 * 1024 * 1024, // 32 GB
    storage_bytes: 5 * 1024 * 1024 * 1024, // 5 GB
    pids_limit: 500,
    network_mode: NetworkMode::Internal,
};

/// Get resource limits based on difficulty level.
///
/// # Arguments
/// * `difficulty` - Difficulty level: "easy", "medium", or "hard"
///
/// # Returns
/// Resource limits appropriate for the difficulty level.
pub fn apply_resource_limits(difficulty: &str) -> ResourceLimits {
    match difficulty.to_lowercase().as_str() {
        "easy" => EASY_LIMITS,
        "medium" => MEDIUM_LIMITS,
        "hard" => HARD_LIMITS,
        _ => MEDIUM_LIMITS, // Default to medium if unknown
    }
}

/// Get network mode based on difficulty level.
pub fn network_mode_from_difficulty(difficulty: &str) -> NetworkMode {
    match difficulty.to_lowercase().as_str() {
        "easy" => NetworkMode::None,
        "medium" | "hard" => NetworkMode::Internal,
        _ => NetworkMode::Internal,
    }
}

/// A volume mount configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Path on the host system.
    pub host_path: String,
    /// Path inside the container.
    pub container_path: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

impl VolumeMount {
    /// Create a new volume mount.
    pub fn new(host_path: impl Into<String>, container_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only: false,
        }
    }

    /// Create a read-only volume mount.
    pub fn read_only(host_path: impl Into<String>, container_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only: true,
        }
    }

    /// Convert to Docker volume string format.
    pub fn to_docker_string(&self) -> String {
        if self.read_only {
            format!("{}:{}:ro", self.host_path, self.container_path)
        } else {
            format!("{}:{}", self.host_path, self.container_path)
        }
    }
}

/// Container configuration combining all settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container name.
    pub name: String,
    /// Docker image to use.
    pub image: String,
    /// Resource limits.
    pub limits: ResourceLimits,
    /// Environment variables.
    pub env_vars: HashMap<String, String>,
    /// Volume mounts.
    pub volumes: Vec<VolumeMount>,
    /// Network mode.
    pub network_mode: NetworkMode,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            image: String::new(),
            limits: ResourceLimits::default(),
            env_vars: HashMap::new(),
            volumes: Vec::new(),
            network_mode: NetworkMode::Internal,
        }
    }
}

/// Create a set of secure volume mounts for a task.
///
/// # Arguments
/// * `task_id` - Unique identifier for the task
///
/// # Returns
/// A vector of volume mounts with appropriate security settings.
pub fn create_secure_volumes(task_id: &str) -> Vec<VolumeMount> {
    vec![
        // Task dependencies - read-only to prevent modification
        VolumeMount::read_only("./task-deps", "/task-deps"),
        // User workspace - read-write for task execution
        VolumeMount::new(
            format!("/var/lib/swe_forge/tasks/{}/workspace", task_id),
            "/home/user",
        ),
        // Results directory - read-write for output collection
        VolumeMount::new(
            format!("/var/lib/swe_forge/tasks/{}/results", task_id),
            "/home/user/results",
        ),
        // Logs directory - read-write for debugging
        VolumeMount::new(
            format!("/var/lib/swe_forge/tasks/{}/logs", task_id),
            "/var/log/task",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_easy() {
        let limits = apply_resource_limits("easy");
        assert_eq!(limits.cpu_count, 0.0);
        assert_eq!(limits.memory_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(limits.network_mode, NetworkMode::None);
    }

    #[test]
    fn test_resource_limits_medium() {
        let limits = apply_resource_limits("medium");
        assert_eq!(limits.cpu_count, 0.0);
        assert_eq!(limits.memory_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(limits.network_mode, NetworkMode::Internal);
    }

    #[test]
    fn test_resource_limits_hard() {
        let limits = apply_resource_limits("hard");
        assert_eq!(limits.cpu_count, 0.0);
        assert_eq!(limits.memory_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(limits.network_mode, NetworkMode::Internal);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = apply_resource_limits("unknown");
        assert_eq!(limits.cpu_count, 0.0); // Should default to medium
    }

    #[test]
    fn test_memory_string() {
        let limits = ResourceLimits {
            memory_bytes: 512 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(limits.memory_string(), "512M");

        let limits_gb = ResourceLimits {
            memory_bytes: 32 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(limits_gb.memory_string(), "32G");
    }

    #[test]
    fn test_volume_mount() {
        let vol = VolumeMount::new("/host/path", "/container/path");
        assert!(!vol.read_only);
        assert_eq!(vol.to_docker_string(), "/host/path:/container/path");

        let vol_ro = VolumeMount::read_only("/host/ro", "/container/ro");
        assert!(vol_ro.read_only);
        assert_eq!(vol_ro.to_docker_string(), "/host/ro:/container/ro:ro");
    }

    #[test]
    fn test_create_secure_volumes() {
        let volumes = create_secure_volumes("test-task-001");
        assert_eq!(volumes.len(), 4);

        // Check task-deps is read-only
        let task_deps = volumes.iter().find(|v| v.container_path == "/task-deps");
        assert!(task_deps.is_some());
        assert!(task_deps.unwrap().read_only);

        // Check results is writable
        let results = volumes
            .iter()
            .find(|v| v.container_path == "/home/user/results");
        assert!(results.is_some());
        assert!(!results.unwrap().read_only);
    }

    #[test]
    fn test_network_mode_display() {
        assert_eq!(format!("{}", NetworkMode::None), "none");
        assert_eq!(format!("{}", NetworkMode::Internal), "internal");
        assert_eq!(format!("{}", NetworkMode::Bridge), "bridge");
    }

    #[test]
    fn test_network_mode_from_difficulty() {
        assert_eq!(network_mode_from_difficulty("easy"), NetworkMode::None);
        assert_eq!(
            network_mode_from_difficulty("medium"),
            NetworkMode::Internal
        );
        assert_eq!(network_mode_from_difficulty("hard"), NetworkMode::Internal);
    }
}
