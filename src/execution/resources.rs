//! Resource limits for Docker container execution.
//!
//! This module provides execution resource limits based on difficulty levels,
//! mirroring the existing difficulty system in the codebase.

use serde::{Deserialize, Serialize};

/// Execution resource limits for a container.
///
/// These limits control the resources available to a container
/// during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLimits {
    /// Memory limit in megabytes.
    pub memory_mb: u64,
    /// CPU cores available (e.g., 0.5, 1.0, 2.0).
    pub cpu_cores: f64,
    /// Disk space limit in gigabytes.
    pub disk_gb: u64,
    /// Maximum number of processes allowed.
    pub max_processes: u64,
    /// Timeout in seconds before the container is killed.
    pub timeout_seconds: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        // Default to medium difficulty limits
        Self {
            memory_mb: 1024,
            cpu_cores: 1.0,
            disk_gb: 5,
            max_processes: 100,
            timeout_seconds: 1200, // 20 minutes
        }
    }
}

impl ExecutionLimits {
    /// Creates new execution limits with the given parameters.
    pub fn new(
        memory_mb: u64,
        cpu_cores: f64,
        disk_gb: u64,
        max_processes: u64,
        timeout_seconds: u64,
    ) -> Self {
        Self {
            memory_mb,
            cpu_cores,
            disk_gb,
            max_processes,
            timeout_seconds,
        }
    }

    /// Returns memory limit in bytes.
    pub fn memory_bytes(&self) -> i64 {
        (self.memory_mb * 1024 * 1024) as i64
    }

    /// Returns CPU period in nanoseconds (fixed at 100ms).
    pub fn cpu_period(&self) -> i64 {
        100_000 // 100ms in microseconds
    }

    /// Returns CPU quota based on cores allocated.
    ///
    /// Formula: quota = period * cores
    /// e.g., 1.0 core = 100000 quota (100% of one CPU)
    pub fn cpu_quota(&self) -> i64 {
        (self.cpu_period() as f64 * self.cpu_cores) as i64
    }

    /// Returns disk space limit in bytes.
    pub fn disk_bytes(&self) -> u64 {
        self.disk_gb * 1024 * 1024 * 1024
    }
}

/// Get execution limits based on difficulty level string.
///
/// Supported difficulty levels:
/// - "easy" - Light resources for simple tasks
/// - "medium" - Moderate resources for standard tasks  
/// - "hard" - Heavy resources for complex tasks
/// - "expert" - Very high resources for expert-level tasks
/// - "nightmare" - Maximum resources for extreme tasks
///
/// Unknown difficulty levels default to "medium".
///
/// # Arguments
///
/// * `difficulty` - The difficulty level as a string (case-insensitive)
///
/// # Returns
///
/// Execution limits appropriate for the difficulty level.
///
/// # Example
///
/// ```
/// use dataforge::execution::get_execution_limits;
///
/// let limits = get_execution_limits("hard");
/// assert_eq!(limits.memory_mb, 2048);
/// assert_eq!(limits.timeout_seconds, 1800);
/// ```
pub fn get_execution_limits(difficulty: &str) -> ExecutionLimits {
    match difficulty.to_lowercase().as_str() {
        "easy" => ExecutionLimits {
            memory_mb: 512,
            cpu_cores: 0.5,
            disk_gb: 2,
            max_processes: 50,
            timeout_seconds: 600, // 10 minutes
        },
        "medium" => ExecutionLimits {
            memory_mb: 1024,
            cpu_cores: 1.0,
            disk_gb: 5,
            max_processes: 100,
            timeout_seconds: 1200, // 20 minutes
        },
        "hard" => ExecutionLimits {
            memory_mb: 2048,
            cpu_cores: 2.0,
            disk_gb: 10,
            max_processes: 200,
            timeout_seconds: 2400, // 40 minutes
        },
        "expert" => ExecutionLimits {
            memory_mb: 4096,
            cpu_cores: 4.0,
            disk_gb: 20,
            max_processes: 500,
            timeout_seconds: 4800, // 80 minutes
        },
        "nightmare" => ExecutionLimits {
            memory_mb: 8192,
            cpu_cores: 8.0,
            disk_gb: 50,
            max_processes: 1000,
            timeout_seconds: 9000, // 150 minutes
        },
        // Unknown difficulty defaults to medium
        _ => ExecutionLimits::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easy_limits() {
        let limits = get_execution_limits("easy");
        assert_eq!(limits.memory_mb, 512);
        assert_eq!(limits.cpu_cores, 0.5);
        assert_eq!(limits.disk_gb, 2);
        assert_eq!(limits.max_processes, 50);
        assert_eq!(limits.timeout_seconds, 600);
    }

    #[test]
    fn test_medium_limits() {
        let limits = get_execution_limits("medium");
        assert_eq!(limits.memory_mb, 1024);
        assert_eq!(limits.cpu_cores, 1.0);
        assert_eq!(limits.disk_gb, 5);
        assert_eq!(limits.max_processes, 100);
        assert_eq!(limits.timeout_seconds, 1200);
    }

    #[test]
    fn test_hard_limits() {
        let limits = get_execution_limits("hard");
        assert_eq!(limits.memory_mb, 2048);
        assert_eq!(limits.cpu_cores, 2.0);
        assert_eq!(limits.disk_gb, 10);
        assert_eq!(limits.max_processes, 200);
        assert_eq!(limits.timeout_seconds, 2400);
    }

    #[test]
    fn test_expert_limits() {
        let limits = get_execution_limits("expert");
        assert_eq!(limits.memory_mb, 4096);
        assert_eq!(limits.cpu_cores, 4.0);
        assert_eq!(limits.disk_gb, 20);
        assert_eq!(limits.max_processes, 500);
        assert_eq!(limits.timeout_seconds, 4800);
    }

    #[test]
    fn test_nightmare_limits() {
        let limits = get_execution_limits("nightmare");
        assert_eq!(limits.memory_mb, 8192);
        assert_eq!(limits.cpu_cores, 8.0);
        assert_eq!(limits.disk_gb, 50);
        assert_eq!(limits.max_processes, 1000);
        assert_eq!(limits.timeout_seconds, 9000);
    }

    #[test]
    fn test_case_insensitive() {
        let limits_lower = get_execution_limits("hard");
        let limits_upper = get_execution_limits("HARD");
        let limits_mixed = get_execution_limits("Hard");

        assert_eq!(limits_lower.memory_mb, limits_upper.memory_mb);
        assert_eq!(limits_lower.memory_mb, limits_mixed.memory_mb);
    }

    #[test]
    fn test_unknown_defaults_to_medium() {
        let limits = get_execution_limits("unknown");
        let medium = get_execution_limits("medium");

        assert_eq!(limits.memory_mb, medium.memory_mb);
        assert_eq!(limits.cpu_cores, medium.cpu_cores);
        assert_eq!(limits.timeout_seconds, medium.timeout_seconds);
    }

    #[test]
    fn test_memory_bytes_conversion() {
        let limits = ExecutionLimits::new(512, 1.0, 5, 100, 300);
        assert_eq!(limits.memory_bytes(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_cpu_quota_calculation() {
        let limits = ExecutionLimits::new(512, 2.0, 5, 100, 300);
        assert_eq!(limits.cpu_period(), 100_000);
        assert_eq!(limits.cpu_quota(), 200_000); // 2 cores = 2 * period
    }

    #[test]
    fn test_disk_bytes_conversion() {
        let limits = ExecutionLimits::new(512, 1.0, 10, 100, 300);
        assert_eq!(limits.disk_bytes(), 10 * 1024 * 1024 * 1024);
    }
}
