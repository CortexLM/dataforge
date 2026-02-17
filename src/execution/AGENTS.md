# AGENTS.md — src/execution/

## Purpose

Docker execution layer using the `bollard` crate. Manages container lifecycle (create → start → exec → cleanup), resource limits, and task execution isolation.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports, lifecycle documentation |
| `container.rs` | `Container` struct with state machine (`Pending → Creating → Running → Completed/Failed/Timeout`) |
| `docker_client.rs` | `DockerClient` wrapper around `bollard::Docker`; also defines `ContainerConfig`, `ContainerStatusInfo`, `ExecResult` |
| `resources.rs` | `ExecutionLimits` — difficulty-based resource limits |

## Key Types

- `Container` — Stateful container with `start()`, `exec()`, `stop()`, `cleanup()`, `wait()` methods
- `ContainerStatus` — State enum: `Pending`, `Creating`, `Running`, `Completed`, `Failed(String)`, `Timeout`
- `ContainerConfig` — Builder for container creation (name, image, cmd, env, limits, volumes, network)
- `ContainerStatusInfo` — Raw status info from Docker daemon
- `ExecResult` — exit_code, stdout, stderr from container exec
- `DockerClient` — Thin wrapper for Docker API operations (create, start, stop, remove, exec, logs, pull, wait)
- `ExecutionLimits` — Memory, CPU, disk, max processes, timeout per difficulty (5 tiers: easy/medium/hard/expert/nightmare)

## Rules

- Container states follow: `Pending → Creating → Running → Completed/Failed(String)/Timeout`
- Always call `cleanup()` after use — containers must not leak
- Use `get_execution_limits()` to get difficulty-appropriate limits
- All container operations are async (bollard is tokio-based)
