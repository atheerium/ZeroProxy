"use client";

/**
 * Daily trend for the analytics page.
 *
 * `/api/usage/analytics` has always returned `dailyTrend` but nothing ever
 * rendered it, while the old /dashboard/usage tab drew its own chart from
 * `/api/usage/chart` (tokens + cost only, and that endpoint cannot answer the
 * 90d/ytd ranges). One chart, from the endpoint the page already calls, so the
 * trend and the stat cards above it can never disagree.
 */
import { useMemo } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { fmtCompact } from "@/lib/analytics/formatting";
import { formatNumber } from "@/components/usage/usageFormat";

export interface DailyTrendPoint {
  date: string;
  requests: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export default function DailyTrendChart({ data }: { data: DailyTrendPoint[] }) {
  const total = useMemo(
    () =>
      data.reduce(
        (acc, d) => ({
          requests: acc.requests + (d.requests || 0),
          totalTokens: acc.totalTokens + (d.totalTokens || 0),
        }),
        { requests: 0, totalTokens: 0 }
      ),
    [data]
  );

  const empty = data.length === 0;

  return (
    <div className="rounded-mini-xl border border-border bg-surface-2 p-5">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="material-symbols-outlined text-[18px] text-text-muted">show_chart</span>
          <h3 className="font-semibold text-text">Daily Trend</h3>
        </div>
        <div className="flex items-center gap-4 text-xs text-text-muted">
          <span className="tabular-nums">
            <span className="font-semibold text-text-main">{formatNumber(total.requests)}</span>{" "}
            requests
          </span>
          <span className="tabular-nums">
            <span className="font-semibold text-text-main">{fmtCompact(total.totalTokens)}</span>{" "}
            tokens
          </span>
        </div>
      </div>

      {empty ? (
        <div className="py-12 text-center text-sm text-text-muted">
          No daily data for this range yet.
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={260}>
          <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: -12 }}>
            <defs>
              <linearGradient id="requestsFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--color-primary)" stopOpacity={0.35} />
                <stop offset="100%" stopColor="var(--color-primary)" stopOpacity={0.02} />
              </linearGradient>
              <linearGradient id="tokensFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--color-success)" stopOpacity={0.25} />
                <stop offset="100%" stopColor="var(--color-success)" stopOpacity={0.02} />
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" className="stroke-border" vertical={false} />
            <XAxis
              dataKey="date"
              tick={{ fill: "currentColor", fontSize: 11 }}
              stroke="currentColor"
              tickLine={false}
              axisLine={false}
              minTickGap={24}
              className="text-text-muted"
            />
            <YAxis
              tick={{ fill: "currentColor", fontSize: 11 }}
              stroke="currentColor"
              tickLine={false}
              axisLine={false}
              width={52}
              className="text-text-muted"
              tickFormatter={fmtCompact}
            />
            <Tooltip
              contentStyle={{
                background: "var(--color-surface)",
                border: "1px solid var(--color-border)",
                borderRadius: 8,
                fontSize: 12,
              }}
              formatter={(value, name) => [
                name === "totalTokens" ? fmtCompact(Number(value)) : formatNumber(Number(value)),
                name === "totalTokens" ? "Tokens" : "Requests",
              ]}
            />
            <Area
              type="monotone"
              dataKey="requests"
              name="requests"
              stroke="var(--color-primary)"
              strokeWidth={2}
              fill="url(#requestsFill)"
            />
            <Area
              type="monotone"
              dataKey="totalTokens"
              name="tokens"
              stroke="var(--color-success)"
              strokeWidth={2}
              fill="url(#tokensFill)"
            />
          </AreaChart>
        </ResponsiveContainer>
      )}
    </div>
  );
}
