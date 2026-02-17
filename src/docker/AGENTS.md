# AGENTS.md — src/docker/

## Purpose

Docker environment generation — produces Dockerfiles, docker-compose.yaml, and container configurations for benchmark task execution. Separate from `src/execution/` which handles runtime container management.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | `DockerEnvironment` struct, re-exports |
| `dockerfile.rs` | `DockerfileBuilder` — generates Dockerfiles with base image selection (`python`, `node`, `rust`, `ubuntu`) |
| `compose.rs` | `ComposeBuilder` — generates docker-compose.yaml with optional database/cache/webserver services |
| `resources.rs` | `ResourceLimits`, `ContainerConfig`, `VolumeMount`, `NetworkMode` — security and resource config |

## Key Types

- `DockerEnvironment` — Complete Docker setup (Dockerfile + compose + container config)
- `DockerfileBuilder` / `DockerfileConfig` — Dockerfile generation with `multi_lang` base image support
- `ComposeBuilder` / `ComposeConfig` / `ComposeService` — docker-compose generation
- `ResourceLimits` — CPU, memory, storage, PIDs, network mode per difficulty (3 tiers: easy/medium/hard)
- `VolumeMount` — Host/container path mapping with read-only option
- `ContainerConfig` — Name, image, limits, env vars, volumes, network mode
- `NetworkMode` — `None`, `Internal`, `Bridge` (difficulty-dependent)
- Base images: `BASE_PYTHON`, `BASE_NODE`, `BASE_RUST`, `BASE_UBUNTU`, `BASE_MULTI_LANG`

## Rules

- Always use `apply_resource_limits(&difficulty)` when creating containers
- Network mode is difficulty-dependent (`network_mode_from_difficulty()`)
- Volumes must use `create_secure_volumes()` for isolation
- Base image selection via `select_base_image()` based on language
