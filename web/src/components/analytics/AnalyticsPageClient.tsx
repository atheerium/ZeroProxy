"use client";

import { useState, useEffect, useCallback, useMemo, Fragment } from "react";
import Card from "@/shared/components/Card";
import SegmentedControl from "@/shared/components/SegmentedControl";
import { formatNumber } from "@/components/usage/usageFormat";
import LiveProviderTopology from "@/components/analytics/LiveProviderTopology";
import DailyTrendChart from "@/components/analytics/DailyTrendChart";
import type { DailyTrendPoint } from "@/components/analytics/DailyTrendChart";
import RequestHistoryTable from "@/components/usage/RequestHistoryTable";
import {
  sharePct,
  formatLatency,
  fmtCompact,
  rateColorClass,
  windowOverlapsPreCounting,
  formatDateShort,
} from "@/lib/analytics/formatting";

const PERIODS = [
  { value: "1d", label: "1d" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" },
  { value: "ytd", label: "ytd" },
  { value: "all", label: "all" },
];

interface AnalyticsSummary {
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  successRatePct: string;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  uniqueModels: number;
  uniqueProviders: number;
  avgLatencyMs: number;
  avgTtftMs: number;
  firstRequest: string | null;
  lastRequest: string | null;
  failureCountingStartedAt: string | null;
}

interface AnalyticsResponse {
  range: string;
  summary: AnalyticsSummary;
  dailyTrend: DailyTrendPoint[];
  byProvider: Array<{
    provider: string;
    requests: number;
    successfulRequests: number;
    failedRequests: number;
    successRatePct: string;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    avgLatencyMs: number;
    avgTtftMs: number;
    sharePct: number;
  }>;
  byModel: Array<{
    model: string;
    provider: string;
    requests: number;
    successfulRequests: number;
    failedRequests: number;
    successRatePct: string;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    avgLatencyMs: number;
    avgTtftMs: number;
    sharePct: number;
  }>;
}

export default function AnalyticsPageClient() {
  const [range, setRange] = useState("30d");
  const [data, setData] = useState<AnalyticsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async (rng: string) => {
    try {
      setLoading(true);
      const res = await fetch(`/api/usage/analytics?range=${rng}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json: AnalyticsResponse = await res.json();
      setData(json);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load analytics");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData(range);
  }, [range, fetchData]);

  const summary = data?.summary;
  const overlapsPreCounting = useMemo(
    () =>
      summary
        ? windowOverlapsPreCounting(
            summary.firstRequest,
            summary.failureCountingStartedAt
          )
        : false,
    [summary]
  );

  const displaySuccessRate = useMemo(() => {
    if (!summary) return "—";
    if (summary.failureCountingStartedAt && overlapsPreCounting) {
      return `${summary.successRatePct} (since ${formatDateShort(summary.failureCountingStartedAt)})`;
    }
    return summary.successRatePct;
  }, [summary, overlapsPreCounting]);

  const totalTokens = summary?.totalTokens ?? 0;

  return (
    <div className="flex min-w-0 flex-col gap-6">
      {/* Time range selector */}
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2">
          <span className="material-symbols-outlined text-[22px] text-text">analytics</span>
          <h2 className="text-lg font-semibold text-text">Analytics</h2>
        </div>
        <SegmentedControl
          options={PERIODS}
          value={range}
          onChange={setRange}
          size="sm"
          className="w-full sm:w-auto"
        />
      </div>

      {loading && !data ? (
        <div className="flex items-center justify-center py-16 text-text-muted">
          <span className="material-symbols-outlined animate-spin text-3xl">progress_activity</span>
        </div>
      ) : error && !data ? (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-6 text-center">
          <span className="material-symbols-outlined text-red-500 text-[32px] mb-2">error</span>
          <p className="text-red-400">{error}</p>
          <button
            onClick={() => fetchData(range)}
            className="mt-4 px-4 py-2 rounded-lg bg-primary/10 text-primary text-sm hover:bg-primary/20 transition-colors"
          >
            Retry
          </button>
        </div>
      ) : (
        <>
          {/* Failure counting caveat */}
          {summary?.failureCountingStartedAt && overlapsPreCounting && (
            <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-sm text-amber-700 dark:text-amber-400">
              <span className="font-semibold">Note:</span> Failure recording began on{" "}
              <strong>{formatDateShort(summary.failureCountingStartedAt)}</strong>.
              The selected window starts before that date, so the success rate may be
              higher than the actual rate because failures were not persisted before that date.
            </div>
          )}

          {/* Summary stat cards */}
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
            <StatCard
              label="Total Requests"
              value={formatNumber(summary?.totalRequests ?? 0)}
              icon="analytics"
            />
            <StatCard
              label="Total Tokens"
              value={fmtCompact(summary?.totalTokens)}
              sub={`${fmtCompact(summary?.promptTokens)} in · ${fmtCompact(summary?.completionTokens)} out`}
              icon="toll"
            />
            <StatCard
              label="Input Tokens"
              value={fmtCompact(summary?.promptTokens)}
              icon="input"
              color="var(--color-danger)"
            />
            <StatCard
              label="Output Tokens"
              value={fmtCompact(summary?.completionTokens)}
              icon="output"
              color="var(--color-success)"
            />
            <StatCard
              label="Success Rate"
              value={displaySuccessRate}
              sub={summary?.failureCountingStartedAt && overlapsPreCounting ? "Caveat: pre-counting window" : undefined}
              icon="check_circle"
            />
            <StatCard
              label="Avg Latency"
              value={formatLatency(summary?.avgLatencyMs)}
              icon="timer"
            />
          </div>

          {/* LIVE TOPOLOGY — real-time, independent of the selected range */}
          <LiveProviderTopology />

          {/* DAILY TREND — rendered from the dailyTrend this page already fetches */}
          <DailyTrendChart data={data?.dailyTrend ?? []} />

          {/* PROVIDER BREAKDOWN table */}
          <Card title="Provider Breakdown" icon="table_chart" padding="md">
            <ProviderTable
              data={data?.byProvider ?? []}
              models={data?.byModel ?? []}
              totalTokens={totalTokens}
            />
          </Card>

          {/* MODEL BREAKDOWN table */}
          <Card title="Model Breakdown" icon="category" padding="md">
            <ModelTable data={data?.byModel ?? []} totalTokens={totalTokens} />
          </Card>

          {/* REQUEST HISTORY — the one instance of this table in the dashboard */}
          <Card title="Recent Requests" icon="history" padding="md">
            <RequestHistoryTable showFilters pageSize={20} />
          </Card>
        </>
      )}
    </div>
  );
}

function StatCard({
  label,
  value,
  sub,
  icon,
  color,
}: {
  label: string;
  value: string;
  sub?: string;
  icon: string;
  color?: string;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-mini-xl border border-border bg-surface-2 p-5">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-text-muted">
          {label}
        </span>
        <span className="material-symbols-outlined text-[18px] text-text-muted">
          {icon}
        </span>
      </div>
      <span
        className="text-2xl font-bold tabular-nums sm:text-3xl"
        style={color ? { color } : undefined}
      >
        {value}
      </span>
      {sub && <span className="text-xs text-text-muted">{sub}</span>}
    </div>
  );
}

type ProviderRow = {
  provider: string;
  requests: number;
  successfulRequests: number;
  failedRequests: number;
  successRatePct: string;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  avgLatencyMs: number;
  avgTtftMs: number;
  sharePct: number;
};

type ProviderModelRow = {
  model: string;
  provider: string;
  requests: number;
  successfulRequests: number;
  failedRequests: number;
  totalTokens: number;
  avgLatencyMs: number;
};

type ProviderSortKey = "requests" | "totalTokens" | "avgLatencyMs";

function ProviderTable({
  data,
  models,
  totalTokens,
}: {
  data: ProviderRow[];
  models: ProviderModelRow[];
  totalTokens: number;
}) {
  const [sortKey, setSortKey] = useState<ProviderSortKey>("requests");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const [expanded, setExpanded] = useState<string | null>(null);

  // Models per provider, for the expandable rows. This is what the old
  // /dashboard/provider-stats page existed for; `byModel` already carries the
  // provider on every row, so no extra request is needed.
  const modelsByProvider = useMemo(() => {
    const map = new Map<string, ProviderModelRow[]>();
    for (const m of models) {
      const list = map.get(m.provider) ?? [];
      list.push(m);
      map.set(m.provider, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => b.requests - a.requests);
    }
    return map;
  }, [models]);

  const sorted = useMemo(() => {
    const arr = [...data];
    arr.sort((a, b) => {
      const va = a[sortKey] ?? 0;
      const vb = b[sortKey] ?? 0;
      return sortDir === "desc" ? vb - va : va - vb;
    });
    return arr;
  }, [data, sortKey, sortDir]);

  const handleSort = (key: ProviderSortKey) => {
    if (sortKey === key) {
      setSortDir(sortDir === "desc" ? "asc" : "desc");
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };

  if (data.length === 0) {
    return <div className="py-8 text-center text-text-muted">No provider data recorded yet.</div>;
  }

  const sortMark = (key: ProviderSortKey) =>
    sortKey === key ? (sortDir === "desc" ? "↓" : "↑") : "";

  const thSortable = (key: ProviderSortKey) =>
    `text-right py-2.5 px-3 text-text-muted font-medium cursor-pointer hover:text-text-main transition-colors select-none ${
      sortKey === key ? "text-text-main" : ""
    }`;

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-border">
            <th className="text-left py-2.5 px-3 text-text-muted font-medium">PROVIDER</th>
            <th className={thSortable("requests")} onClick={() => handleSort("requests")}>
              REQUESTS {sortMark("requests")}
            </th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">SUCCESS</th>
            <th className={thSortable("avgLatencyMs")} onClick={() => handleSort("avgLatencyMs")}>
              LATENCY {sortMark("avgLatencyMs")}
            </th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">TTFT</th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">INPUT</th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">OUTPUT</th>
            <th className={thSortable("totalTokens")} onClick={() => handleSort("totalTokens")}>
              TOTAL {sortMark("totalTokens")}
            </th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">SHARE</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border/60">
          {sorted.map((p) => {
            const share = totalTokens > 0 ? sharePct(p.totalTokens, totalTokens) : 0;
            const providerModels = modelsByProvider.get(p.provider) ?? [];
            const isExpanded = expanded === p.provider;
            return (
              <Fragment key={p.provider}>
                <tr
                  className={`hover:bg-surface-soft transition-colors ${
                    providerModels.length > 0 ? "cursor-pointer" : ""
                  }`}
                  onClick={() =>
                    providerModels.length > 0 && setExpanded(isExpanded ? null : p.provider)
                  }
                >
                  <td className="py-2.5 px-3 font-medium text-text-main">
                    <span className="flex items-center gap-1.5">
                      {providerModels.length > 0 && (
                        <span className="material-symbols-outlined text-[14px] text-text-muted transition-transform">
                          {isExpanded ? "keyboard_arrow_down" : "chevron_right"}
                        </span>
                      )}
                      {p.provider}
                    </span>
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                    {formatNumber(p.requests)}
                  </td>
                  <td
                    className={`py-2.5 px-3 text-right tabular-nums ${rateColorClass(
                      p.successRatePct ? parseFloat(p.successRatePct) : 0
                    )}`}
                  >
                    {p.successRatePct || "—"}
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums text-text-muted">
                    {formatLatency(p.avgLatencyMs)}
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums text-text-muted">
                    {formatLatency(p.avgTtftMs)}
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums text-[color:var(--color-danger)]">
                    {fmtCompact(p.promptTokens)}
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums text-[color:var(--color-success)]">
                    {fmtCompact(p.completionTokens)}
                  </td>
                  <td className="py-2.5 px-3 text-right tabular-nums font-bold text-text-main">
                    {fmtCompact(p.totalTokens)}
                  </td>
                  <td className="py-2.5 px-3">
                    <div className="flex items-center justify-end gap-2">
                      <div className="flex-1 max-w-[100px] h-1.5 rounded-full bg-surface-soft overflow-hidden">
                        <div
                          className="h-full rounded-full bg-primary"
                          style={{ width: `${share}%` }}
                        />
                      </div>
                      <span className="text-text-muted text-xs tabular-nums w-10 text-right">
                        {share.toFixed(1)}%
                      </span>
                    </div>
                  </td>
                </tr>
                {isExpanded && providerModels.length > 0 && (
                  <tr>
                    <td colSpan={9} className="bg-surface-soft/40 px-3 py-2">
                      <table className="w-full text-xs">
                        <thead>
                          <tr className="text-text-muted">
                            <th className="text-left py-1 pr-3 font-medium">MODEL</th>
                            <th className="text-right py-1 px-3 font-medium">REQUESTS</th>
                            <th className="text-right py-1 px-3 font-medium">SUCCESS</th>
                            <th className="text-right py-1 px-3 font-medium">LATENCY</th>
                            <th className="text-right py-1 px-3 font-medium">TOKENS</th>
                          </tr>
                        </thead>
                        <tbody>
                          {providerModels.map((m) => (
                            <tr key={`${m.provider}/${m.model}`} className="border-t border-border/40">
                              <td className="py-1 pr-3 font-medium text-text-main">{m.model}</td>
                              <td className="py-1 px-3 text-right tabular-nums text-text-muted">
                                {formatNumber(m.requests)}
                              </td>
                              <td
                                className={`py-1 px-3 text-right tabular-nums ${rateColorClass(
                                  m.requests > 0 ? (m.successfulRequests / m.requests) * 100 : 0
                                )}`}
                              >
                                {m.requests > 0
                                  ? `${((m.successfulRequests / m.requests) * 100).toFixed(1)}%`
                                  : "—"}
                              </td>
                              <td className="py-1 px-3 text-right tabular-nums text-text-muted">
                                {formatLatency(m.avgLatencyMs)}
                              </td>
                              <td className="py-1 px-3 text-right tabular-nums text-text-muted">
                                {fmtCompact(m.totalTokens)}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </td>
                  </tr>
                )}
              </Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function ModelTable({
  data,
  totalTokens,
}: {
  data: Array<{
    model: string;
    provider: string;
    requests: number;
    successfulRequests: number;
    failedRequests: number;
    successRatePct: string;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    avgLatencyMs: number;
    avgTtftMs: number;
    sharePct: number;
  }>;
  totalTokens: number;
}) {
  const [sortKey, setSortKey] = useState<"model" | "totalTokens">("totalTokens");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");

  const sorted = useMemo(() => {
    const arr = [...data];
    arr.sort((a, b) => {
      if (sortKey === "model") {
        return sortDir === "desc"
          ? b.model.localeCompare(a.model)
          : a.model.localeCompare(b.model);
      }
      const va = a.totalTokens;
      const vb = b.totalTokens;
      return sortDir === "desc" ? vb - va : va - vb;
    });
    return arr;
  }, [data, sortKey, sortDir]);

  const handleSort = (key: "model" | "totalTokens") => {
    if (sortKey === key) {
      setSortDir(sortDir === "desc" ? "asc" : "desc");
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };

  if (data.length === 0) {
    return <div className="py-8 text-center text-text-muted">No model data recorded yet.</div>;
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-border">
            <th className="text-left py-2.5 px-3 text-text-muted font-medium">MODEL</th>
            <th className="text-left py-2.5 px-3 text-text-muted font-medium">PROVIDER</th>
            <th
              className="text-right py-2.5 px-3 text-text-muted font-medium cursor-pointer hover:text-text-main transition-colors select-none"
              onClick={() => handleSort("totalTokens")}
            >
              REQUESTS {sortKey === "totalTokens" ? (sortDir === "desc" ? "↓" : "↑") : ""}
            </th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">INPUT</th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">OUTPUT</th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">TOTAL</th>
            <th className="text-right py-2.5 px-3 text-text-muted font-medium">SHARE</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border/60">
          {sorted.map((m) => {
            const share = totalTokens > 0 ? sharePct(m.totalTokens, totalTokens) : 0;
            return (
              <tr key={`${m.provider}/${m.model}`} className="hover:bg-surface-soft transition-colors">
                <td className="py-2.5 px-3 font-medium text-text-main">{m.model}</td>
                <td className="py-2.5 px-3 text-text-muted">{m.provider}</td>
                <td className="py-2.5 px-3 text-right tabular-nums text-text-main">
                  {formatNumber(m.requests)}
                </td>
                <td className="py-2.5 px-3 text-right tabular-nums text-[color:var(--color-danger)]">
                  {fmtCompact(m.promptTokens)}
                </td>
                <td className="py-2.5 px-3 text-right tabular-nums text-[color:var(--color-success)]">
                  {fmtCompact(m.completionTokens)}
                </td>
                <td className="py-2.5 px-3 text-right tabular-nums font-bold text-text-main">
                  {fmtCompact(m.totalTokens)}
                </td>
                <td className="py-2.5 px-3 text-right">
                  <div className="flex items-center justify-end gap-2">
                    <div className="flex-1 max-w-[80px] h-1.5 rounded-full bg-surface-soft overflow-hidden">
                      <div
                        className="h-full rounded-full bg-primary"
                        style={{ width: `${share}%` }}
                      />
                    </div>
                    <span className="text-text-muted text-xs tabular-nums w-10 text-right">
                      {share.toFixed(1)}%
                    </span>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
