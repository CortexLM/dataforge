//! Docker execution layer for swe_forge tasks.
//!
//! This module provides the Docker execution layer using the bollard crate
//! for container lifecycle management, resource control, and task execution.
//!
//! # Architecture
//!
//! Container states follow this lifecycle:
//! ```text
//! PENDING → CREATING → RUNNING → COMPLETED/FAILED/TIMEOUT → CLEANUP
//! ```
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::execution::{DockerClient, Container, ContainerConfig};
//!
//! let client = DockerClient::new()?;
//!
//! let config = ContainerConfig::new("task-123", "python:3.11-slim")
//!     .with_difficulty("medium");
//!
//! let mut container = Container::new(&client, config).await?;
//! container.start(&client).await?;
//! let result = container.exec(&client, &["python", "-c", "print('hello')"]).await?;
//! container.cleanup(&client).await?;
//! ```

pub mod container;
pub mod docker_client;
pub mod resources;

pub use container::{Container, ContainerStatus, ExecResult};
pub use docker_client::DockerClient;
pub use resources::{get_execution_limits, ExecutionLimits};
