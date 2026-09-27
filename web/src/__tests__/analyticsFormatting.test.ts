import { describe, it, expect } from "vitest";
import {
  rateColor,
  rateColorClass,
  sharePct,
  formatSuccessRate,
  formatLatency,
  fmtCompact,
  windowOverlapsPreCounting,
  formatDateShort,
  formatTimeShort,
} from "@/lib/analytics/formatting";

describe("rateColor", () => {
  it("returns green for >= 99", () => {
    expect(rateColor(99)).toBe("green");
    expect(rateColor(100)).toBe("green");
    expect(rateColor(99.5)).toBe("green");
  });
  it("returns amber for >= 95 and < 99", () => {
    expect(rateColor(95)).toBe("amber");
    expect(rateColor(97)).toBe("amber");
    expect(rateColor(98.9)).toBe("amber");
  });
  it("returns red for < 95", () => {
    expect(rateColor(94)).toBe("red");
    expect(rateColor(50)).toBe("red");
    expect(rateColor(0)).toBe("red");
  });
});

describe("rateColorClass", () => {
  it("maps to correct CSS classes", () => {
    expect(rateColorClass(99)).toBe("text-green-500");
    expect(rateColorClass(95)).toBe("text-amber-500");
    expect(rateColorClass(94)).toBe("text-red-500");
  });
});

describe("sharePct", () => {
  it("computes percentage rounded to 1 decimal", () => {
    expect(sharePct(500, 1000)).toBe(50);
    expect(sharePct(333, 1000)).toBe(33.3);
    expect(sharePct(1, 3)).toBe(33.3);
  });
  it("returns 0 when total is 0", () => {
    expect(sharePct(100, 0)).toBe(0);
    expect(sharePct(0, 0)).toBe(0);
  });
});

describe("formatSuccessRate", () => {
  it("returns formatted percentage", () => {
    expect(formatSuccessRate(95, 100)).toBe("95.0%");
    expect(formatSuccessRate(99, 100)).toBe("99.0%");
  });
  it("returns — when total is 0", () => {
    expect(formatSuccessRate(0, 0)).toBe("—");
  });
  it("returns — when successful is 0", () => {
    expect(formatSuccessRate(0, 100)).toBe("0.0%");
  });
});

describe("formatLatency", () => {
  it("returns ms under 1000", () => {
    expect(formatLatency(500)).toBe("500ms");
    expect(formatLatency(999)).toBe("999ms");
  });
  it("returns seconds above 1000", () => {
    expect(formatLatency(1000)).toBe("1.0s");
    expect(formatLatency(1500)).toBe("1.5s");
  });
  it("returns — for null/undefined", () => {
    expect(formatLatency(null)).toBe("—");
    expect(formatLatency(undefined)).toBe("—");
  });
});

describe("fmtCompact", () => {
  it("formats with K/M/B suffixes", () => {
    expect(fmtCompact(500)).toBe("500");
    expect(fmtCompact(1500)).toBe("1.5K");
    expect(fmtCompact(1500000)).toBe("1.5M");
    expect(fmtCompact(1500000000)).toBe("1.5B");
  });
  it("returns 0 for null/undefined", () => {
    expect(fmtCompact(null)).toBe("0");
    expect(fmtCompact(undefined)).toBe("0");
  });
});

describe("windowOverlapsPreCounting", () => {
  it("returns true when window starts before counting started", () => {
    expect(
      windowOverlapsPreCounting("2025-01-01T00:00:00Z", "2025-06-01T00:00:00Z")
    ).toBe(true);
  });
  it("returns false when window starts after counting started", () => {
    expect(
      windowOverlapsPreCounting("2025-07-01T00:00:00Z", "2025-06-01T00:00:00Z")
    ).toBe(false);
  });
  it("returns false when failureCountingStartedAt is null", () => {
    expect(windowOverlapsPreCounting("2025-01-01T00:00:00Z", null)).toBe(false);
  });
  it("returns false when windowStart is null", () => {
    expect(windowOverlapsPreCounting(null, "2025-06-01T00:00:00Z")).toBe(false);
  });
});

describe("formatDateShort", () => {
  it("formats a valid ISO date", () => {
    const result = formatDateShort("2025-06-15T00:00:00Z");
    expect(result).toBe("Jun 15, 2025");
  });
  it("returns — for null", () => {
    expect(formatDateShort(null)).toBe("—");
  });
});

describe("formatTimeShort", () => {
  it("formats a valid ISO time", () => {
    const result = formatTimeShort("2025-06-15T14:30:00Z");
    expect(result).toBe("14:30:00");
  });
  it("returns — for null", () => {
    expect(formatTimeShort(null)).toBe("—");
  });
});
