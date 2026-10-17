/**
 * Pure logic behind the `tsc` error ratchet.
 *
 * Split out from the runner so the comparison itself is unit-testable —
 * `web/src/__tests__/tscBaseline.test.ts` imports this file directly, and
 * `check-tsc-baseline.mjs` is only a thin I/O wrapper around it.
 *
 * WHY PER-FILE AND NOT A TOTAL. A total is not a ratchet: fixing 10 errors in
 * one file while adding 10 in another leaves the total flat, so the gate stays
 * green through an arbitrary amount of new debt. This repo already lost a day
 * to that class of reasoning (AGENTS.md, verification traps). Every file is
 * therefore compared independently, and the gate fails on ANY file whose count
 * rose.
 */

/**
 * Parse `tsc --noEmit` output into a file -> error-count map.
 *
 * Only lines shaped `path(line,col): error TSxxxx: message` are counted. `tsc`
 * also emits non-error diagnostics (e.g. `Found 491 errors.`) and, on a crash,
 * free-form text; counting those would make the count drift for reasons that
 * have nothing to do with type errors.
 *
 * @param {string} output raw combined stdout+stderr from tsc
 * @returns {Record<string, number>} file path -> number of errors, sorted by path
 */
export function parseTscErrors(output) {
  /** @type {Record<string, number>} */
  const counts = Object.create(null);
  const line = /^(.+?)\(\d+,\d+\): error TS\d+:/;
  for (const raw of String(output ?? "").split(/\r?\n/)) {
    const m = line.exec(raw.trim());
    if (!m) continue;
    const file = m[1];
    counts[file] = (counts[file] ?? 0) + 1;
  }
  // Sorted so a regenerated baseline produces a stable, reviewable diff.
  return Object.fromEntries(Object.entries(counts).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

/**
 * Compare a recorded baseline against a freshly parsed current state.
 *
 * A file that is in the baseline but absent from `current` counts as 0 — that
 * is an improvement, not a disappearance to be worried about.
 *
 * @param {Record<string, number>} baseline recorded per-file counts
 * @param {Record<string, number>} current freshly parsed per-file counts
 */
export function compareTscErrors(baseline = {}, current = {}) {
  const files = new Set([...Object.keys(baseline ?? {}), ...Object.keys(current ?? {})]);

  /** @type {{file: string, before: number, after: number, delta: number}[]} */
  const regressions = [];
  /** @type {{file: string, before: number, after: number, delta: number}[]} */
  const improvements = [];
  let before = 0;
  let after = 0;

  for (const file of [...files].sort()) {
    const b = Number(baseline?.[file] ?? 0);
    const a = Number(current?.[file] ?? 0);
    before += b;
    after += a;
    if (a > b) regressions.push({ file, before: b, after: a, delta: a - b });
    else if (a < b) improvements.push({ file, before: b, after: a, delta: a - b });
  }

  return { regressions, improvements, before, after };
}

/** One line per regression, most negative-free (largest growth first). */
export function formatRegressions(regressions) {
  return regressions
    .slice()
    .sort((x, y) => y.delta - x.delta || (x.file < y.file ? -1 : 1))
    .map((r) => `  ${r.file}: ${r.before} -> ${r.after} (+${r.delta})`)
    .join("\n");
}
