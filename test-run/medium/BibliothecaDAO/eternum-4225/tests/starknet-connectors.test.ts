import { describe, expect, it } from "vitest";

import { buildStarknetConnectors, getInjectedConnectorsOptions } from "./starknet-connectors";

describe("starknet-connectors", () => {
  it("keeps recommended wallet providers available when injected wallets exist", () => {
    const options = getInjectedConnectorsOptions();

    expect(options.includeRecommended).toBe("always");
    expect(options.order).toBe("random");
    expect(options.recommended.map((connector) => connector.id)).toEqual(
      expect.arrayContaining(["argentX", "braavos"]),
    );
  });

  it("returns only the cartridge connector when cartridge-only mode is enabled", () => {
    const cartridgeConnector = { id: "controller" } as any;
    const injected = [{ id: "alpha" } as any, { id: "beta" } as any];

    const connectors = buildStarknetConnectors(cartridgeConnector, injected, true);

    expect(connectors).toEqual([cartridgeConnector]);
    expect(connectors).not.toContain(injected[0]);
  });

  it("includes non-cartridge connectors when cartridge-only mode is disabled", () => {
    const cartridgeConnector = { id: "controller" } as any;
    const injected = [{ id: "alpha" } as any, { id: "beta" } as any];

    const connectors = buildStarknetConnectors(cartridgeConnector, injected, false);

    expect(connectors).toEqual([cartridgeConnector, ...injected]);
    expect(connectors).toContain(injected[1]);
  });
});
