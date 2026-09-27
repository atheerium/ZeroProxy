/**
 * Shared reader for the OpenAI-compatible SSE stream that
 * `POST /api/dashboard/chat/completions` returns.
 *
 * Why this exists: the dashboard needs to stream model output in more than one
 * place (the Playground), and the parse loop is fiddly enough that copying it
 * invites drift. The wire format is plain OpenAI chunks:
 *
 *   data: {"choices":[{"delta":{"content":"He"},"finish_reason":null}]}
 *   data: {"choices":[],"usage":{"promptTokens":9,"completionTokens":2}}
 *   data: [DONE]
 *
 * No custom event names, so the loop only has to care about `data:` lines.
 *
 * Note the `usage` keys are camelCase. The dashboard API is camelCase
 * throughout (see the `#[serde(rename_all = "camelCase")]` structs across
 * `src/server/`), and the streaming endpoint is no exception — so the
 * OpenAI-wire `prompt_tokens` spelling never appears here. `readUsage`
 * normalises both anyway, because a proxy may pass a genuine OpenAI body
 * through untranslated.
 */

/** Token accounting the server attaches to the final chunk (`include_usage`). */
export interface ChatStreamUsage {
  prompt_tokens?: number;
  completion_tokens?: number;
  total_tokens?: number;
  /** Server-side TTFT, when it measured one. */
  ttft_ms?: number;
  /** True when the server estimated the counts rather than measuring them. */
  estimated?: boolean;
}

/** OpenAI error envelope: `{"error":{"message","type","code"}}`. */
export interface ChatStreamErrorBody {
  error?: { message?: string; type?: string; code?: string };
}

export interface ChatStreamRequest {
  model: string;
  messages: Array<{ role: string; content: unknown }>;
  stream?: boolean;
  temperature?: number;
  max_tokens?: number;
  signal?: AbortSignal;
}

export interface ChatStreamHandlers {
  /** Called with each incremental assistant text fragment. */
  onText: (delta: string) => void;
  /** Called once when the server reports token usage, if it does. */
  onUsage?: (usage: ChatStreamUsage) => void;
  /** Called with `choices[0].finish_reason` when the stream ends normally. */
  onFinishReason?: (reason: string | null) => void;
  /** Called for the first text fragment — the client-side TTFT anchor. */
  onFirstToken?: () => void;
}

/**
 * Pull assistant text out of one stream chunk.
 *
 * Proxies are not perfectly uniform: our own executors emit
 * `choices[0].delta.content`, but some upstream-compatible providers echo a
 * full `message` object, and a few non-OpenAI shapes leak through with
 * `output_text` / `text`. Try each in turn and return the first hit.
 *
 * `delta.reasoning_content` is read as a last resort. A reasoning model can
 * spend its whole completion budget there and emit an empty `content` delta,
 * so ignoring it renders a finished, non-empty response as a blank turn.
 */
export function readAssistantText(chunk: unknown): string {
  if (!chunk || typeof chunk !== "object") return "";
  const c = chunk as {
    choices?: Array<{
      delta?: { content?: unknown; reasoning_content?: unknown; reasoning?: unknown };
      message?: { content?: unknown };
      text?: unknown;
    }>;
    output_text?: unknown;
    text?: unknown;
  };
  const first = c.choices?.[0];
  const candidates: unknown[] = [
    first?.delta?.content,
    first?.message?.content,
    c.output_text,
    c.text,
    first?.text,
    first?.delta?.reasoning_content,
    first?.delta?.reasoning,
  ];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate) return candidate;
  }
  return "";
}

/** First finite number among `values`, or undefined. 0 counts: 0 is a value. */
function firstNumber(...values: unknown[]): number | undefined {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
  }
  return undefined;
}

/**
 * Pull `usage` out of one stream chunk, normalised to snake_case.
 *
 * Returns null when the chunk has no usage, and also when the usage envelope
 * carries no usable count — the server sends a populated object with null
 * counts when it has nothing to report, and reporting that would leave the
 * caller's estimate fallback with nothing to trigger on.
 */
export function readUsage(chunk: unknown): ChatStreamUsage | null {
  if (!chunk || typeof chunk !== "object") return null;
  const raw = (chunk as { usage?: unknown }).usage;
  if (!raw || typeof raw !== "object") return null;
  const source = raw as Record<string, unknown>;

  const prompt_tokens = firstNumber(source.prompt_tokens, source.promptTokens, source.inputTokens);
  const completion_tokens = firstNumber(
    source.completion_tokens,
    source.completionTokens,
    source.outputTokens,
  );
  const total_tokens = firstNumber(source.total_tokens, source.totalTokens);
  const ttft_ms = firstNumber(source.ttft_ms, source.ttftMs);

  if (prompt_tokens === undefined && completion_tokens === undefined && total_tokens === undefined) {
    return null;
  }

  return { prompt_tokens, completion_tokens, total_tokens, ttft_ms, estimated: source.estimated === true };
}

/** Pull `finish_reason` out of one stream chunk, or null when still streaming. */
export function readFinishReason(chunk: unknown): string | null {
  if (!chunk || typeof chunk !== "object") return null;
  const reason = (chunk as { choices?: Array<{ finish_reason?: unknown }> }).choices?.[0]
    ?.finish_reason;
  return typeof reason === "string" ? reason : null;
}

/** Best-effort human message from a non-2xx response body. */
export function readErrorMessage(body: unknown, status: number): string {
  if (body && typeof body === "object") {
    const envelope = body as ChatStreamErrorBody;
    if (typeof envelope.error?.message === "string" && envelope.error.message) {
      return envelope.error.message;
    }
  }
  return `Request failed with HTTP ${status}`;
}

/**
 * POST a chat completion and drive the SSE stream through `handlers`.
 *
 * Resolves when the stream terminates (`data: [DONE]`) or the body ends.
 * Throws on a non-2xx response or an unreadable body. An `AbortSignal` abort
 * rejects with the signal's own `AbortError`, which callers should treat as a
 * user-initiated stop rather than a failure.
 */
export async function streamChatCompletion(
  request: ChatStreamRequest,
  handlers: ChatStreamHandlers,
): Promise<void> {
  const { signal, ...body } = request;

  const response = await fetch("/api/dashboard/chat/completions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "text/event-stream",
    },
    body: JSON.stringify(body),
    signal,
  });

  if (!response.ok) {
    const errorBody = await response.json().catch(() => null);
    throw new Error(readErrorMessage(errorBody, response.status));
  }
  if (!response.body) {
    throw new Error("Upstream returned no response body");
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let sawFirstToken = false;

  const handleLine = (line: string) => {
    const trimmed = line.trim();
    if (!trimmed.startsWith("data:")) return;
    const payload = trimmed.slice(5).trim();
    if (!payload || payload === "[DONE]") return;

    let chunk: unknown;
    try {
      chunk = JSON.parse(payload);
    } catch {
      // A malformed line is a provider bug, not a reason to kill the stream.
      return;
    }

    const usage = readUsage(chunk);
    if (usage) handlers.onUsage?.(usage);

    const reason = readFinishReason(chunk);
    if (reason) handlers.onFinishReason?.(reason);

    const text = readAssistantText(chunk);
    if (text) {
      if (!sawFirstToken) {
        sawFirstToken = true;
        handlers.onFirstToken?.();
      }
      handlers.onText(text);
    }
  };

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split(/\r?\n/);
      // The final element is an incomplete line — keep it for the next read.
      buffer = lines.pop() ?? "";
      for (const line of lines) handleLine(line);
    }
    // Flush any trailing line that never got its newline.
    buffer += decoder.decode();
    if (buffer) handleLine(buffer);
  } finally {
    reader.releaseLock();
  }
}
