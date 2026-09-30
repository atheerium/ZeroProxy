import { describe, it, expect } from "vitest";
import {
  parseTscErrors,
  compareTscErrors,
  formatRegressions,
} from "../../scripts/lib/tscBaseline.mjs";

/**
 * Tests for the `tsc` error ratchet's comparison logic.
 *
 * The ratchet is only as trustworthy as this comparison, and its whole job is
 * to catch a change that a total-error check would miss — so the case that
 * matters most is the offsetting one, asserted explicitly below.
 */
describe("parseTscErrors", () => {
  it("counts one entry per error line, grouped by file", () => {
    const out = [
      "src/a.tsx(1,2): error TS2322: Type 'x' is not assignable.",
      "src/a.tsx(9,4): error TS6133: 'y' is declared but never read.",
      "src/b.ts(3,1): error TS2304: Cannot find name 'z'.",
    ].join("\n");
    expect(parseTscErrors(out)).toEqual({ "src/a.tsx": 2, "src/b.ts": 1 });
  });

  it("ignores non-error diagnostics so the count tracks type errors only", () => {
    const out = [
      "src/a.ts(1,2): error TS2322: boom",
      "Found 1 error in 1 file.",
    ].join("\n");
    expect(parseTscErrors(out)).toEqual({ "src/a.ts": 1 });
  });

  it("returns an empty map for empty or unrecognised output", () => {
    expect(parseTscErrors("")).toEqual({});
    expect(parseTscErrors(undefined as unknown as string)).toEqual({});
    expect(parseTscErrors("error TS5023: Unknown compiler option")).toEqual({});
  });

  it("produces a key-sorted map so a regenerated baseline diffs cleanly", () => {
    const out = [
      "src/z.ts(1,1): error TS1: a",
      "src/a.ts(1,1): error TS1: b",
      "src/m.ts(1,1): error TS1: c",
    ].join("\n");
    expect(Object.keys(parseTscErrors(out))).toEqual(["src/a.ts", "src/m.ts", "src/z.ts"]);
  });
});

describe("compareTscErrors", () => {
  it("passes when nothing changed", () => {
    const s = { "src/a.ts": 3, "src/b.ts": 1 };
    const r = compareTscErrors(s, { ...s });
    expect(r.regressions).toEqual([]);
    expect(r.improvements).toEqual([]);
    expect(r.before).toBe(4);
    expect(r.after).toBe(4);
  });

  it("fails when a single file gains errors", () => {
    const r = compareTscErrors({ "src/a.ts": 3 }, { "src/a.ts": 5 });
    expect(r.regressions).toEqual([{ file: "src/a.ts", before: 3, after: 5, delta: 2 }]);
  });

  // The reason this ratchet is per-file. A total-only check sees 0 net change
  // here and stays green through 9 brand-new errors.
  it("catches offsetting growth that a total-error check would miss", () => {
    const r = compareTscErrors(
      { "src/fixed.tsx": 20, "src/grew.tsx": 2 },
      { "src/fixed.tsx": 11, "src/grew.tsx": 11 },
    );
    expect(r.after).toBe(r.before); // total is flat...
    expect(r.regressions).toHaveLength(1); // ...but a file regressed
    expect(r.regressions[0]).toMatchObject({ file: "src/grew.tsx", before: 2, after: 11, delta: 9 });
    expect(r.improvements).toHaveLength(1);
  });

  it("treats a brand-new file with errors as a regression from zero", () => {
    const r = compareTscErrors({}, { "src/new.tsx": 2 });
    expect(r.regressions).toEqual([{ file: "src/new.tsx", before: 0, after: 2, delta: 2 }]);
  });

  it("treats a file that dropped out of the report as fully fixed, not missing", () => {
    const r = compareTscErrors({ "src/gone.ts": 4 }, {});
    expect(r.regressions).toEqual([]);
    expect(r.improvements).toEqual([{ file: "src/gone.ts", before: 4, after: 0, delta: -4 }]);
    expect(r.after).toBe(0);
  });

  it("never reports the same file as both regressed and improved", () => {
    const r = compareTscErrors({ "src/a.ts": 5 }, { "src/a.ts": 1 });
    expect(r.regressions).toEqual([]);
    expect(r.improvements).toHaveLength(1);
  });
});

describe("formatRegressions", () => {
  it("orders by growth then path, one line per file", () => {
    const text = formatRegressions([
      { file: "src/small.ts", before: 1, after: 2, delta: 1 },
      { file: "src/big.ts", before: 1, after: 40, delta: 39 },
    ]);
    expect(text).toBe(["  src/big.ts: 1 -> 40 (+39)", "  src/small.ts: 1 -> 2 (+1)"].join("\n"));
  });

  it("is empty when nothing regressed", () => {
    expect(formatRegressions([])).toBe("");
  });
});
