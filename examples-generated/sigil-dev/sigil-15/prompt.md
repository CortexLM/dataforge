# sigil-dev/sigil-15

sigil-dev/sigil (#15): feat: complete Phase 5 remaining tasks (chat, doctor, auth)

## Summary

- **Chat CLI command**: Connects to gateway SSE endpoint, streams text_delta events to stdout, handles errors on stderr. Supports --workspace, --model, --session flags.
- **Doctor CLI command**: Runs 6 diagnostic checks (binary, platform, gateway status, config, plugins, disk space) with actionable output.
- **Auth/ABAC middleware**: Bearer token authentication with TokenValidator interface, user context injection, public path bypass (/health, /openapi.*), 401/403 responses. Config-based token validation wired in gateway. Auth disabled when no tokens configured (dev mode).
- **SSE client infrastructure**: postSSE method and parseSSEStream parser in client.go for CLI-to-gateway streaming.

Closes sigil-8po, sigil-9s6, sigil-31w. Closes epic sigil-3m0 (Phase 5: Server & API).

## Test plan

- [x] `task test` — 21/21 packages pass, 0 failures
- [x] `task lint` — 0 issues (Go, Markdown, YAML)
- [x] Phase 5 gate checklist verified (12/12 items)
- [x] 7 new chat tests (SSE streaming, connection failure, error events, flags)
- [x] 8 new doctor tests (all checks, gateway up/down, plugins, disk space)
- [x] 9 new auth tests (public bypass, 401 cases, 403, valid token, context injection, dev mode)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
