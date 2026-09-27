import { describe, it, expect } from "vitest";
import { isFreeModelId, filterFreeModelIds } from "../shared/utils/freeModels";

describe("isFreeModelId", () => {
  it("matches the :free suffix OpenRouter uses", () => {
    expect(isFreeModelId("cohere/north-mini-code:free")).toBe(true);
    expect(isFreeModelId("nvidia/nemotron-3.5-lightning:free")).toBe(true);
  });

  it("matches the -free suffix direct vendors use", () => {
    expect(isFreeModelId("nemotron-3-ultra-free")).toBe(true);
    expect(isFreeModelId("muse-spark-1.2-contributor-free")).toBe(true);
  });

  it("matches a bare /free model name", () => {
    expect(isFreeModelId("openrouter/free")).toBe(true);
  });

  it("is case-insensitive, so capitalised Free is not missed", () => {
    expect(isFreeModelId("meta-llama/Llama-Vision-Free")).toBe(true);
    expect(isFreeModelId("deepseek-ai/DeepSeek-R1-Distill-Llama-70B-Free")).toBe(true);
  });

  it("does not treat infix free as free", () => {
    expect(isFreeModelId("goldeneye-free-auto")).toBe(false);
    expect(isFreeModelId("gpt-5.6-luna-free-thinking")).toBe(false);
  });

  it("is not fooled by free appearing early in the id", () => {
    expect(isFreeModelId("free-solo/turbo-1")).toBe(false);
    expect(isFreeModelId("freestyle/ultra")).toBe(false);
  });

  it("rejects paid models", () => {
    expect(isFreeModelId("openai/gpt-4o")).toBe(false);
    expect(isFreeModelId("anthropic/claude-opus-4")).toBe(false);
  });

  it("handles empty and absent ids without throwing", () => {
    expect(isFreeModelId("")).toBe(false);
    expect(isFreeModelId(null)).toBe(false);
    expect(isFreeModelId(undefined)).toBe(false);
  });
});

describe("filterFreeModelIds", () => {
  it("keeps only free models", () => {
    const models = [
      { id: "cohere/north-mini-code:free" },
      { id: "openai/gpt-4o" },
      { id: "nemotron-3-ultra-free" },
    ];
    expect(filterFreeModelIds(models).map((m) => m.id)).toEqual([
      "cohere/north-mini-code:free",
      "nemotron-3-ultra-free",
    ]);
  });

  it("returns an empty list when nothing is free", () => {
    expect(filterFreeModelIds([{ id: "openai/gpt-4o" }])).toEqual([]);
  });
});
