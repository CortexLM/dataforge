//! Docker Compose configuration generation for dataforge tasks.
//!
//! This module provides utilities for generating docker-compose.yaml files
//! for multi-container task environments.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::dockerfile::DockerfileConfig;

/// Build configuration for a compose service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeBuild {
    /// Build context directory.
    pub context: String,
    /// Path to Dockerfile.
    pub dockerfile: String,
}

/// Health check configuration for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Test command to run.
    pub test: Vec<String>,
    /// Interval between health checks.
    pub interval: String,
    /// Timeout for each health check.
    pub timeout: String,
    /// Number of retries before marking unhealthy.
    pub retries: u32,
}

/// Resource limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitsConfig {
    /// CPU limit (e.g., "2.0").
    pub cpus: String,
    /// Memory limit (e.g., "1G").
    pub memory: String,
}

/// Resources configuration wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesConfig {
    /// Resource limits.
    pub limits: ResourceLimitsConfig,
}

/// Deploy configuration for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    /// Resources configuration.
    pub resources: ResourcesConfig,
}

/// A service definition in docker-compose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeService {
    /// Service name.
    pub name: String,
    /// Docker image to use (if not building).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Build configuration (if building from Dockerfile).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<ComposeBuild>,
    /// Container name.
    pub container_name: String,
    /// Container hostname.
    pub hostname: String,
    /// Environment variables.
    pub environment: HashMap<String, String>,
    /// Volume mounts.
    pub volumes: Vec<String>,
    /// Networks to connect to.
    pub networks: Vec<String>,
    /// Service dependencies.
    pub depends_on: Vec<String>,
    /// Health check configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthCheck>,
    /// Deploy configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<DeployConfig>,
}

impl Default for ComposeService {
    fn default() -> Self {
        Self {
            name: String::new(),
            image: None,
            build: None,
            container_name: String::new(),
            hostname: String::new(),
            environment: HashMap::new(),
            volumes: Vec::new(),
            networks: vec!["task-internal".to_string()],
            depends_on: Vec::new(),
            healthcheck: None,
            deploy: None,
        }
    }
}

/// Network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network driver (e.g., "bridge").
    pub driver: String,
    /// Whether the network is internal (no external access).
    pub internal: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            driver: "bridge".to_string(),
            internal: true,
        }
    }
}

/// Volume configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeConfig {}

/// Complete docker-compose configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    /// Compose file version.
    pub version: String,
    /// Services in the compose file.
    pub services: HashMap<String, ComposeService>,
    /// Networks defined in the compose file.
    pub networks: HashMap<String, NetworkConfig>,
    /// Volumes defined in the compose file.
    pub volumes: HashMap<String, VolumeConfig>,
}

impl Default for ComposeConfig {
    fn default() -> Self {
        let mut networks = HashMap::new();
        networks.insert("task-internal".to_string(), NetworkConfig::default());

        Self {
            version: "3.8".to_string(),
            services: HashMap::new(),
            networks,
            volumes: HashMap::new(),
        }
    }
}

/// Builder for generating docker-compose.yaml content.
#[derive(Debug, Clone)]
pub struct ComposeBuilder {
    config: ComposeConfig,
}

impl ComposeBuilder {
    /// Create a new ComposeBuilder with default configuration.
    pub fn new() -> Self {
        Self {
            config: ComposeConfig::default(),
        }
    }

    /// Add the main workspace service based on Dockerfile configuration.
    pub fn add_workspace(&mut self, docker_config: &DockerfileConfig) -> &mut Self {
        let (cpus, memory) = get_resource_limits_for_difficulty(&docker_config.difficulty);

        let mut environment = docker_config.env_vars.clone();
        environment.insert("TASK_ID".to_string(), docker_config.task_id.clone());
        environment.insert(
            "TASK_DIFFICULTY".to_string(),
            docker_config.difficulty.clone(),
        );
        environment.insert("TASK_CATEGORY".to_string(), docker_config.category.clone());

        let service = ComposeService {
            name: "workspace".to_string(),
            image: None,
            build: Some(ComposeBuild {
                context: ".".to_string(),
                dockerfile: "Dockerfile".to_string(),
            }),
            container_name: format!("task-{}-workspace", docker_config.task_id),
            hostname: "workspace".to_string(),
            environment,
            volumes: vec![
                "workspace-home:/home/user".to_string(),
                "task-results:/home/user/results".to_string(),
                "./task-deps:/task-deps:ro".to_string(),
            ],
            networks: vec!["task-internal".to_string()],
            depends_on: Vec::new(),
            healthcheck: Some(HealthCheck {
                test: vec!["CMD".to_string(), "true".to_string()],
                interval: "10s".to_string(),
                timeout: "5s".to_string(),
                retries: 3,
            }),
            deploy: Some(DeployConfig {
                resources: ResourcesConfig {
                    limits: ResourceLimitsConfig { cpus, memory },
                },
            }),
        };

        self.config
            .services
            .insert("workspace".to_string(), service);
        self.config
            .volumes
            .insert("workspace-home".to_string(), VolumeConfig {});
        self.config
            .volumes
            .insert("task-results".to_string(), VolumeConfig {});

        self
    }

    /// Add a database service.
    ///
    /// # Arguments
    /// * `db_type` - Database type: "postgres", "mysql", "mongodb", "redis"
    pub fn add_database(&mut self, db_type: &str) -> &mut Self {
        let (image, port, healthcheck_cmd, env_vars) = match db_type.to_lowercase().as_str() {
            "postgres" | "postgresql" => (
                "postgres:16-alpine",
                "5432",
                vec![
                    "CMD-SHELL".to_string(),
                    "pg_isready -U taskuser -d taskdb".to_string(),
                ],
                vec![
                    (
                        "POSTGRES_USER".to_string(),
                        "${POSTGRES_USER:-taskuser}".to_string(),
                    ),
                    (
                        "POSTGRES_PASSWORD".to_string(),
                        "${POSTGRES_PASSWORD:-changeme}".to_string(),
                    ),
                    (
                        "POSTGRES_DB".to_string(),
                        "${POSTGRES_DB:-taskdb}".to_string(),
                    ),
                ],
            ),
            "mysql" | "mariadb" => (
                "mysql:8-oracle",
                "3306",
                vec![
                    "CMD".to_string(),
                    "mysqladmin".to_string(),
                    "ping".to_string(),
                    "-h".to_string(),
                    "localhost".to_string(),
                ],
                vec![
                    (
                        "MYSQL_ROOT_PASSWORD".to_string(),
                        "${MYSQL_ROOT_PASSWORD:-changeme}".to_string(),
                    ),
                    (
                        "MYSQL_USER".to_string(),
                        "${MYSQL_USER:-taskuser}".to_string(),
                    ),
                    (
                        "MYSQL_PASSWORD".to_string(),
                        "${MYSQL_PASSWORD:-changeme}".to_string(),
                    ),
                    (
                        "MYSQL_DATABASE".to_string(),
                        "${MYSQL_DATABASE:-taskdb}".to_string(),
                    ),
                ],
            ),
            "mongodb" | "mongo" => (
                "mongo:7",
                "27017",
                vec![
                    "CMD".to_string(),
                    "mongosh".to_string(),
                    "--eval".to_string(),
                    "db.adminCommand('ping')".to_string(),
                ],
                vec![
                    (
                        "MONGO_INITDB_ROOT_USERNAME".to_string(),
                        "${MONGO_INITDB_ROOT_USERNAME:-taskuser}".to_string(),
                    ),
                    (
                        "MONGO_INITDB_ROOT_PASSWORD".to_string(),
                        "${MONGO_INITDB_ROOT_PASSWORD:-changeme}".to_string(),
                    ),
                ],
            ),
            _ => {
                // Default to postgres
                (
                    "postgres:16-alpine",
                    "5432",
                    vec![
                        "CMD-SHELL".to_string(),
                        "pg_isready -U taskuser -d taskdb".to_string(),
                    ],
                    vec![
                        (
                            "POSTGRES_USER".to_string(),
                            "${POSTGRES_USER:-taskuser}".to_string(),
                        ),
                        (
                            "POSTGRES_PASSWORD".to_string(),
                            "${POSTGRES_PASSWORD:-changeme}".to_string(),
                        ),
                        (
                            "POSTGRES_DB".to_string(),
                            "${POSTGRES_DB:-taskdb}".to_string(),
                        ),
                    ],
                )
            }
        };

        let service = ComposeService {
            name: "database".to_string(),
            image: Some(image.to_string()),
            build: None,
            container_name: "task-db".to_string(),
            hostname: "database".to_string(),
            environment: env_vars.into_iter().collect(),
            volumes: vec![format!("db-data:/var/lib/{}", get_db_data_path(db_type))],
            networks: vec!["task-internal".to_string()],
            depends_on: Vec::new(),
            healthcheck: Some(HealthCheck {
                test: healthcheck_cmd,
                interval: "5s".to_string(),
                timeout: "5s".to_string(),
                retries: 10,
            }),
            deploy: Some(DeployConfig {
                resources: ResourcesConfig {
                    limits: ResourceLimitsConfig {
                        cpus: "0.5".to_string(),
                        memory: "256M".to_string(),
                    },
                },
            }),
        };

        self.config.services.insert("database".to_string(), service);
        self.config
            .volumes
            .insert("db-data".to_string(), VolumeConfig {});

        // Update workspace to depend on database
        if let Some(workspace) = self.config.services.get_mut("workspace") {
            workspace.depends_on.push("database".to_string());
            workspace
                .environment
                .insert("DATABASE_HOST".to_string(), "database".to_string());
            workspace
                .environment
                .insert("DATABASE_PORT".to_string(), port.to_string());
        }

        self
    }

    /// Add a Redis cache service.
    pub fn add_cache(&mut self) -> &mut Self {
        let service = ComposeService {
            name: "cache".to_string(),
            image: Some("redis:7-alpine".to_string()),
            build: None,
            container_name: "task-cache".to_string(),
            hostname: "cache".to_string(),
            environment: HashMap::new(),
            volumes: Vec::new(),
            networks: vec!["task-internal".to_string()],
            depends_on: Vec::new(),
            healthcheck: Some(HealthCheck {
                test: vec![
                    "CMD".to_string(),
                    "redis-cli".to_string(),
                    "ping".to_string(),
                ],
                interval: "5s".to_string(),
                timeout: "3s".to_string(),
                retries: 5,
            }),
            deploy: Some(DeployConfig {
                resources: ResourcesConfig {
                    limits: ResourceLimitsConfig {
                        cpus: "0.25".to_string(),
                        memory: "128M".to_string(),
                    },
                },
            }),
        };

        self.config.services.insert("cache".to_string(), service);

        // Update workspace to use cache
        if let Some(workspace) = self.config.services.get_mut("workspace") {
            workspace
                .environment
                .insert("REDIS_HOST".to_string(), "cache".to_string());
            workspace
                .environment
                .insert("REDIS_PORT".to_string(), "6379".to_string());
        }

        self
    }

    /// Add an Nginx web server service.
    pub fn add_webserver(&mut self) -> &mut Self {
        let service = ComposeService {
            name: "webserver".to_string(),
            image: Some("nginx:alpine".to_string()),
            build: None,
            container_name: "task-web".to_string(),
            hostname: "webserver".to_string(),
            environment: HashMap::new(),
            volumes: vec![
                "./task-deps/nginx.conf:/etc/nginx/nginx.conf:ro".to_string(),
                "./task-deps/html:/usr/share/nginx/html:ro".to_string(),
            ],
            networks: vec!["task-internal".to_string()],
            depends_on: vec!["workspace".to_string()],
            healthcheck: Some(HealthCheck {
                test: vec![
                    "CMD".to_string(),
                    "wget".to_string(),
                    "--no-verbose".to_string(),
                    "--tries=1".to_string(),
                    "--spider".to_string(),
                    "http://localhost/".to_string(),
                ],
                interval: "10s".to_string(),
                timeout: "5s".to_string(),
                retries: 3,
            }),
            deploy: Some(DeployConfig {
                resources: ResourcesConfig {
                    limits: ResourceLimitsConfig {
                        cpus: "0.25".to_string(),
                        memory: "64M".to_string(),
                    },
                },
            }),
        };

        self.config
            .services
            .insert("webserver".to_string(), service);

        // Update workspace to know about webserver
        if let Some(workspace) = self.config.services.get_mut("workspace") {
            workspace
                .environment
                .insert("WEBSERVER_HOST".to_string(), "webserver".to_string());
            workspace
                .environment
                .insert("WEBSERVER_PORT".to_string(), "80".to_string());
        }

        self
    }

    /// Build and return the docker-compose.yaml content as a YAML string.
    pub fn build(&self) -> String {
        let mut output = String::new();

        // Version
        output.push_str(&format!("version: \"{}\"\n\n", self.config.version));

        // Services
        output.push_str("services:\n");
        for (name, service) in &self.config.services {
            output.push_str(&format!("  {}:\n", name));

            if let Some(ref image) = service.image {
                output.push_str(&format!("    image: {}\n", image));
            }

            if let Some(ref build) = service.build {
                output.push_str("    build:\n");
                output.push_str(&format!("      context: {}\n", build.context));
                output.push_str(&format!("      dockerfile: {}\n", build.dockerfile));
            }

            output.push_str(&format!("    container_name: {}\n", service.container_name));
            output.push_str(&format!("    hostname: {}\n", service.hostname));

            if !service.environment.is_empty() {
                output.push_str("    environment:\n");
                for (key, value) in &service.environment {
                    output.push_str(&format!("      {}: \"{}\"\n", key, value));
                }
            }

            if !service.volumes.is_empty() {
                output.push_str("    volumes:\n");
                for vol in &service.volumes {
                    output.push_str(&format!("      - {}\n", vol));
                }
            }

            if !service.networks.is_empty() {
                output.push_str("    networks:\n");
                for net in &service.networks {
                    output.push_str(&format!("      - {}\n", net));
                }
            }

            if !service.depends_on.is_empty() {
                output.push_str("    depends_on:\n");
                for dep in &service.depends_on {
                    output.push_str(&format!("      - {}\n", dep));
                }
            }

            if let Some(ref healthcheck) = service.healthcheck {
                output.push_str("    healthcheck:\n");
                output.push_str("      test:\n");
                for item in &healthcheck.test {
                    output.push_str(&format!("        - \"{}\"\n", item));
                }
                output.push_str(&format!("      interval: {}\n", healthcheck.interval));
                output.push_str(&format!("      timeout: {}\n", healthcheck.timeout));
                output.push_str(&format!("      retries: {}\n", healthcheck.retries));
            }

            if let Some(ref deploy) = service.deploy {
                output.push_str("    deploy:\n");
                output.push_str("      resources:\n");
                output.push_str("        limits:\n");
                output.push_str(&format!(
                    "          cpus: \"{}\"\n",
                    deploy.resources.limits.cpus
                ));
                output.push_str(&format!(
                    "          memory: {}\n",
                    deploy.resources.limits.memory
                ));
            }

            output.push('\n');
        }

        // Networks
        output.push_str("networks:\n");
        for (name, config) in &self.config.networks {
            output.push_str(&format!("  {}:\n", name));
            output.push_str(&format!("    driver: {}\n", config.driver));
            output.push_str(&format!("    internal: {}\n", config.internal));
        }
        output.push('\n');

        // Volumes
        if !self.config.volumes.is_empty() {
            output.push_str("volumes:\n");
            for name in self.config.volumes.keys() {
                output.push_str(&format!("  {}:\n", name));
            }
        }

        output
    }
}

impl Default for ComposeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Get resource limits based on difficulty level.
fn get_resource_limits_for_difficulty(difficulty: &str) -> (String, String) {
    match difficulty.to_lowercase().as_str() {
        "easy" => ("1.0".to_string(), "512M".to_string()),
        "medium" => ("2.0".to_string(), "1G".to_string()),
        "hard" => ("4.0".to_string(), "2G".to_string()),
        _ => ("2.0".to_string(), "1G".to_string()),
    }
}

/// Get the data path for different database types.
fn get_db_data_path(db_type: &str) -> &'static str {
    match db_type.to_lowercase().as_str() {
        "postgres" | "postgresql" => "postgresql/data",
        "mysql" | "mariadb" => "mysql",
        "mongodb" | "mongo" => "mongodb",
        _ => "postgresql/data",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_builder_basic() {
        let config = DockerfileConfig {
            base_image: "dataforge/ubuntu-24.04:latest".to_string(),
            task_id: "test-001".to_string(),
            category: "file-operations".to_string(),
            difficulty: "easy".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let mut builder = ComposeBuilder::new();
        builder.add_workspace(&config);
        let yaml = builder.build();

        assert!(yaml.contains("version: \"3.8\""));
        assert!(yaml.contains("workspace:"));
        assert!(yaml.contains("container_name: task-test-001-workspace"));
        assert!(yaml.contains("task-internal:"));
    }

    #[test]
    fn test_compose_builder_with_database() {
        let config = DockerfileConfig {
            base_image: "dataforge/ubuntu-24.04:latest".to_string(),
            task_id: "test-002".to_string(),
            category: "data-science".to_string(),
            difficulty: "medium".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let mut builder = ComposeBuilder::new();
        builder.add_workspace(&config);
        builder.add_database("postgres");
        let yaml = builder.build();

        assert!(yaml.contains("database:"));
        assert!(yaml.contains("postgres:16-alpine"));
        assert!(yaml.contains("POSTGRES_USER"));
        assert!(yaml.contains("DATABASE_HOST"));
    }

    #[test]
    fn test_compose_builder_with_cache() {
        let config = DockerfileConfig {
            base_image: "dataforge/ubuntu-24.04:latest".to_string(),
            task_id: "test-003".to_string(),
            category: "web".to_string(),
            difficulty: "medium".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let mut builder = ComposeBuilder::new();
        builder.add_workspace(&config);
        builder.add_cache();
        let yaml = builder.build();

        assert!(yaml.contains("cache:"));
        assert!(yaml.contains("redis:7-alpine"));
        assert!(yaml.contains("REDIS_HOST"));
    }

    #[test]
    fn test_compose_builder_with_webserver() {
        let config = DockerfileConfig {
            base_image: "dataforge/ubuntu-24.04:latest".to_string(),
            task_id: "test-004".to_string(),
            category: "web".to_string(),
            difficulty: "hard".to_string(),
            packages: Vec::new(),
            copy_paths: Vec::new(),
            env_vars: HashMap::new(),
            user: "user".to_string(),
            workdir: "/home/user".to_string(),
        };

        let mut builder = ComposeBuilder::new();
        builder.add_workspace(&config);
        builder.add_webserver();
        let yaml = builder.build();

        assert!(yaml.contains("webserver:"));
        assert!(yaml.contains("nginx:alpine"));
        assert!(yaml.contains("WEBSERVER_HOST"));
    }

    #[test]
    fn test_resource_limits_by_difficulty() {
        assert_eq!(
            get_resource_limits_for_difficulty("easy"),
            ("1.0".to_string(), "512M".to_string())
        );
        assert_eq!(
            get_resource_limits_for_difficulty("medium"),
            ("2.0".to_string(), "1G".to_string())
        );
        assert_eq!(
            get_resource_limits_for_difficulty("hard"),
            ("4.0".to_string(), "2G".to_string())
        );
    }
}
