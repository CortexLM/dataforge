/**
 * Tests for removal of billing/metering and hosted provider config from core.
 */
import { describe, expect, it, vi } from "vitest";

// Mock logger to keep output quiet
vi.mock("../../src/logger.js", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

describe("core billing removal", () => {
  it("should not expose the billing module in core", async () => {
    let importError: unknown;
    try {
      await import("../../src/core/billing.js");
    } catch (err) {
      importError = err;
    }

    expect(importError).toBeInstanceOf(Error);
    expect((importError as Error).message).toMatch(/billing(\.js)?/i);
  });
});

describe("provider config without hosted/baseUrl support", () => {
  it("resolveProvider should ignore baseUrl in provider config", async () => {
    const mod = await import("../../src/core/providers.js");
    const { ProviderRegistry } = mod;

    const createClientSpy = vi.fn().mockResolvedValue({
      query: vi.fn(),
      listModels: vi.fn().mockResolvedValue([]),
      healthCheck: vi.fn().mockResolvedValue(true),
    });

    const registry = new ProviderRegistry();
    registry.register({
      id: "test-provider",
      name: "Test Provider",
      description: "Mock provider for testing",
      defaultModel: "test-model",
      supportedModels: ["test-model"],
      validateCredentials: vi.fn().mockResolvedValue(true),
      createClient: createClientSpy,
      getCredentialType: () => "api-key",
    });

    await registry.setCredential("test-provider", "test-key-123");

    await registry.resolveProvider({
      name: "test-provider",
      baseUrl: "https://gateway.example.com/v1",
      options: { temperature: 0.9 },
    });

    expect(createClientSpy).toHaveBeenCalledWith(
      "test-key-123",
      { temperature: 0.9 },
    );
  });
});
