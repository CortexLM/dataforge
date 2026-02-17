const loadCore = async () => {
  jest.resetModules();
  return import("../../lib/tracing-core");
};

describe("TracerCoreError", () => {
  it("serializes to a safe JSON shape", async () => {
    const { TracerCoreError } = await loadCore();
    const error = new TracerCoreError("TRACE_FAILED", "Unable to trace", { reason: "timeout" });

    expect(error.code).toBe("TRACE_FAILED");
    expect(error.message).toBe("Unable to trace");
    expect(error.toJSON()).toEqual({
      code: "TRACE_FAILED",
      message: "Unable to trace",
      details: { reason: "timeout" }
    });
  });
});
