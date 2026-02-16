# AGENTS.md — src/execution/

## Purpose

Docker execution layer using the `bollard` crate. Manages container lifecycle (create → start → exec → cleanup), resource limits, and task execution isolation.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports, lifecycle documentation |
| `container.rs` | `Container` struct with state machine (`PENDING → CREATING → RUNNING → COMPLETED/FAILED/TIMEOUT → CLEANUP`) |
| `docker_client.rs` | `DockerClient` wrapper around `bollard::Docker` |
| `resources.rs` | `ExecutionLimits` — difficulty-based resource limits |

## Key Types

- `Container` — Stateful container with `start()`, `exec()`, `cleanup()` methods
- `ContainerStatus` — State enum tracking container lifecycle
- `ExecResult` — stdout, stderr, exit code from container exec
- `DockerClient` — Thin wrapper for Docker API operations
- `ExecutionLimits` — Memory, CPU, timeout, network limits per difficulty

## Rules

- Container states follow: `PENDING → CREATING → RUNNING → COMPLETED/FAILED/TIMEOUT → CLEANUP`
- Always call `cleanup()` after use — containers must not leak
- Use `get_execution_limits()` to get difficulty-appropriate limits
- All container operations are async (bollard is tokio-based)
