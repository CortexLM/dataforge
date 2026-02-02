//! Docker environment generation for synth-bench tasks.
//!
//! This module provides utilities for generating Dockerfiles, docker-compose.yaml files,
//! and managing container resources for benchmark task execution.

pub mod compose;
pub mod dockerfile;
pub mod resources;

pub use compose::{
    ComposeBuild, ComposeBuilder, ComposeConfig, ComposeService, DeployConfig, HealthCheck,
    NetworkConfig, ResourceLimitsConfig, ResourcesConfig, VolumeConfig,
};
pub use dockerfile::{
    select_base_image, DockerfileBuilder, DockerfileConfig, BASE_MULTI_LANG, BASE_NODE,
    BASE_PYTHON, BASE_RUST, BASE_UBUNTU,
};
pub use resources::{
    apply_resource_limits, create_secure_volumes, ContainerConfig, NetworkMode, ResourceLimits,
    VolumeMount,
};

/// Represents a complete Docker environment for a task.
#[derive(Debug, Clone)]
pub struct DockerEnvironment {
    /// The Dockerfile content
    pub dockerfile: String,
    /// The docker-compose.yaml content
    pub compose: String,
    /// Container configuration with resource limits
    pub container_config: ContainerConfig,
}

impl DockerEnvironment {
    /// Create a new Docker environment from a Dockerfile configuration.
    pub fn new(config: &DockerfileConfig) -> Self {
        let dockerfile = DockerfileBuilder::new(config.clone()).build();

        let mut compose_builder = ComposeBuilder::new();
        compose_builder.add_workspace(config);
        let compose = compose_builder.build();

        let limits = apply_resource_limits(&config.difficulty);
        let container_config = ContainerConfig {
            name: format!("task-{}", config.task_id),
            image: config.base_image.clone(),
            limits,
            env_vars: config.env_vars.clone(),
            volumes: create_secure_volumes(&config.task_id),
            network_mode: resources::network_mode_from_difficulty(&config.difficulty),
        };

        Self {
            dockerfile,
            compose,
            container_config,
        }
    }

    /// Create a Docker environment with additional services (database, cache, webserver).
    pub fn with_services(
        config: &DockerfileConfig,
        database: Option<&str>,
        include_cache: bool,
        include_webserver: bool,
    ) -> Self {
        let dockerfile = DockerfileBuilder::new(config.clone()).build();

        let mut compose_builder = ComposeBuilder::new();
        compose_builder.add_workspace(config);

        if let Some(db_type) = database {
            compose_builder.add_database(db_type);
        }

        if include_cache {
            compose_builder.add_cache();
        }

        if include_webserver {
            compose_builder.add_webserver();
        }

        let compose = compose_builder.build();

        let limits = apply_resource_limits(&config.difficulty);
        let container_config = ContainerConfig {
            name: format!("task-{}", config.task_id),
            image: config.base_image.clone(),
            limits,
            env_vars: config.env_vars.clone(),
            volumes: create_secure_volumes(&config.task_id),
            network_mode: resources::network_mode_from_difficulty(&config.difficulty),
        };

        Self {
            dockerfile,
            compose,
            container_config,
        }
    }
}
