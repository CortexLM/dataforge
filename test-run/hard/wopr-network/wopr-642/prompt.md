# wopr-network/wopr-642

wopr-network/wopr (#642): refactor: remove billing/metering from wopr-core (WOP-521)

Remove all billing, metering, and cost-related concepts from wopr-core so it is BYOK-only. Eliminate the billing/metering event types and usage events from the core event system. Remove cost fields from provider and model responses. Remove any notion of hosted vs BYOK provider sources and any provider configuration options for overriding base URL routing. Ensure no billing logic remains in core and update the public API accordingly. User-facing behavior should reflect that core has no billing/metering features and provider configuration is limited to BYOK without hosted options.
