# AGENTS.md — src/llm/

## Purpose

LLM integration layer providing an OpenAI-compatible API client with function calling, multi-model routing, prompt caching, and cost tracking. All LLM interactions in swe-forge go through this module.

## Module Structure

| File | Responsibility |
|------|---------------|
| `mod.rs` | Re-exports, module docs, usage examples |
| `litellm.rs` | Core API client (`LiteLlmClient`), request/response types, `LlmProvider` trait |
| `providers/openrouter.rs` | OpenRouter provider implementation |
| `router.rs` | `MultiModelRouter` with strategies: `CostOptimized`, `RoundRobin`, `CapabilityBased` |
| `cache.rs` | `PromptCache` for multi-conversation prompt caching (content hashing) |
| `cost.rs` | `CostTracker` with daily/monthly budgets, usage recording |

## Key Types

- `LlmProvider` (trait) — `async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse>`
- `LiteLlmClient` — Direct OpenAI-compatible HTTP client
- `OpenRouterProvider` — OpenRouter-specific provider
- `GenerationRequest` — Messages + model + tools + tool_choice + temperature
- `GenerationResponse` — Choices with `ToolCallInfo` for function calling
- `ToolDefinition` — JSON Schema function definition for `tools` array
- `ToolChoice` — `Auto`, `None`, `Required`, `Named(String)`
- `Message` — `system`, `user`, `assistant`, `tool` roles
- `MultiModelRouter` — Routes requests across providers by strategy
- `PromptCache` / `SharedPromptCache` — Thread-safe prompt caching (`Arc<RwLock<>>`)
- `CostTracker` — Atomic cost tracking with budget enforcement

## Rules

- Always use `tools` + `tool_choice: "required"` for structured output — never parse free-form text
- Provider trait objects must be `Send + Sync` (used as `Arc<dyn LlmProvider>`)
- Default model: `openai/gpt-5.2-codex:nitro` (set in `src/cli/commands.rs`)
- Cost tracking is optional but should be used when available
- Cache keys are content hashes (`sha2`) — not message indices
