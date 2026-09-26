"use client";

import { useState, useEffect } from "react";
import Card from "@/shared/components/Card";

interface MemberOut {
  member: string;
  provider: string;
  model: string;
  requests: number;
  successRate: number;
  avgTtftMs: number;
  avgCost: number;
}

interface PresetOut {
  preset: string;
  requests: number;
  successRate: number;
  avgTtftMs: number;
  avgCost: number;
  fallbackRate: number;
  topErrorClass: string | null;
  members: MemberOut[];
}

interface AutoUsagePayload {
  summary: {
    requests: number;
    successRate: number;
    avgTtftMs: number;
    avgCost: number;
    fallbackRate: number;
    totalCost: number;
  };
  presets: PresetOut[];
  quarantine: { members: { model: string; remainingSeconds: number }[] };
  ungroupedPresetRequests?: number;
}

interface AutoBreakdownProps {
  period?: string;
}

const periodToBackend = (p: string) => {
  if (p === "24h") return "24h";
  if (p === "7d") return "7d";
  if (p === "30d") return "30d";
  if (p === "60d") return "60d";
  return "today";
};

const fmtPct = (n: number) => `${(n * 100).toFixed(1)}%`;
const fmtCost = (n: number) => `$${(n || 0).toFixed(4)}`;
const fmtMs = (n: number) => (n > 0 ? `${Math.round(n)}ms` : "—");
const fmtInt = (n: number) => new Intl.NumberFormat().format(n || 0);

function Rate({ value }: { value: number }) {
  const color =
    value >= 0.99
      ? "text-[color:var(--color-success)]"
      : value >= 0.95
        ? "text-[color:var(--color-warning)]"
        : "text-[color:var(--color-danger)]";
  return (
    <span className={`text-xs tabular-nums font-medium ${color}`}>{fmtPct(value)}</span>
  );
}

const PRESET_LABELS: Record<string, string> = {
  auto: "Balanced",
  "auto/best-coding": "Best Coding",
  "auto/best-free": "Best Free",
};

export default function AutoBreakdown({ period }: AutoBreakdownProps) {
  const [data, setData] = useState<AutoUsagePayload | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetch(`/api/usage/auto?period=${periodToBackend(period || "today")}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((json) => {
        if (!cancelled && json) setData(json);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [period]);

  if (loading && !data) return <Card className="p-4 text-sm text-text-muted">Loading…</Card>;
  if (!data) return <Card className="p-4 text-sm text-text-muted">No auto usage data.</Card>;

  const { summary, presets, quarantine, ungroupedPresetRequests } = data;

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
        <Card className="p-3">
          <div className="text-xs text-text-muted">Requests</div>
          <div className="text-lg font-semibold tabular-nums">{fmtInt(summary.requests)}</div>
        </Card>
        <Card className="p-3">
          <div className="text-xs text-text-muted">Success</div>
          <div className="text-lg font-semibold tabular-nums">
            <Rate value={summary.successRate} />
          </div>
        </Card>
        <Card className="p-3">
          <div className="text-xs text-text-muted">Avg TTFT</div>
          <div className="text-lg font-semibold tabular-nums">{fmtMs(summary.avgTtftMs)}</div>
        </Card>
        <Card className="p-3">
          <div className="text-xs text-text-muted">Fallback rate</div>
          <div className="text-lg font-semibold tabular-nums">{fmtPct(summary.fallbackRate)}</div>
        </Card>
        <Card className="p-3">
          <div className="text-xs text-text-muted">Avg cost</div>
          <div className="text-lg font-semibold tabular-nums">{fmtCost(summary.avgCost)}</div>
        </Card>
        <Card className="p-3">
          <div className="text-xs text-text-muted">Total cost</div>
          <div className="text-lg font-semibold tabular-nums">{fmtCost(summary.totalCost)}</div>
        </Card>
      </div>

      {ungroupedPresetRequests != null && ungroupedPresetRequests > 0 && (
        <Card className="p-3 text-xs text-text-muted">
          {fmtInt(ungroupedPresetRequests)} preset request(s) pre-date usage tagging and are not
          grouped into a bucket.
        </Card>
      )}

      {presets.length === 0 ? (
        <Card className="p-4 text-sm text-text-muted">
          No auto-tagged requests in this period. Send a request with model{" "}
          <code className="text-text">auto</code>, <code className="text-text">auto/best-coding</code>
          , or <code className="text-text">auto/best-free</code>.
        </Card>
      ) : (
        presets.map((preset) => (
          <Card key={preset.preset} className="p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <div>
                <div className="text-sm font-semibold">
                  {PRESET_LABELS[preset.preset] || preset.preset}
                </div>
                <div className="text-xs text-text-muted font-mono">{preset.preset}</div>
              </div>
              <div className="flex flex-wrap gap-4 text-xs tabular-nums">
                <span>
                  <span className="text-text-muted">Req</span> {fmtInt(preset.requests)}
                </span>
                <span>
                  <span className="text-text-muted">OK</span> <Rate value={preset.successRate} />
                </span>
                <span>
                  <span className="text-text-muted">TTFT</span> {fmtMs(preset.avgTtftMs)}
                </span>
                <span>
                  <span className="text-text-muted">Fallback</span> {fmtPct(preset.fallbackRate)}
                </span>
                <span>
                  <span className="text-text-muted">Cost</span> {fmtCost(preset.avgCost)}/req
                </span>
                {preset.topErrorClass && (
                  <span className="text-[color:var(--color-danger)]">
                    top err: {preset.topErrorClass}
                  </span>
                )}
              </div>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-border text-left text-text-muted">
                    <th className="py-2 pr-3 font-medium">Member</th>
                    <th className="py-2 pr-3 font-medium text-right">Requests</th>
                    <th className="py-2 pr-3 font-medium text-right">Success</th>
                    <th className="py-2 pr-3 font-medium text-right">Avg TTFT</th>
                    <th className="py-2 font-medium text-right">Avg cost</th>
                  </tr>
                </thead>
                <tbody>
                  {preset.members.map((m) => (
                    <tr key={m.member} className="border-b border-border/50 last:border-0">
                      <td className="py-2 pr-3 font-mono" title={m.member}>
                        {m.provider}
                        <span className="text-text-muted">/{m.model}</span>
                      </td>
                      <td className="py-2 pr-3 text-right tabular-nums">{fmtInt(m.requests)}</td>
                      <td className="py-2 pr-3 text-right">
                        <Rate value={m.successRate} />
                      </td>
                      <td className="py-2 pr-3 text-right tabular-nums">{fmtMs(m.avgTtftMs)}</td>
                      <td className="py-2 text-right tabular-nums">{fmtCost(m.avgCost)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Card>
        ))
      )}

      <Card className="p-4">
        <div className="mb-2 text-sm font-semibold">Quarantined now</div>
        {quarantine.members.length === 0 ? (
          <div className="text-xs text-text-muted">No members cooling down.</div>
        ) : (
          <ul className="flex flex-col gap-1 text-xs">
            {quarantine.members.map((q) => (
              <li key={q.model} className="flex justify-between font-mono">
                <span>{q.model}</span>
                <span className="text-text-muted tabular-nums">
                  {q.remainingSeconds}s remaining
                </span>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
