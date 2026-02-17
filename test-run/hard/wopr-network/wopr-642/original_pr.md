# wopr-network/wopr-642 (original PR)

wopr-network/wopr (#642): refactor: remove billing/metering from wopr-core (WOP-521)

## Summary

Removes all billing/metering/cost concepts from wopr-core as part of WOP-521 cleanup. Core should be BYOK-only - billing belongs in the platform layer.

## Changes

- **Remove MeterEvent** - deleted interface from `src/types.ts`, `src/core/events.ts`, `src/plugin-types/events.ts`
- **Remove meter:usage event** - removed from all WOPREventMaps
- **Remove cost fields** - `ProviderResponse.cost`, `ModelResponse.cost` deleted
- **Remove ProviderSource** - deleted `"byok" | "hosted"` type
- **Remove baseUrl override** - `ProviderConfig.baseUrl` deleted (gateway routing is platform)
- **Delete billing.ts** - entire `src/core/billing.ts` removed
- **Delete related tests** - removed `billing.test.ts` and `provider-base-url.test.ts`

## Subtasks (Linear)

- [x] WOP-516: Remove MeterEvent and meter:usage from core event system
- [x] WOP-517: Remove cost field and hosted provider concepts from ProviderResponse
- [ ] WOP-518: Remove metering emission from sessions.ts (not found - may already be clean)
- [ ] WOP-519: Delete agent/coder-500 branch (has billing in core)
- [x] WOP-520: Refactor provider abstraction to BYOK-only

## Testing

- Build: ✅
- Tests: ✅ 1280 passing
- Lint: ✅

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->
## Summary by CodeRabbit

* **Removed Features**
  * Removed billing and usage metering system and related event type.
  * Removed source/baseUrl options and cost fields from provider APIs.

* **Refactor**
  * Simplified event model and provider configuration surface.

* **Tests**
  * Removed unit tests for billing and provider baseUrl behavior.

* **Chores**
  * CI: added Trivy ignore file; updated package overrides.
<!-- end of auto-generated comment: release notes by coderabbit.ai -->
