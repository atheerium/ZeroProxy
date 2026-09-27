"use client";

/**
 * Playground — a native chat surface for poking at models directly.
 *
 * Why it exists: testing a provider change used to mean rewiring an external
 * agent (opencode, a CLI tool config, curl with a hand-written key) against
 * this proxy. Every iteration cost a config edit plus a restart of that tool.
 * The Playground talks to `/api/dashboard/chat/completions` — the same route
 * the dashboard's own chat uses — so a bad model is observable in one click.
 *
 * Shape: a model selector on top, the transcript in the middle, and a stats
 * rail on the right. The rail is the point of the page during debugging:
 * time-to-first-token, total duration, output speed, and token counts are the
 * numbers you actually need when a model "feels broken".
 *
 * The model list is NOT built here. It comes from the shared
 * `ModelSelectModal`, which consumes the same `buildAvailableModels` pipeline
 * as the provider page's Available Models — so a model disabled on the
 * provider page is disabled here too, and custom/free rows show up without a
 * second implementation to keep in sync.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import Button from "@/shared/components/Button";
import ModelSelectModal from "@/shared/components/ModelSelectModal";
import { cn } from "@/shared/utils/cn";
import { streamChatCompletion, type ChatStreamUsage } from "@/shared/utils/chatStream";

const MODEL_KEY = "playground.model";
const CONVERSATION_KEY = "playground.conversation";

/** A row as handed back by ModelSelectModal. `value` is `alias/model-id`. */
interface SelectedModel {
  id: string;
  name: string;
  value: string;
  isFree?: boolean;
}

interface TurnStats {
  /** Request sent -> first content delta. `null` when nothing ever streamed. */
  ttftMs: number | null;
  /** Request sent -> stream end (or abort). */
  totalMs: number;
  promptTokens: number | null;
  completionTokens: number | null;
  totalTokens: number | null;
  /** Output tokens per second over the generation window. `null` if unknown. */
  tokensPerSec: number | null;
  /** True when tokensPerSec came from the char/4 estimate, not the server. */
  tokensPerSecEstimated: boolean;
  finishReason: string | null;
}

interface Turn {
  id: string;
  role: "user" | "assistant";
  content: string;
  /** Set on an assistant turn that failed; `content` may still be partial. */
  error?: string;
  /** The `alias/model-id` that produced this turn. */
  model?: string;
  stats?: TurnStats;
}

interface Connection {
  provider: string;
  name?: string;
  isActive?: boolean;
  providerSpecificData?: { prefix?: string };
}

let turnSeq = 0;
const nextTurnId = () => `t${++turnSeq}`;

/**
 * Re-key restored turns onto a fresh sequence.
 *
 * `turnSeq` is module-level, so it restarts at 0 on every page load and would
 * otherwise hand out `t1` again on top of a persisted `t1`. Ids are not merely
 * React keys: `flushStream` appends a delta to *every* turn whose id matches,
 * so one collision corrupts the transcript twice over. Re-keying also repairs
 * conversations already saved with duplicate ids.
 */
const adoptTurnIds = (turns: Turn[]): Turn[] => {
  const seen = new Set<string>();
  return turns.map((turn) => {
    let id = nextTurnId();
    while (seen.has(id)) id = nextTurnId();
    seen.add(id);
    return { ...turn, id };
  });
};

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

const HTML_ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

const escapeHtml = (raw: string): string =>
  raw.replace(/[&<>"']/g, (ch) => HTML_ESCAPES[ch] ?? ch);

/**
 * Model output is untrusted — a provider can echo back text it fetched — and
 * `marked` passes raw HTML straight through by default. Render html tokens as
 * literal text so `<script>` from a response body shows up as text, not a tag.
 */
const markdownRenderer = new marked.Renderer();
markdownRenderer.html = ({ text }: { text: string }) => escapeHtml(text);

const renderMarkdown = (source: string): string => {
  try {
    return marked.parse(source, {
      gfm: true,
      breaks: true,
      renderer: markdownRenderer,
    }) as string;
  } catch {
    return `<pre>${escapeHtml(source)}</pre>`;
  }
};

const formatMs = (ms: number | null | undefined): string => {
  if (ms == null || !Number.isFinite(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
};

const formatCount = (n: number | null | undefined): string =>
  n == null || !Number.isFinite(n) ? "—" : n.toLocaleString();

const formatRate = (n: number | null | undefined): string =>
  n == null || !Number.isFinite(n) ? "—" : `${n.toFixed(1)} tok/s`;

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

interface StatsInput {
  ttftMs: number | null;
  totalMs: number;
  usage: ChatStreamUsage | null;
  finishReason: string | null;
  /** Streamed text length, used only for the char/4 token estimate. */
  charCount: number;
}

function buildTurnStats(input: StatsInput): TurnStats {
  const { ttftMs, totalMs, usage, finishReason, charCount } = input;

  const reported = usage?.completion_tokens ?? null;
  // Providers that skip `include_usage` still streamed *something*. char/4 is
  // the usual approximation; flag it so the rail never passes an estimate off
  // as a server-reported number. The server can also report its own guess
  // (`estimated: true`), which is an estimate by another name.
  const completionTokens = reported ?? (charCount > 0 ? Math.round(charCount / 4) : null);
  const estimated = (reported == null || usage?.estimated === true) && completionTokens != null;

  // The client anchor measures send -> first *text* delta, so it stays null for
  // a turn that only produced reasoning. Fall back to the server's own
  // measurement so those turns report a real number instead of a dash.
  const measuredTtft = ttftMs ?? usage?.ttft_ms ?? null;

  // Prefer the generation window (first token -> end); fall back to the whole
  // request when the model produced text in a single chunk.
  const generationMs =
    measuredTtft != null && totalMs > measuredTtft ? totalMs - measuredTtft : totalMs;
  const tokensPerSec =
    completionTokens != null && generationMs > 0
      ? (completionTokens / generationMs) * 1000
      : null;

  return {
    ttftMs: measuredTtft,
    totalMs,
    promptTokens: usage?.prompt_tokens ?? null,
    completionTokens,
    totalTokens:
      usage?.total_tokens ??
      (usage?.prompt_tokens != null && completionTokens != null
        ? usage.prompt_tokens + completionTokens
        : completionTokens),
    tokensPerSec,
    tokensPerSecEstimated: estimated,
    finishReason,
  };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function PlaygroundPageClient() {
  const [model, setModel] = useState<SelectedModel | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [connections, setConnections] = useState<Connection[]>([]);
  const [hydrated, setHydrated] = useState(false);
  const [copiedTurnId, setCopiedTurnId] = useState<string | null>(null);

  const abortRef = useRef<AbortController | null>(null);
  /** Live stream scratchpad: text arrives here and is flushed per frame. */
  const streamRef = useRef<{ turnId: string; buffer: string; frame: number | null } | null>(null);
  const turnsRef = useRef<Turn[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    turnsRef.current = turns;
  }, [turns]);

  // Restore the last session. Runs once, guarded so a malformed/legacy
  // localStorage payload degrades to an empty playground instead of a crash.
  useEffect(() => {
    try {
      const rawModel = localStorage.getItem(MODEL_KEY);
      if (rawModel) {
        const parsed = JSON.parse(rawModel) as SelectedModel;
        if (parsed && typeof parsed.value === "string" && parsed.value) setModel(parsed);
      }
      const rawTurns = localStorage.getItem(CONVERSATION_KEY);
      if (rawTurns) {
        const parsed = JSON.parse(rawTurns) as Turn[];
        if (Array.isArray(parsed)) {
          setTurns(adoptTurnIds(parsed.filter((t) => t && typeof t.content === "string")));
        }
      }
    } catch {
      /* corrupt payload — start clean */
    }
    setHydrated(true);
  }, []);

  // Connected providers gate which groups ModelSelectModal offers, so an
  // OAuth provider you have not authorized stays out of the picker.
  useEffect(() => {
    let alive = true;
    fetch("/api/providers", { cache: "no-store" })
      .then((res) => (res.ok ? res.json() : null))
      .then((data) => {
        if (!alive) return;
        const list = (data?.connections ?? []) as Connection[];
        setConnections(list.filter((c) => c?.provider));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // Persist after hydration only, so the restore above is not immediately
  // overwritten by the empty initial state.
  useEffect(() => {
    if (!hydrated) return;
    try {
      if (model) localStorage.setItem(MODEL_KEY, JSON.stringify(model));
      else localStorage.removeItem(MODEL_KEY);
    } catch {
      /* quota or private mode — persistence is best-effort */
    }
  }, [model, hydrated]);

  useEffect(() => {
    if (!hydrated) return;
    try {
      localStorage.setItem(CONVERSATION_KEY, JSON.stringify(turns));
    } catch {
      /* quota — the transcript is a convenience, not the product */
    }
  }, [turns, hydrated]);

  // Pin the transcript to the newest content as it streams.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [turns]);

  const flushStream = useCallback(() => {
    const live = streamRef.current;
    if (!live) return;
    if (live.frame != null) {
      cancelAnimationFrame(live.frame);
      live.frame = null;
    }
    const text = live.buffer;
    live.buffer = "";
    if (!text) return;
    const turnId = live.turnId;
    setTurns((prev) =>
      prev.map((t) => (t.id === turnId ? { ...t, content: t.content + text } : t)),
    );
  }, []);

  // Batch deltas into one render per frame. Without this a fast provider
  // re-renders the whole transcript on every token chunk.
  const queueStream = useCallback(() => {
    const live = streamRef.current;
    if (!live || live.frame != null) return;
    live.frame = requestAnimationFrame(() => {
      const current = streamRef.current;
      if (!current) return;
      current.frame = null;
      const text = current.buffer;
      if (!text) return;
      current.buffer = "";
      const turnId = current.turnId;
      setTurns((prev) =>
        prev.map((t) => (t.id === turnId ? { ...t, content: t.content + text } : t)),
      );
    });
  }, []);

  const stop = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const clear = useCallback(() => {
    abortRef.current?.abort();
    setTurns([]);
    setInput("");
    setStreaming(false);
    inputRef.current?.focus();
  }, []);

  const send = useCallback(async () => {
    const prompt = input.trim();
    if (!prompt || !model || streaming) return;

    const history = [...turnsRef.current, { id: nextTurnId(), role: "user" as const, content: prompt }];
    const replyId = nextTurnId();
    setTurns([...history, { id: replyId, role: "assistant", content: "", model: model.value }]);
    setInput("");
    setStreaming(true);

    const controller = new AbortController();
    abortRef.current = controller;
    const sentAt = performance.now();
    let ttftMs: number | null = null;
    let finishReason: string | null = null;
    let usage: ChatStreamUsage | null = null;
    let charCount = 0;

    streamRef.current = { turnId: replyId, buffer: "", frame: null };

    try {
      await streamChatCompletion(
        {
          model: model.value,
          messages: history.map((t) => ({ role: t.role, content: t.content })),
          stream: true,
          signal: controller.signal,
        },
        {
          onText: (delta) => {
            charCount += delta.length;
            const live = streamRef.current;
            if (!live) return;
            live.buffer += delta;
            queueStream();
          },
          onFirstToken: () => {
            ttftMs = performance.now() - sentAt;
          },
          onUsage: (u) => {
            usage = u;
          },
          onFinishReason: (reason) => {
            finishReason = reason;
          },
        },
      );
    } catch (err) {
      // An abort is the Stop button, not a failure — keep the partial text.
      if (!controller.signal.aborted) {
        const message = err instanceof Error ? err.message : String(err);
        setTurns((prev) =>
          prev.map((t) => (t.id === replyId ? { ...t, error: message } : t)),
        );
      }
    } finally {
      flushStream();
      const stats = buildTurnStats({
        ttftMs,
        totalMs: performance.now() - sentAt,
        usage,
        finishReason,
        charCount,
      });
      setTurns((prev) =>
        prev.map((t) => (t.id === replyId ? { ...t, stats } : t)),
      );
      streamRef.current = null;
      abortRef.current = null;
      setStreaming(false);
    }
  }, [flushStream, input, model, queueStream, streaming]);

  const copyTurn = useCallback(async (turn: Turn) => {
    try {
      await navigator.clipboard.writeText(turn.content);
      setCopiedTurnId(turn.id);
      setTimeout(() => setCopiedTurnId((id) => (id === turn.id ? null : id)), 1500);
    } catch {
      /* clipboard blocked — the text is selectable anyway */
    }
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Enter sends, Shift+Enter inserts a newline.
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void send();
      }
    },
    [send],
  );

  // Per-turn summaries for the rail, oldest first.
  const history = useMemo(
    () =>
      turns
        .filter((t) => t.role === "assistant")
        .map((t) => ({ id: t.id, model: t.model ?? "", stats: t.stats, error: t.error })),
    [turns],
  );

  const latest = history.length > 0 ? history[history.length - 1] : null;

  const sessionTotals = useMemo(() => {
    const done = history.filter((h) => h.stats);
    if (done.length === 0) return null;
    const ttfts = done.map((h) => h.stats!.ttftMs).filter((v): v is number => v != null);
    const totals = done.map((h) => h.stats!.totalMs);
    const completion = done.reduce((sum, h) => sum + (h.stats!.completionTokens ?? 0), 0);
    const prompt = done.reduce((sum, h) => sum + (h.stats!.promptTokens ?? 0), 0);
    return {
      turns: done.length,
      avgTtft: ttfts.length ? ttfts.reduce((a, b) => a + b, 0) / ttfts.length : null,
      avgTotal: totals.reduce((a, b) => a + b, 0) / totals.length,
      completion,
      prompt,
    };
  }, [history]);

  const activeProviders = useMemo(
    () => connections.filter((c) => c.isActive !== false).map((c) => ({ ...c })),
    [connections],
  );

  const modelLabel = model ? model.name || model.value : "Select a model";

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-4">
      {/* ---- Header: model selector ---- */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <span className="material-symbols-outlined text-[20px] text-brand-coral">experiment</span>
          <h1 className="text-[18px] font-semibold text-ink">Playground</h1>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setPickerOpen(true)}
            className={cn(
              "flex h-10 min-w-0 max-w-[420px] items-center gap-2 rounded-mini-md border border-hairline bg-surface-card px-3 text-[13px] text-ink",
              "transition-colors hover:border-ink/40 focus:outline-none focus:ring-2 focus:ring-brand-coral/30",
            )}
            aria-label="Select model"
          >
            <span className="material-symbols-outlined shrink-0 text-[18px] text-text-muted">psychology</span>
            <span className="truncate">{modelLabel}</span>
            {model?.isFree && (
              <span className="shrink-0 rounded bg-green-500/10 px-1.5 py-0.5 text-[10px] font-bold text-green-500">
                FREE
              </span>
            )}
            <span className="material-symbols-outlined ml-auto shrink-0 text-[18px] text-text-muted">expand_more</span>
          </button>

          <Button
            variant="ghost"
            size="sm"
            icon="delete_sweep"
            onClick={clear}
            disabled={turns.length === 0}
          >
            Clear
          </Button>
        </div>
      </div>

      {/* ---- Body: transcript + stats rail ---- */}
      <div className="flex min-h-0 flex-1 gap-4">
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-mini-md border border-hairline bg-surface-card">
          <div ref={scrollRef} className="custom-scrollbar min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
            {turns.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
                <span className="material-symbols-outlined text-[32px] text-text-muted">forum</span>
                <p className="text-[13px] text-text-muted">
                  Pick a model, send a message, and watch the timings in the rail.
                </p>
              </div>
            ) : (
              turns.map((turn) =>
                turn.role === "user" ? (
                  <div key={turn.id} className="flex justify-end">
                    <div className="max-w-[80%] whitespace-pre-wrap rounded-mini-md bg-brand-coral px-3 py-2 text-[13px] text-on-primary">
                      {turn.content}
                    </div>
                  </div>
                ) : (
                  <div key={turn.id} className="group flex flex-col gap-1">
                    <div className="flex items-center gap-2 text-[11px] text-text-muted">
                      <span className="material-symbols-outlined text-[14px]">assistant</span>
                      <span className="font-mono">{turn.model ?? modelLabel}</span>
                      {turn.stats && (
                        <span className="font-mono">
                          {formatMs(turn.stats.ttftMs)} ttft · {formatMs(turn.stats.totalMs)} total
                        </span>
                      )}
                      {turn.content && (
                        <button
                          type="button"
                          onClick={() => void copyTurn(turn)}
                          className="ml-auto opacity-0 transition-opacity group-hover:opacity-100 focus:opacity-100"
                          aria-label="Copy response"
                        >
                          <span className="material-symbols-outlined text-[14px] hover:text-ink">
                            {copiedTurnId === turn.id ? "check" : "content_copy"}
                          </span>
                        </button>
                      )}
                    </div>
                    <div
                      className="text-[13px] leading-relaxed text-ink [&_code]:rounded [&_code]:bg-surface-soft [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[12px] [&_pre]:overflow-x-auto [&_pre]:rounded-mini-md [&_pre]:bg-surface-soft [&_pre]:p-3 [&_pre]:font-mono [&_pre]:text-[12px] [&_ul]:list-disc [&_ul]:pl-5"
                      dangerouslySetInnerHTML={{ __html: renderMarkdown(turn.content) }}
                    />
                    {turn.error && (
                      <div className="flex items-start gap-2 rounded-mini-md border border-red-500/30 bg-red-500/10 px-3 py-2">
                        <span className="material-symbols-outlined mt-0.5 shrink-0 text-[15px] text-red-500">
                          error
                        </span>
                        <p className="font-mono text-[12px] break-words text-red-500">{turn.error}</p>
                      </div>
                    )}
                    {streaming && turn.id === turns[turns.length - 1]?.id && !turn.content && (
                      <span className="material-symbols-outlined animate-spin text-[16px] text-text-muted">
                        progress_activity
                      </span>
                    )}
                  </div>
                ),
              )
            )}
          </div>

          {/* ---- Composer ---- */}
          <div className="border-t border-hairline p-3">
            <textarea
              ref={inputRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={onKeyDown}
              rows={3}
              disabled={!model}
              placeholder={
                model
                  ? `Message ${model.value} — Enter to send, Shift+Enter for a newline`
                  : "Select a model to start"
              }
              className={cn(
                "w-full resize-y rounded-mini-md border border-hairline bg-surface-2 px-3 py-2 text-[13px] text-ink",
                "placeholder:text-text-muted focus:border-brand-coral/40 focus:outline-none focus:ring-2 focus:ring-brand-coral/30",
                "disabled:cursor-not-allowed disabled:opacity-60",
              )}
            />
            <div className="mt-2 flex items-center justify-end gap-2">
              {streaming ? (
                <Button variant="secondary" size="sm" icon="stop" onClick={stop}>
                  Stop
                </Button>
              ) : (
                <Button
                  variant="primary"
                  size="sm"
                  icon="send"
                  onClick={() => void send()}
                  disabled={!model || !input.trim()}
                >
                  Send
                </Button>
              )}
            </div>
          </div>
        </div>

        {/* ---- Stats rail ---- */}
        <aside className="custom-scrollbar hidden w-[264px] shrink-0 overflow-y-auto rounded-mini-md border border-hairline bg-surface-card p-4 lg:block">
          <h2 className="mb-3 text-[12px] font-semibold tracking-wide text-text-muted uppercase">
            Last response
          </h2>
          <dl className="space-y-2 text-[12px]">
            <StatRow label="Time to first token" value={formatMs(latest?.stats?.ttftMs)} />
            <StatRow label="Total time" value={formatMs(latest?.stats?.totalMs)} />
            <StatRow
              label="Output speed"
              value={
                latest?.stats?.tokensPerSec == null
                  ? "—"
                  : `${formatRate(latest.stats.tokensPerSec)}${latest.stats.tokensPerSecEstimated ? " (est.)" : ""}`
              }
            />
            <StatRow label="Finish reason" value={latest?.stats?.finishReason ?? "—"} />
          </dl>

          <h2 className="mt-5 mb-3 text-[12px] font-semibold tracking-wide text-text-muted uppercase">
            Tokens
          </h2>
          <dl className="space-y-2 text-[12px]">
            <StatRow label="Prompt" value={formatCount(latest?.stats?.promptTokens)} />
            <StatRow label="Completion" value={formatCount(latest?.stats?.completionTokens)} />
            <StatRow label="Total" value={formatCount(latest?.stats?.totalTokens)} />
          </dl>

          {sessionTotals && (
            <>
              <h2 className="mt-5 mb-3 text-[12px] font-semibold tracking-wide text-text-muted uppercase">
                Session
              </h2>
              <dl className="space-y-2 text-[12px]">
                <StatRow label="Turns" value={String(sessionTotals.turns)} />
                <StatRow label="Avg time to first token" value={formatMs(sessionTotals.avgTtft)} />
                <StatRow label="Avg total time" value={formatMs(sessionTotals.avgTotal)} />
                <StatRow label="Completion total" value={formatCount(sessionTotals.completion)} />
              </dl>
            </>
          )}

          <h2 className="mt-5 mb-3 text-[12px] font-semibold tracking-wide text-text-muted uppercase">
            Turns
          </h2>
          {history.length === 0 ? (
            <p className="text-[12px] text-text-muted">No responses yet.</p>
          ) : (
            <ul className="space-y-1.5">
              {history
                .slice()
                .reverse()
                .map((h, i) => (
                  <li
                    key={h.id}
                    className="rounded-mini-md border border-hairline px-2 py-1.5 text-[11px]"
                  >
                    <div className="flex items-center gap-1.5">
                      <span className="text-text-muted">#{history.length - i}</span>
                      <span className="truncate font-mono text-ink" title={h.model}>
                        {h.model || "—"}
                      </span>
                      {h.error && (
                        <span className="material-symbols-outlined ml-auto text-[13px] text-red-500">
                          error
                        </span>
                      )}
                    </div>
                    <div className="mt-0.5 font-mono text-text-muted">
                      {formatMs(h.stats?.ttftMs)} ttft · {formatMs(h.stats?.totalMs)} total
                    </div>
                  </li>
                ))}
            </ul>
          )}
        </aside>
      </div>

      <ModelSelectModal
        isOpen={pickerOpen}
        onClose={() => setPickerOpen(false)}
        selectedModel={model?.value}
        activeProviders={activeProviders}
        title="Playground model"
        onSelect={(m) => {
          setModel({ id: m.id, name: m.name, value: m.value, isFree: m.isFree });
          setPickerOpen(false);
        }}
      />
    </div>
  );
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <dt className="text-text-muted">{label}</dt>
      <dd className="truncate font-mono text-ink">{value}</dd>
    </div>
  );
}
