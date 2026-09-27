"use client";

import { useState, useEffect, useCallback, useMemo, useRef, Fragment } from "react";
import Card from "@/shared/components/Card";
import { formatNumber, formatPercent } from "@/components/usage/usageFormat";
import {
  fmtCompact,
  formatDateShort,
  formatTimeShort,
  formatLatency,
  rateColorClass,
} from "@/lib/analytics/formatting";

interface ProviderStat {
  provider: string;
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  successRatePct: string;
  avgLatencyMs: number;
  avgTtftMs: number;
  totalTokensIn: number;
  totalTokensOut: number;
}

interface ModelStat {
  provider: string;
  model: string;
  requests: number;
  successfulRequests: number;
  failedRequests: number;
  successRatePct: string;
  avgLatencyMs: number;
  avgTtftMs: number;
}

interface ProviderStatsResponse {
  updatedAt: string;
  failureCountingStartedAt: string | null;
  providers: ProviderStat[];
  models: ModelStat[];
}

export default function ProviderStatsPageClient() {
  const [data, setData] = useState<ProviderStatsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);
  const [sortKey, setSortKey] = useState<"totalRequests" | "avgLatencyMs">("totalRequests");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const handleSort = useCallback(
    (column: "totalRequests" | "avgLatencyMs") => {
      if (sortKey === column) setSortDir(d => (d === "asc" ? "desc" : "asc"));
      else { setSortKey(column); setSortDir("desc"); }
    },
    [sortKey],
  );
  const [expandedProvider, setExpandedProvider] = useState<string | null>(null);
  const [fetching, setFetching] = useState(false);

  const inFlight = useRef(false);

  const fetchData = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      setFetching(true);
      const res = await fetch("/api/usage/provider-stats");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json: ProviderStatsResponse = await res.json();
      setData(json);
      setError(null);
      setLastRefresh(new Date());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load provider stats");
    } finally {
      inFlight.current = false;
      setFetching(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  const handleRefresh = useCallback(() => {
    fetchData();
  }, [fetchData]);

  // Compute summary stats
  const totalRequests = data?.providers.reduce((s, p) => s + p.totalRequests, 0) ?? 0;
  const totalSuccessful = data?.providers.reduce((s, p) => s + p.successfulRequests, 0) ?? 0;
  // Request-weighted, not a plain mean of per-provider means: a provider with
  // 2 requests would otherwise count as much as one with 88.
  const latencySampleCount = data?.providers.reduce(
    (s, p) => s + (p.avgLatencyMs > 0 ? p.totalRequests : 0),
    0
  );
  const avgLatency = latencySampleCount
    ? Math.round(
        data!.providers.reduce((s, p) => s + (p.avgLatencyMs || 0) * p.totalRequests, 0) /
          latencySampleCount
      )
    : 0;
  const activeProviders = data?.providers.length ?? 0;
  // The backend emits avgLatencyMs/avgTtftMs as 0 when it has no samples, so 0
  // means "no data" and must not render as an instant response.
  const hasLatencyData = avgLatency > 0;
  const successRatePct = totalRequests > 0 ? (totalSuccessful / totalRequests) * 100 : null;

  // Sort providers
  const sortedProviders = useMemo(() => {
    const arr = [...(data?.providers ?? [])];
    arr.sort((a, b) => {
      let va: number, vb: number;
      if (sortKey === "totalRequests") {
        va = a.totalRequests;
        vb = b.totalRequests;
      } else if (sortKey === "avgLatencyMs") {
        va = a.avgLatencyMs || 0;
        vb = b.avgLatencyMs || 0;
      } else {
        // successRatePct
        va = a.totalRequests > 0 ? (a.successfulRequests / a.totalRequests) * 100 : 0;
        vb = b.totalRequests > 0 ? (b.successfulRequests / b.totalRequests) * 100 : 0;
      }
      return sortDir === "desc" ? vb - va : va - vb;
    });
    return arr;
  }, [data?.providers, sortKey, sortDir]);

  // Group models by provider
  const modelsByProvider = useMemo(() => {
    const map = new Map<string, ModelStat[]>();
    for (const m of data?.models ?? []) {
      if (!map.has(m.provider)) map.set(m.provider, []);
      map.get(m.provider)!.push(m);
    }
    return map;
  }, [data?.models]);

  const SortIcon = ({ column }: { column: string }) => {
    if (sortKey !== column) return null;
    return (
      <span className="material-symbols-outlined text-[14px] ml-1 align-middle text-primary">
        {sortDir === "desc" ? "arrow_downward" : "arrow_upward"}
      </span>
    );
  };

  if (error && !data) {
    return (
      <div>
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-6 text-center">
          <span className="material-symbols-outlined text-red-500 text-[32px] mb-2">error</span>
          <p className="text-red-400">{error}</p>
          <button
            onClick={handleRefresh}
            className="mt-4 px-4 py-2 rounded-lg bg-primary/10 text-primary text-sm hover:bg-primary/20 transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  const failureCountingStartedAt = data?.failureCountingStartedAt ?? null;
  // null means no failure has ever been recorded, so every rate on this page is
  // "no failures seen" rather than "100% healthy" — the distinction the user
  // needs most is the one that case falls on.
  const countingNotStarted = failureCountingStartedAt === null;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-end gap-3">
        {lastRefresh && (
          <span className="text-xs text-text-muted">
            Updated {formatTimeShort(lastRefresh.toISOString())}
          </span>
        )}
        <button
          onClick={handleRefresh}
          disabled={fetching}
          className="p-2 rounded-lg bg-surface hover:bg-surface/80 text-text-muted hover:text-text-main transition-colors disabled:opacity-50"
          title="Refresh"
        >
          <span className={`material-symbols-outlined text-[18px] ${fetching ? "animate-spin" : ""}`}>
            refresh
          </span>
        </button>
      </div>

      {/* Failure counting caveat */}
      {countingNotStarted ? (
        <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-sm text-amber-700 dark:text-amber-400">
          <span className="font-semibold">Success rate is not yet meaningful.</span> Provider
          failures were never recorded before this build, so these rates reflect only requests
          that succeeded. They will start reflecting real provider health once a failure is
          recorded.
        </div>
      ) : (
        <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-sm text-amber-700 dark:text-amber-400">
          <span className="font-semibold">Note:</span> Failure recording began on{" "}
          <strong>{formatDateShort(failureCountingStartedAt)}</strong>. Success rates for windows
          starting before that date may be higher than actual because failures were not persisted
          before that date.
        </div>
      )}

      {/* Summary Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card className="p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="flex items-center justify-center size-8 rounded-lg bg-primary/10 text-primary">
              <span className="material-symbols-outlined text-[18px]">analytics</span>
            </div>
            <span className="text-sm text-text-muted">Total Requests</span>
          </div>
          <p className="text-xl font-semibold text-text-main">{formatNumber(totalRequests)}</p>
        </Card>

        <Card className="p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="flex items-center justify-center size-8 rounded-lg bg-blue-500/10 text-blue-500">
              <span className="material-symbols-outlined text-[18px]">timer</span>
            </div>
            <span className="text-sm text-text-muted">Avg Latency</span>
          </div>
          <p className="text-xl font-semibold text-text-main">
            {hasLatencyData ? formatLatency(avgLatency) : "—"}
          </p>
        </Card>

        <Card className="p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="flex items-center justify-center size-8 rounded-lg bg-green-500/10 text-green-500">
              <span className="material-symbols-outlined text-[18px]">check_circle</span>
            </div>
            <span className="text-sm text-text-muted">Success Rate</span>
          </div>
          <p className="text-xl font-semibold text-text-main">
            {successRatePct === null ? "—" : formatPercent(successRatePct)}
          </p>
        </Card>

        <Card className="p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="flex items-center justify-center size-8 rounded-lg bg-purple-500/10 text-purple-500">
              <span className="material-symbols-outlined text-[18px]">dns</span>
            </div>
            <span className="text-sm text-text-muted">Active Providers</span>
          </div>
          <p className="text-xl font-semibold text-text-main">{activeProviders}</p>
        </Card>
      </div>

      {/* Provider Table */}
      <Card title="Provider Breakdown" icon="table_chart" padding="md">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-2.5 px-3 text-text-muted font-medium">Provider</th>
                <th
                  className="text-right py-2.5 px-3 text-text-muted font-medium cursor-pointer hover:text-text-main transition-colors select-none"
                  onClick={() => handleSort("totalRequests")}
                >
                  Requests <SortIcon column="totalRequests" />
                </th>
                <th className="text-right py-2.5 px-3 text-text-muted font-medium">Success</th>
                <th className="text-right py-2.5 px-3 text-text-muted font-medium">Rate</th>
                <th
                  className="text-right py-2.5 px-3 text-text-muted font-medium cursor-pointer hover:text-text-main transition-colors select-none"
                  onClick={() => handleSort("avgLatencyMs")}
                >
                  Avg Latency <SortIcon column="avgLatencyMs" />
                </th>
                <th className="text-right py-2.5 px-3 text-text-muted font-medium">Tokens In</th>
                <th className="text-right py-2.5 px-3 text-text-muted font-medium">Tokens Out</th>
                <th className="text-right py-2.5 px-3 text-text-muted font-medium">TTFT</th>
                <th className="py-2.5 px-3 w-8" />
              </tr>
            </thead>
            <tbody className="divide-y divide-border/60">
              {sortedProviders.map((p) => {
                const isExpanded = expandedProvider === p.provider;
                const models = modelsByProvider.get(p.provider) ?? [];
                const rate = p.totalRequests > 0 ? (p.successfulRequests / p.totalRequests) * 100 : 0;
                return (
                  <Fragment key={p.provider}>
                    <tr
                      className="border-b border-border/50 hover:bg-surface/50 transition-colors cursor-pointer"
                      onClick={() =>
                        setExpandedProvider(isExpanded ? null : models.length > 0 ? p.provider : null)
                      }
                    >
                      <td className="py-2.5 px-3 font-medium text-text-main">{p.provider}</td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                        {formatNumber(p.totalRequests)}
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                        {formatNumber(p.successfulRequests)}
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums">
                        <span className={rateColorClass(rate)}>
                          {rate.toFixed(1)}%
                        </span>
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                        {formatLatency(p.avgLatencyMs)}
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-muted">
                        {fmtCompact(p.totalTokensIn)}
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-muted">
                        {fmtCompact(p.totalTokensOut)}
                      </td>
                      <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                        {formatLatency(p.avgTtftMs)}
                      </td>
                      <td className="py-2.5 px-3 text-center">
                        {models.length > 0 && (
                          <span
                            className={`material-symbols-outlined text-[16px] text-text-muted transition-transform ${
                              isExpanded ? "rotate-90" : ""
                            }`}
                          >
                            chevron_right
                          </span>
                        )}
                      </td>
                    </tr>
                    {isExpanded && models.length > 0 && (
                      <tr key={`${p.provider}-models`}>
                        <td colSpan={9} className="p-0">
                          <div className="bg-surface-soft/30 border-b border-border/30">
                            <table className="w-full text-xs">
                              <thead>
                                <tr className="text-text-muted">
                                  <th className="text-left py-1.5 px-6 pl-12 font-medium">Model</th>
                                  <th className="text-right py-1.5 px-3 font-medium">Requests</th>
                                  <th className="text-right py-1.5 px-3 font-medium">Success</th>
                                  <th className="text-right py-1.5 px-3 font-medium">Rate</th>
                                  <th className="text-right py-1.5 px-3 font-medium">Avg Latency</th>
                                  <th className="px-3 w-8" />
                                </tr>
                              </thead>
                              <tbody>
                                {models.map((m) => {
                                  const mRate = m.requests > 0 ? (m.successfulRequests / m.requests) * 100 : 0;
                                  return (
                                    <tr key={m.model} className="border-t border-border/20">
                                      <td className="py-1.5 px-6 pl-12 font-mono text-text-main">
                                        {m.model}
                                      </td>
                                      <td className="py-1.5 px-3 text-right tabular-nums text-text-main">
                                        {formatNumber(m.requests)}
                                      </td>
                                      <td className="py-1.5 px-3 text-right tabular-nums text-text-main">
                                        {formatNumber(m.successfulRequests)}
                                      </td>
                                      <td className="py-1.5 px-3 text-right tabular-nums">
                                        <span className={rateColorClass(mRate)}>
                                          {mRate.toFixed(1)}%
                                        </span>
                                      </td>
                                      <td className="py-1.5 px-3 text-right tabular-nums text-text-main">
                                        {formatLatency(m.avgLatencyMs)}
                                      </td>
                                      <td className="px-3 w-8" />
                                    </tr>
                                  );
                                })}
                              </tbody>
                            </table>
                          </div>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
              {sortedProviders.length === 0 && (
                <tr>
                  <td colSpan={9} className="py-8 text-center text-text-muted">
                    No provider data recorded yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}
