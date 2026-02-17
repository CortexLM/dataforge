const loadCore = async (minSpeckArea?: string) => {
  jest.resetModules();
  if (minSpeckArea !== undefined) {
    process.env.MIN_SPECK_AREA = minSpeckArea;
  } else {
    delete process.env.MIN_SPECK_AREA;
  }

  return import("../../lib/tracing-core");
};

describe("tracing core settings schema", () => {
  it("applies defaults and respects env-based min speck area", async () => {
    const { TracerSettingsSchema } = await loadCore("9");
    const settings = TracerSettingsSchema.parse({});

    expect(settings.mode).toBe("auto");
    expect(settings.output).toBe("fill");
    expect(settings.strokeWidth).toBeUndefined();
    expect(settings.minSpeckArea).toBe(9);
  });

  it("infers stroke width for stroke output or outline mode", async () => {
    const { TracerSettingsSchema } = await loadCore();

    const strokeSettings = TracerSettingsSchema.parse({ output: "stroke" });
    expect(strokeSettings.output).toBe("stroke");
    expect(strokeSettings.strokeWidth).toBe(1);

    const outlineSettings = TracerSettingsSchema.parse({ output: "fill", outlineMode: true });
    expect(outlineSettings.outlineMode).toBe(true);
    expect(outlineSettings.strokeWidth).toBe(1);
  });
});
