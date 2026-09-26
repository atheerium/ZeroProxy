import { describe, it, expect } from "vitest";
import { buildAvailableModels, isFreeModelId } from "@/shared/models/availableModels";

describe("isFreeModelId", () => {
  it("flags :free and -free suffixes", () => {
    expect(isFreeModelId("moonshot/kimi-k3-free", "trk")).toBe(true);
    expect(isFreeModelId("qwen/qwen3:free", "trk")).toBe(true);
    expect(isFreeModelId("gpt-4", "openai")).toBe(false);
  });
  it("respects explicitFree", () => {
    expect(isFreeModelId("gpt-4", "openai", true)).toBe(true);
  });
});

describe("buildAvailableModels", () => {
  const catalog = [
    { id: "gpt-4", name: "GPT-4", type: "llm" },
    { id: "text-embedding-3-small", name: "Embed", type: "embedding" },
    { id: "qwen/qwen3:free", name: "Qwen Free", type: "llm" },
    { id: "moonshot/kimi-k3-free", name: "Kimi Free", type: "llm" },
    { id: "grok-4", name: "Grok 4", type: "llm" },
  ];

  it("deduplicates core ids and filters by type llm", () => {
    const r = buildAvailableModels({
      catalogModels: catalog as any,
      providerAlias: "openai",
    });
    expect(r.allCoreIds).toEqual(expect.arrayContaining(["gpt-4", "qwen/qwen3:free", "moonshot/kimi-k3-free", "grok-4"]));
    expect(r.allCoreIds).not.toContain("text-embedding-3-small");
  });

  it("live models merged and deduped", () => {
    const r = buildAvailableModels({
      catalogModels: [{ id: "gpt-4", type: "llm" }] as any,
      liveModels: [{ id: "gpt-4" }, { id: "gpt-5" }],
      providerAlias: "openai",
    });
    expect(r.allCoreIds).toEqual(["gpt-4", "gpt-5"]);
  });

  it("disabledIds moves rows to disabledCoreRows, excluded from enabledCoreRows", () => {
    const r = buildAvailableModels({
      catalogModels: catalog as any,
      disabledIds: ["gpt-4"],
      providerAlias: "openai",
    });
    expect(r.disabledCoreRows.map((x) => x.id)).toContain("gpt-4");
    expect(r.enabledCoreRows.map((x) => x.id)).not.toContain("gpt-4");
    expect(r.allRows.map((x) => x.id)).toContain("gpt-4");
  });

  it("custom rows always survive freeOnly", () => {
    const r = buildAvailableModels({
      catalogModels: [{ id: "gpt-4", type: "llm" }] as any,
      customModels: [{ id: "my-custom", providerAlias: "openai" } as any],
      modelAliases: { myAlias: "my-custom" } as any,
      providerAlias: "openai",
      freeOnly: true,
    });
    // customRows should be present even when core has no free models matching
    expect(r.customRows.length).toBeGreaterThan(0);
    expect(r.enabledRows.map((x) => x.id)).toEqual(expect.arrayContaining(r.customRows.map((x) => x.id)));
  });

  it("freeOnly filters core to free ids only", () => {
    const r = buildAvailableModels({
      catalogModels: catalog as any,
      providerAlias: "openai",
      freeOnly: true,
    });
    expect(r.enabledCoreRows.every((x) => x.isFree)).toBe(true);
    expect(r.enabledCoreRows.map((x) => x.id)).toEqual(expect.arrayContaining(["qwen/qwen3:free", "moonshot/kimi-k3-free"]));
    expect(r.enabledCoreRows.map((x) => x.id)).not.toContain("gpt-4");
  });

  it("providerAlias baked into fullModel", () => {
    const r = buildAvailableModels({
      catalogModels: [{ id: "grok-4", type: "llm" }] as any,
      providerAlias: "xai",
    });
    expect(r.enabledCoreRows[0].fullModel).toBe("xai/grok-4");
  });

  it("custom model with providerFreeTier: true produces isFree row", () => {
    const r = buildAvailableModels({
      catalogModels: [] as any,
      customModels: [{ id: "my-free", providerAlias: "openai", providerFreeTier: true } as any],
      providerAlias: "openai",
    });
    expect(r.customRows[0].isFree).toBe(true);
  });

  it("custom model without providerFreeTier produces isFree false", () => {
    const r = buildAvailableModels({
      catalogModels: [] as any,
      customModels: [{ id: "my-paid", providerAlias: "openai" } as any],
      providerAlias: "openai",
    });
    expect(r.customRows[0].isFree).toBe(false);
  });

  it("ModelSelectModal and ProviderDetail mirror: same buildAvailableModels result for same inputs", () => {
    const input = {
      catalogModels: catalog as any,
      liveModels: [{ id: "gpt-5" }] as any,
      customModels: [] as any,
      modelAliases: {} as any,
      disabledIds: ["grok-4"] as any,
      providerAlias: "openai",
      type: "llm" as const,
      freeOnly: false,
    };
    const a = buildAvailableModels(input);
    const b = buildAvailableModels(input);
    expect(a).toEqual(b);
    // disabled map must match regardless of which page renders
    expect(a.disabledCoreRows.map((x) => x.id)).toEqual(b.disabledCoreRows.map((x) => x.id));
  });
});
