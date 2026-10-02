"use client";

/**
 * Live provider topology — the real-time React Flow graph that used to live on
 * /dashboard/usage (inside `UsageStats`), preserved here because nothing else
 * visualises which provider is serving traffic *right now*.
 *
 * Deliberately NOT wired to the analytics range selector. The SSE endpoint
 * (`/api/usage/stream`) only understands `today|24h|7d|30d|60d|all` and
 * **silently falls back to 7d** for anything else (`src/server/api/usage.rs`,
 * `stream_usage_stats`), so subscribing with `90d`/`ytd` would quietly show 7d
 * data. The graph is an inherently live, short-window view, so it pins `24h`
 * and the historical numbers come from `/api/usage/analytics` instead.
 */
import { Suspense, useEffect, useMemo, useState } from "react";
import { FREE_PROVIDERS } from "@/shared/constants/providers";
import ProviderTopology from "@/components/usage/ProviderTopologyWrapper";

interface StreamPayload {
  activeRequests?: any[];
  recentRequests?: any[];
  errorProvider?: string;
}

interface ConnectedProvider {
  provider: string;
  name?: string;
}

export default function LiveProviderTopology() {
  const [payload, setPayload] = useState<StreamPayload>({});
  const [providers, setProviders] = useState<ConnectedProvider[]>([]);
  const [connected, setConnected] = useState(false);

  // Connected providers, deduplicated by provider type. NoAuth free providers
  // (e.g. opencode-zen) have no connection to list, so add them explicitly —
  // same rule the old UsageStats used.
  useEffect(() => {
    let cancelled = false;
    fetch("/api/providers")
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (cancelled || !d) return;
        const seen = new Set<string>();
        const unique: ConnectedProvider[] = (d.connections || []).filter((c: any) => {
          if (!c?.provider || seen.has(c.provider)) return false;
          seen.add(c.provider);
          return true;
        });
        const noAuth = Object.values(FREE_PROVIDERS)
          .filter((p: any) => p.noAuth && !seen.has(p.id))
          .map((p: any) => ({ provider: p.id, name: p.name }));
        setProviders([...unique, ...noAuth]);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const es = new EventSource("/api/usage/stream?period=24h");
    es.onopen = () => setConnected(true);
    es.onmessage = (e) => {
      try {
        setPayload(JSON.parse(e.data));
      } catch (err) {
        console.error("[live-topology] parse error:", err);
      }
    };
    // onerror fires on normal reconnects too; leave `connected` alone so the
    // badge does not flap while the stream is retrying.
    return () => es.close();
  }, []);

  const activeRequests = useMemo(() => payload.activeRequests ?? [], [payload]);
  const lastProvider = payload.recentRequests?.[0]?.provider ?? "";

  return (
    <Suspense
      fallback={
        <div className="flex h-64 items-center justify-center rounded-mini-xl border border-border bg-surface-2 text-text-muted">
          <span className="material-symbols-outlined animate-spin text-2xl">progress_activity</span>
        </div>
      }
    >
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="material-symbols-outlined text-[18px] text-text-muted">hub</span>
            <h3 className="font-semibold text-text">Live Provider Topology</h3>
          </div>
          <span className="flex items-center gap-1.5 text-xs text-text-muted">
            <span
              className={`inline-block h-1.5 w-1.5 rounded-full ${
                connected ? "bg-[color:var(--color-success)]" : "bg-text-muted"
              }`}
            />
            {connected ? "streaming" : "connecting"}
          </span>
        </div>
        <ProviderTopology
          providers={providers}
          activeRequests={activeRequests}
          lastProvider={lastProvider}
          errorProvider={payload.errorProvider ?? ""}
        />
      </div>
    </Suspense>
  );
}
