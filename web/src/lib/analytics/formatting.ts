/**
 * Pure formatting/derivation logic for the analytics pages.
 * No React dependency — fully testable with vitest.
 *
 * Reuses existing helpers from @/components/usage/usageFormat where they exist.
 */

import { formatPercent } from "@/components/usage/usageFormat";

/**
 * Rate colour thresholds per the maintainer's spec:
 * >= 99 → green, >= 95 → amber, < 95 → red.
 */
export function rateColor(ratePct: number): "green" | "amber" | "red" {
  if (ratePct >= 99) return "green";
  if (ratePct >= 95) return "amber";
  return "red";
}

export function rateColorClass(ratePct: number): string {
  switch (rateColor(ratePct)) {
    case "green":
      return "text-green-500";
    case "amber":
      return "text-amber-500";
    case "red":
      return "text-red-500";
  }
}

/**
 * Share percentage for a bar/progress display.
 * Returns a number 0–100 rounded to 1 decimal.
 */
export function sharePct(part: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round((part / total) * 1000) / 10;
}

/**
 * Format a success rate string for display.
 * Returns "—" when the value is null/undefined/0, otherwise "X.X%".
 */
export function formatSuccessRate(successful: number, total: number): string {
  if (total <= 0 || successful == null) return "—";
  return formatPercent((successful / total) * 100);
}

/**
 * Format latency: ms under 1000, seconds above with 1 decimal.
 */
export function formatLatency(ms: number | null | undefined): string {
  if (ms == null || isNaN(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/**
 * Format a number with K/M/B suffixes, matching the compact style.
 */
export function fmtCompact(n: number | null | undefined): string {
  const v = n || 0;
  const abs = Math.abs(v);
  if (abs >= 1_000_000_000) return `${(v / 1_000_000_000).toFixed(1)}B`;
  if (abs >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  return new Intl.NumberFormat().format(v);
}

/**
 * Determine if a time window overlaps the pre-failure-counting period.
 * Returns true when the window starts before failureCountingStartedAt.
 */
export function windowOverlapsPreCounting(
  windowStart: string | null,
  failureCountingStartedAt: string | null
): boolean {
  if (!failureCountingStartedAt || !windowStart) return false;
  const start = new Date(windowStart).getTime();
  const countingStart = new Date(failureCountingStartedAt).getTime();
  return start < countingStart;
}

/**
 * Format a date as a short locale string (UTC).
 */
export function formatDateShort(iso: string | null): string {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    return d.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      timeZone: "UTC",
    });
  } catch {
    return "—";
  }
}

/**
 * Format a time as HH:MM:SS (UTC).
 */
export function formatTimeShort(iso: string | null): string {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    const h = String(d.getUTCHours()).padStart(2, "0");
    const m = String(d.getUTCMinutes()).padStart(2, "0");
    const s = String(d.getUTCSeconds()).padStart(2, "0");
    return `${h}:${m}:${s}`;
  } catch {
    return "—";
  }
}
