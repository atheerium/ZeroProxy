/**
 * `chatStream` parses the SSE body of `POST /api/dashboard/chat/completions`.
 *
 * The fixtures below are CAPTURED payloads, not invented ones. They exist
 * because two real bugs shipped against hand-written expectations:
 *
 *  - The server emits `usage` in camelCase (`promptTokens`), while the client
 *    read OpenAI's snake_case (`prompt_tokens`). Every token count silently
 *    fell back to the chars/4 estimate, so a chars/4 approximation was being
 *    presented as if it came from the server.
 *  - Reasoning models stream their output in `delta.reasoning_content`, which
 *    the client ignored, so a completed response rendered as an empty turn.
 *
 * A 59-token completion with an empty `delta` is the second bug's fingerprint.
 */
import { describe, expect, it } from "vitest";
import { readAssistantText, readUsage } from "@/shared/utils/chatStream";

describe("readUsage", () => {
  it("reads the server's camelCase usage block", () => {
    // Captured live from the streaming endpoint.
    const chunk = {
      choices: [{ delta: {}, finish_reason: "stop" }],
      usage: {
        promptTokens: 89,
        inputTokens: null,
        completionTokens: 59,
        outputTokens: null,
        totalTokens: 148,
        estimated: true,
        ttftMs: null,
        errorClass: null,
        latencyMs: null,
      },
    };

    expect(readUsage(chunk)).toEqual({
      prompt_tokens: 89,
      completion_tokens: 59,
      total_tokens: 148,
      ttft_ms: undefined,
      estimated: true,
    });
  });

  it("still reads OpenAI's snake_case usage block", () => {
    const chunk = { usage: { prompt_tokens: 9, completion_tokens: 2, total_tokens: 11 } };

    expect(readUsage(chunk)).toEqual({
      prompt_tokens: 9,
      completion_tokens: 2,
      total_tokens: 11,
      ttft_ms: undefined,
      estimated: false,
    });
  });

  it("reports a server-measured ttft when one is present", () => {
    const chunk = { usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2, ttftMs: 412 } };

    expect(readUsage(chunk)?.ttft_ms).toBe(412);
  });

  it("treats a zero count as a real number rather than a missing one", () => {
    // `??` must be used, not `||`: 0 tokens is a real value, and a falsy check
    // would silently swap it for the chars/4 estimate.
    const chunk = { usage: { promptTokens: 0, completionTokens: 0, totalTokens: 0 } };

    expect(readUsage(chunk)).toEqual({
      prompt_tokens: 0,
      completion_tokens: 0,
      total_tokens: 0,
      ttft_ms: undefined,
      estimated: false,
    });
  });

  it("returns null when the chunk carries no usage at all", () => {
    expect(readUsage({ choices: [{ delta: { content: "hi" } }] })).toBeNull();
  });

  it("returns null when usage is present but carries no recognisable count", () => {
    // The server sends a populated envelope with null counts when it has
    // nothing to report; reporting that as usage would defeat the caller's
    // estimate fallback.
    expect(readUsage({ usage: { promptTokens: null, completionTokens: null, totalTokens: null } })).toBeNull();
  });
});

describe("readAssistantText", () => {
  it("reads a normal content delta", () => {
    expect(readAssistantText({ choices: [{ delta: { content: "pong" } }] })).toBe("pong");
  });

  it("reads a reasoning delta, which is a reasoning model's only output", () => {
    const chunk = { choices: [{ delta: { reasoning_content: "Let me work through it." } }] };

    expect(readAssistantText(chunk)).toBe("Let me work through it.");
  });

  it("prefers content over reasoning when both are present", () => {
    const chunk = {
      choices: [{ delta: { reasoning_content: "thinking…", content: "answer" } }],
    };

    expect(readAssistantText(chunk)).toBe("answer");
  });

  it("returns empty string for an empty delta with only a finish_reason", () => {
    // The captured 59-token empty completion. This must not throw or invent text.
    expect(readAssistantText({ choices: [{ delta: {}, finish_reason: "stop" }] })).toBe("");
  });
});
