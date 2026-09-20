"use client";

import { useState, useEffect, useMemo } from "react";
import Card from "@/shared/components/Card";
import { AI_PROVIDERS } from "@/shared/constants/providers";

interface ProviderStat {
  requests?: number;
  failedRequests?: number;
  promptTokens?: number;
  completionTokens?: number;
  cachedTokens?: number;
  cost?: number;
  latencyTotalSum?: number;
  latencyTtftSum?: number;
  latencyCount?: number;
}

interface ModelStat {
  requests?: number;
  failedRequests?: number;
  promptTokens?: number;
  completionTokens?: number;
  cost?: number;
  rawModel?: string;
  provider?: string;
  lastUsed?: string;
  latencyTotalSum?: number;
  latencyTtftSum?: number;
  latencyCount?: number;
}

interface UsageStatsPayload {
  byProvider?: Record<string, ProviderStat>;
  byModel?: Record<string, ModelStat>;
}

interface ProviderBreakdownTableProps {
  period?: string;
  byProvider?: Record<string, ProviderStat>;
  byModel?: Record<string, ModelStat>;
}

const periodToBackend = (p: string) => {
  if (p === "24h") return "24h";
  if (p === "7d") return "7d";
  if (p === "30d") return "30d";
  if (p === "60d") return "60d";
  return "today";
};

const compact = new Intl.NumberFormat(undefined, {
  notation: "compact",
  maximumFractionDigits: 1,
});

const fmtInt = (n: number) => new Intl.NumberFormat().format(n || 0);
const fmtCost = (n: number) => `$${(n || 0).toFixed(2)}`;
  const fmtMs = (n: number) => n > 0 ? `${Math.round(n)}ms` : "--";
  const formatLatencyMs = (ms: number | null) => (ms == null || ms === 0) ? "—" : ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;

function SuccessRate({ requests, failed }: { requests: number; failed?: number }) {
  if (failed === undefined || failed === null) {
    return <span className="text-text-muted text-xs">N/A</span>;
  }
  if (requests === 0) {
    return <span className="text-text-muted text-xs">--</span>;
  }
  const rate = ((requests - failed) / requests) * 100;
  const color =
    rate >= 99 ? "text-[color:var(--color-success)]" : rate >= 95 ? "text-[color:var(--color-warning)]" : "text-[color:var(--color-danger)]";
  return (
    <span className={`text-xs tabular-nums font-medium ${color}`}>
      {rate.toFixed(1)}%
    </span>
  );
}

export default function ProviderBreakdownTable({
  period: propPeriod,
  byProvider: propByProvider,
  byModel: propByModel,
}: ProviderBreakdownTableProps) {
  const isSelfFetching = !!propPeriod && !propByProvider;
  const [stats, setStats] = useState<UsageStatsPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [expandedProvider, setExpandedProvider] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);
  const [sortKey, setSortKey] = useState<string>("total");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");

  useEffect(() => {
    if (!isSelfFetching) return;
    let cancelled = false;
    setLoading(true);
    fetch(`/api/usage/stats?period=${periodToBackend(propPeriod)}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (!cancelled && data) setStats(data);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [propPeriod, isSelfFetching]);

  const byProvider = propByProvider || stats?.byProvider || {};
  const byModel = propByModel || stats?.byModel || {};

  // Auto-refresh every 30s when self-fetching
  useEffect(() => {
    if (!isSelfFetching) return;
    const timer = setInterval(() => {
      setLoading(true);
      fetch(`/api/usage/stats?period=${periodToBackend(propPeriod || "today")}`)
        .then((r) => (r.ok ? r.json() : null))
        .then((data) => {
          if (data) setStats(data);
          setLastRefresh(new Date());
        })
        .catch(() => {})
        .finally(() => setLoading(false));
    }, 30000);
    return () => clearInterval(timer);
  }, [isSelfFetching, propPeriod]);

  const entries = useMemo(() => {
    return Object.entries(byProvider).map(([id, data]) => {
      const config = AI_PROVIDERS[id] || { color: "#6b7280", name: id };
      const input = data.promptTokens || 0;
      const output = data.completionTokens || 0;
      const total = input + output;
      const latencyCount = data.latencyCount || 0;
      return {
        id,
        name: config.name || id,
        color: config.color || "#6b7280",
        requests: data.requests || 0,
        failedRequests: data.failedRequests,
        input,
        output,
        total,
        cost: data.cost || 0,
        avgLatency: latencyCount > 0 ? (data.latencyTotalSum || 0) / latencyCount : 0,
        avgTtft: latencyCount > 0 ? (data.latencyTtftSum || 0) / latencyCount : 0,
      };
    });
  }, [byProvider]);

  const sortEntries = (a: (typeof entries)[0], b: (typeof entries)[0]) => {
    const keys: Record<string, (e: typeof a) => number> = {
      total: (e) => e.total,
      requests: (e) => e.requests,
      input: (e) => e.input,
      output: (e) => e.output,
      cost: (e) => e.cost,
      avgLatency: (e) => e.avgLatency,
      avgTtft: (e) => e.avgTtft,
    };
    const va = keys[sortKey]?.(a) ?? 0;
    const vb = keys[sortKey]?.(b) ?? 0;
    return sortDir === "desc" ? vb - va : va - vb;
  };
  entries.sort(sortEntries);

  const grandTotal = entries.reduce((sum, e) => sum + e.total, 0);

  const modelsForProvider = useMemo(() => {
    if (!expandedProvider) return {};
    const result: Record<string, (typeof entries)[0] & { modelKey: string }> = {};
    for (const [key, data] of Object.entries(byModel)) {
      if (data.provider !== expandedProvider) continue;
      const input = data.promptTokens || 0;
      const output = data.completionTokens || 0;
      const latencyCount = data.latencyCount || 0;
      result[key] = {
        modelKey: key,
        id: data.rawModel || key,
        name: data.rawModel || key,
        color: AI_PROVIDERS[expandedProvider]?.color || "#6b7280",
        requests: data.requests || 0,
        failedRequests: data.failedRequests,
        input,
        output,
        total: input + output,
        cost: data.cost || 0,
        avgLatency: latencyCount > 0 ? (data.latencyTotalSum || 0) / latencyCount : 0,
        avgTtft: latencyCount > 0 ? (data.latencyTtftSum || 0) / latencyCount : 0,
      };
    }
    return result;
  }, [expandedProvider, byModel]);

  // Compute summary stats for cards
  const summaryTotalReq = entries.reduce((s, e) => s + e.requests, 0);
  const summaryFailedReq = entries.reduce((s, e) => s + (e.failedRequests || 0), 0);
  const summarySuccessRate = summaryTotalReq > 0 ? ((summaryTotalReq - summaryFailedReq) / summaryTotalReq) * 100 : 0;
  const summaryAvgLatency = entries.length > 0 ? Math.round(entries.reduce((s, e) => s + (e.avgLatency || 0), 0) / entries.length) : 0;
  const summaryTotalTokens = entries.reduce((s, e) => s + e.total, 0);
  const activeProvidersCount = entries.length;

  const handleSort = (key: string) => {
    if (sortKey === key) {
      setSortDir(sortDir === "desc" ? "asc" : "desc");
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };
  const SortArrow = ({ column }: { column: string }) => (
    <span className="ml-1 inline-block text-[10px] text-text-muted select-none">{sortKey === column ? (sortDir === "desc" ? "↓" : "↑") : ""}</span>
  );

  if (isSelfFetching && loading) {
    return (
      <Card padding="none" className="overflow-hidden">
        <div className="px-4 py-3 border-b border-border">
          <span className="text-sm font-semibold text-text-muted uppercase tracking-wide">Provider Breakdown</span>
        </div>
        <div className="px-4 py-8 text-center text-text-muted text-sm">Loading...</div>
      </Card>
    );
  }

  return (
    <Card padding="none" className="overflow-hidden">
      {/* Summary cards row */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3 p-4 border-b border-border">
        <div className="rounded-lg bg-surface/60 p-3">
          <div className="text-xs text-text-muted uppercase tracking-wide font-medium">Total Requests</div>
          <div className="text-xl font-bold text-text-main">{fmtInt(summaryTotalReq)}</div>
        </div>
        <div className="rounded-lg bg-surface/60 p-3">
          <div className="text-xs text-text-muted uppercase tracking-wide font-medium">Avg Latency</div>
          <div className="text-xl font-bold text-text-main">{formatLatencyMs(summaryAvgLatency)}</div>
        </div>
        <div className="rounded-lg bg-surface/60 p-3">
          <div className="text-xs text-text-muted uppercase tracking-wide font-medium">Success Rate</div>
          <div className={`text-xl font-bold ${summarySuccessRate >= 99 ? "text-[color:var(--color-success)]" : summarySuccessRate >= 95 ? "text-[color:var(--color-warning)]" : "text-[color:var(--color-danger)]"}`}>{summarySuccessRate.toFixed(1)}%</div>
        </div>
        <div className="rounded-lg bg-surface/60 p-3">
          <div className="text-xs text-text-muted uppercase tracking-wide font-medium">Providers</div>
          <div className="text-xl font-bold text-text-main">{activeProvidersCount}</div>
        </div>
      </div>

      {/* Header with refresh */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="text-sm font-semibold text-text-muted uppercase tracking-wide">Provider Breakdown</span>
        <div className="flex items-center gap-2">
          {lastRefresh && (
            <span className="text-[10px] text-text-muted whitespace-nowrap">Updated {lastRefresh.toLocaleTimeString()}</span>
          )}
          <button
            onClick={() => {
              setLoading(true);
              fetch(`/api/usage/stats?period=${periodToBackend(propPeriod || "today")}`)
                .then((r) => (r.ok ? r.json() : null))
                .then((data) => { if (data) setStats(data); })
                .catch(() => {})
                .finally(() => { setLoading(false); setLastRefresh(new Date()); });
            }}
            className="p-1.5 rounded-md hover:bg-surface/60 text-text-muted transition-colors"
            title="Refresh"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="23 4 23 10 17 10" /><path d="M1 20c1.5-1 3.5-1 5-1s4 .5 8 3 4-1.5 4-1.5" /><path d="M20.3 15.3a9 9 0 1 0-2.1 10.7" /></svg>
          </button>
        </div>
      </div>

      {entries.length === 0 ? (
        <div className="px-4 py-8 text-center text-text-muted text-sm">No provider usage recorded yet.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[900px] border-collapse text-sm">
            <thead>
              <tr className="border-b border-border text-text-muted">
                <th className="px-4 py-2.5 text-left font-semibold text-xs uppercase tracking-wide">Provider</th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("requests")}>Requests <SortArrow column="requests" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide">Success</th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("avgLatency")}>Avg Latency <SortArrow column="avgLatency" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("avgTtft")}>Avg TTFT <SortArrow column="avgTtft" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("input")}>Input <SortArrow column="input" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("output")}>Output <SortArrow column="output" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("total")}>Total <SortArrow column="total" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide cursor-pointer hover:text-text-main select-none" onClick={() => handleSort("cost")}>Cost <SortArrow column="cost" /></th>
                <th className="px-4 py-2.5 text-right font-semibold text-xs uppercase tracking-wide">Share</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/60">
              {entries.map((e) => {
                const share = grandTotal > 0 ? (e.total / grandTotal) * 100 : 0;
                const isExpanded = expandedProvider === e.id;
                const providerModels = isExpanded
                  ? Object.values(modelsForProvider).sort((a, b) => b.total - a.total)
                  : [];
                return (
                  <>
                    <tr
                      key={e.id}
                      className="hover:bg-surface-soft transition-colors cursor-pointer"
                      onClick={() =>
                        setExpandedProvider(isExpanded ? null : e.id)
                      }
                    >
                      <td className="px-4 py-2.5">
                        <div className="flex items-center gap-2 min-w-0">
                          <span
                            className="text-text-muted text-xs transition-transform"
                            style={{ display: "inline-block", transform: isExpanded ? "rotate(90deg)" : "rotate(0deg)" }}
                          >
                            ▸
                          </span>
                          <span
                            className="block w-2 h-2 rounded-full shrink-0"
                            style={{ backgroundColor: e.color }}
                          />
                          <span className="font-medium truncate">{e.name}</span>
                        </div>
                      </td>
                      <td className="px-4 py-2.5 text-right text-text-muted whitespace-nowrap">{fmtInt(e.requests)}</td>
                      <td className="px-4 py-2.5 text-right whitespace-nowrap">
                        <SuccessRate requests={e.requests} failed={e.failedRequests} />
                      </td>
                      <td className="px-4 py-2.5 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(e.avgLatency)}</td>
                      <td className="px-4 py-2.5 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(e.avgTtft)}</td>
                      <td className="px-4 py-2.5 text-right text-[color:var(--color-danger)] whitespace-nowrap">{compact.format(e.input)}</td>
                      <td className="px-4 py-2.5 text-right text-[color:var(--color-success)] whitespace-nowrap">{compact.format(e.output)}</td>
                      <td className="px-4 py-2.5 text-right font-bold whitespace-nowrap">{compact.format(e.total)}</td>
                      <td className="px-4 py-2.5 text-right text-[color:var(--color-warning)] whitespace-nowrap">{fmtCost(e.cost)}</td>
                      <td className="px-4 py-2.5">
                        <div className="flex items-center justify-end gap-2">
                          <div className="flex-1 max-w-[120px] h-1.5 rounded-full bg-surface-soft overflow-hidden">
                            <div
                              className="h-full rounded-full"
                              style={{ width: `${share}%`, backgroundColor: e.color }}
                            />
                          </div>
                          <span className="text-text-muted text-xs tabular-nums w-10 text-right">{share.toFixed(1)}%</span>
                        </div>
                      </td>
                    </tr>
                    {isExpanded &&
                      providerModels.map((m) => (
                        <tr key={`model-${m.modelKey}`} className="bg-surface-soft/30">
                          <td className="px-4 py-2 pl-10 text-text-muted text-xs">
                            {m.name}
                          </td>
                          <td className="px-4 py-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtInt(m.requests)}</td>
                          <td className="px-4 py-2 text-right whitespace-nowrap">
                            <SuccessRate requests={m.requests} failed={m.failedRequests} />
                          </td>
                          <td className="px-4 py-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(m.avgLatency)}</td>
                          <td className="px-4 py-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(m.avgTtft)}</td>
                          <td className="px-4 py-2 text-right text-[color:var(--color-danger)] text-xs whitespace-nowrap">{compact.format(m.input)}</td>
                          <td className="px-4 py-2 text-right text-[color:var(--color-success)] text-xs whitespace-nowrap">{compact.format(m.output)}</td>
                          <td className="px-4 py-2 text-right font-medium text-xs whitespace-nowrap">{compact.format(m.total)}</td>
                          <td className="px-4 py-2 text-right text-[color:var(--color-warning)] text-xs whitespace-nowrap">{fmtCost(m.cost)}</td>
                          <td className="px-4 py-2 text-right text-text-muted text-xs whitespace-nowrap">
                            {e.total > 0 ? `${((m.total / e.total) * 100).toFixed(1)}%` : "--"}
                          </td>
                        </tr>
                      ))}
                  </>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {/* Phase 2: Combo metrics section */}
      {stats?.byCombo && Object.keys(stats.byCombo || {}).length > 0 && (
        <div className="mt-4 px-4 pt-4 border-t border-border">
          <h3 className="text-sm font-semibold text-text-muted uppercase tracking-wide mb-3">Combo Metrics</h3>
          <div className="overflow-x-auto">
            <table className="w-full text-sm border-collapse">
              <thead>
                <tr className="border-b border-border text-text-muted">
                  <th className="text-left py-2 px-2 font-semibold text-xs uppercase tracking-wide">Combo</th>
                  <th className="text-right py-2 px-2 font-semibold text-xs uppercase tracking-wide">Requests</th>
                  <th className="text-right py-2 px-2 font-semibold text-xs uppercase tracking-wide">Avg TTFT</th>
                  <th className="text-right py-2 px-2 font-semibold text-xs uppercase tracking-wide">Avg Latency</th>
                  <th className="text-right py-2 px-2 font-semibold text-xs uppercase tracking-wide">Success</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/40">
                {Object.entries(stats.byCombo || {}).map(([comboId, data]) => {
                  const count = (data as any).requests || 0;
                  const failed = (data as any).failed_requests || 0;
                  const rate = count > 0 ? (((count - failed) / count) * 100) : 0;
                  const latCount = (data as any).latency_count || 0;
                  const avgLatency = latCount > 0 ? ((data as any).latency_total_sum || 0) / latCount : 0;
                  const avgTtft = latCount > 0 ? ((data as any).latency_ttft_sum || 0) / latCount : 0;
                  return (
                    <tr key={comboId} className="hover:bg-surface/40">
                      <td className="py-1.5 px-2 font-medium text-text-main text-xs truncate max-w-[180px]">{comboId}</td>
                      <td className="py-1.5 px-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtInt(count)}</td>
                      <td className="py-1.5 px-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(avgTtft)}</td>
                      <td className="py-1.5 px-2 text-right text-text-muted text-xs whitespace-nowrap">{fmtMs(avgLatency)}</td>
                      <td className="py-1.5 px-2 text-right whitespace-nowrap">
                        <span className={`text-xs font-medium ${rate >= 99 ? "text-[color:var(--color-success)]" : rate >= 95 ? "text-[color:var(--color-warning)]" : "text-[color:var(--color-danger)]"}`}>{rate.toFixed(1)}%</span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Card>
  );
}
