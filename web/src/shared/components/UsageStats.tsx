"use client";

import { useState, useEffect } from "react";
import { FREE_PROVIDERS } from "@/shared/constants/providers";
import OverviewCards from "@/components/usage/OverviewCards";
import ProviderTopology from "@/components/usage/ProviderTopologyWrapper";
import RequestHistoryTable from "@/components/usage/RequestHistoryTable";

import React from "react";

interface UsageStatsProps {
  period?: string;
  setPeriod?: (period: string) => void;
  hidePeriodSelector?: boolean;
}

interface StatsData {
  byModel?: Record<string, any>;
  byAccount?: Record<string, any>;
  byApiKey?: Record<string, any>;
  byEndpoint?: Record<string, any>;
  byProvider?: Record<string, { requests?: number; promptTokens?: number; completionTokens?: number; cachedTokens?: number; totalTokens?: number; cost?: number }>;
  activeRequests?: any[];
  recentRequests?: any[];
  errorProvider?: string;
  pending?: {
    byModel?: Record<string, number>;
    byAccount?: Record<string, Record<string, number>>;
  };
}

const PERIODS = [
  { value: "24h", label: "24h" },
  { value: "7d", label: "7D" },
  { value: "30d", label: "30D" },
  { value: "60d", label: "60D" },
];

interface Provider {
  provider: string;
  name?: string;
}

export default function UsageStats({ period: periodProp, setPeriod: setPeriodProp, hidePeriodSelector = false }: UsageStatsProps = {}) {
  const [stats, setStats] = useState<StatsData | null>(null);
  const [loading, setLoading] = useState(true);
  const [fetching, setFetching] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [periodLocal, setPeriodLocal] = useState("7d");
  const period = periodProp ?? periodLocal;
  const setPeriod = setPeriodProp ?? setPeriodLocal;

  // Fetch connected providers once, deduplicate by provider type
  // Always include noAuth free providers (e.g. opencode) regardless of connections
  useEffect(() => {
    fetch("/api/providers")
      .then((r) => r.ok ? r.json() : null)
      .then((d) => {
        const seen = new Set();
        const unique = (d?.connections || []).filter((c: any) => {
          if (seen.has(c.provider)) return false;
          seen.add(c.provider);
          return true;
        });
        const noAuthProviders = Object.values(FREE_PROVIDERS)
          .filter((p: any) => p.noAuth && !seen.has(p.id))
          .map((p: any) => ({ provider: p.id, name: p.name }));
        setProviders([...unique, ...noAuthProviders]);
      })
      .catch(() => {});
  }, []);

  // Fetch filtered stats via REST when period changes
  useEffect(() => {
    // First load: show full spinner; subsequent: show subtle fetching indicator
    if (!stats) setLoading(true);
    else setFetching(true);

    fetch(`/api/usage/stats?period=${period}`)
      .then((r) => r.ok ? r.json() : null)
      .then((data) => {
        if (data) setStats((prev) => ({ ...prev, ...data }));
      })
      .catch(() => {})
      .finally(() => {
        setLoading(false);
        setFetching(false);
      });
  }, [period]); // eslint-disable-line react-hooks/exhaustive-deps

  // SSE connection - real-time updates; reconnects when period changes
  useEffect(() => {
    const es = new EventSource(`/api/usage/stream?period=${period}`);

    es.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data);
        // Merge full payload so totals, byModel, byProvider etc. update in real-time
        setStats((prev) => ({
          ...(prev || {}),
          ...data,
        }));
        setLoading(false);
      } catch (err) {
        console.error("[SSE CLIENT] parse error:", err);
      }
    };

    es.onerror = () => setLoading(false);

    return () => es.close();
  }, [period]);

  // Compute active table data removed — unified RequestHistoryTable used below
  if (!stats && !loading) return <div className="text-text-muted">Failed to load usage statistics.</div>;

  const spinner = (
    <div className="flex items-center justify-center py-12 text-text-muted">
      <span className="material-symbols-outlined text-[32px] animate-spin">progress_activity</span>
    </div>
  );

  return (
    <div className="flex min-w-0 flex-col gap-6">
      {/* Period selector (hidden when controlled by parent) */}
      {!hidePeriodSelector && (
        <div className="flex w-full items-center gap-2 sm:w-auto sm:self-end">
          <div className="grid flex-1 grid-cols-4 items-center gap-1 rounded-lg border border-border bg-bg-subtle p-1 sm:flex sm:flex-none">
            {PERIODS.map((p) => (
              <button
                key={p.value}
                onClick={() => setPeriod(p.value)}
                disabled={fetching}
                className={`rounded-md px-3 py-1 text-sm font-medium transition-colors ${period === p.value ? "bg-primary text-white shadow-sm" : "text-text-muted hover:bg-bg-hover hover:text-text"}`}
              >
                {p.label}
              </button>
            ))}
          </div>
          {fetching && (
            <span className="material-symbols-outlined text-[16px] text-text-muted animate-spin">progress_activity</span>
          )}
        </div>
      )}

      {/* Overview cards */}
      {loading ? spinner : <OverviewCards stats={stats} />}

      {/* Provider topology */}
      {loading ? spinner : (
        <ProviderTopology
          providers={providers}
          activeRequests={stats.activeRequests || []}
          lastProvider={stats.recentRequests?.[0]?.provider || ""}
          errorProvider={stats.errorProvider || ""}
        />
      )}

      {/* Unified requests table (merged Recent Requests + Usage by Model) */}
      <div className="flex flex-col gap-3">
        {loading ? spinner : (
          <RequestHistoryTable showFilters={false} pageSize={20} />
        )}
      </div>
    </div>
  );
}
