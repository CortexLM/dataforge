# EpicenterHQ/epicenter-1348 (original PR)

EpicenterHQ/epicenter (#1348): refactor: extract @epicenter/server into standalone package

Extracts the server code from `@epicenter/hq` into a standalone `@epicenter/server` package at `packages/server/`. The server was coupled to the core library — anyone who wanted to self-host a sync server had to install the entire workspace system. Now it's independently installable.

The CLI's `serve` command uses a dynamic import to avoid a circular dependency. `AnyWorkspaceClient` was added to `@epicenter/hq/static` exports since it was needed by the server but wasn't previously exported. All 50 server tests and 560 epicenter tests pass with zero regressions.

Also includes three docs articles and draft encryption specs for future work. Phase 2 (room manager extraction + three-mode auth) is deferred to a follow-up.
