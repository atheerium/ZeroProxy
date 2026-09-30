/**
 * Guard for the `vite.define` substitution trap (AGENTS.md Trap 9).
 *
 * A `define` that fails to substitute is invisible to `pnpm build`, to vitest,
 * to `astro check` and to `tsc` — all four pass — and then the browser throws
 * `__UI_GIT_SHA__ is not defined` at runtime, in production, only on the page
 * that reads the freshness badge. That is a green gate over a broken feature.
 *
 * This suite pins the SOURCE-level invariant, which is the half that can be
 * checked without a build (and therefore the half that actually runs in CI:
 * the `web` job order is install -> astro check -> vitest -> pnpm build, so a
 * test that only inspected `dist/` would skip on every CI run).
 *
 * The other half — that substitution actually happened in the emitted bundle —
 * is `scripts/check-ui-defines.mjs`, run as a post-build step, because there
 * is no build output to inspect at vitest time.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const WEB_ROOT = join(__dirname, "..", "..");
const SRC_ROOT = join(WEB_ROOT, "src");
const ASTRO_CONFIG = join(WEB_ROOT, "astro.config.mjs");

/** Matches a `__UI_*__` use, but not the `declare const` lines in env.d.ts. */
const UI_IDENTIFIER = /__UI_[A-Z0-9_]+__/g;

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "dist") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      // Tests are excluded on purpose, and this file most of all: its own
      // failure messages quote `__UI_*__` names, so scanning it would make the
      // guard report itself as an undefined-identifier bug on every run.
      if (entry !== "__tests__") walk(full, out);
    } else if (/\.(ts|tsx|astro|mjs|js)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

/** Identifier names that appear as a real expression in component/library code. */
function usedIdentifiers(): Map<string, string[]> {
  const used = new Map<string, string[]>();
  for (const file of walk(SRC_ROOT)) {
    // env.d.ts *declares* the globals for the type checker; it does not use them.
    if (file.endsWith("env.d.ts")) continue;
    const text = readFileSync(file, "utf8");
    for (const match of text.match(UI_IDENTIFIER) ?? []) {
      const sites = used.get(match) ?? [];
      sites.push(relative(WEB_ROOT, file));
      used.set(match, sites);
    }
  }
  return used;
}

/** Keys inside the `define: { ... }` block of astro.config.mjs. */
function definedIdentifiers(): Set<string> {
  const config = readFileSync(ASTRO_CONFIG, "utf8");
  const block = config.match(/define:\s*\{([\s\S]*?)\n\s{4}\}/);
  if (!block) {
    // Failing here is correct: it means the shape of the config changed and
    // this parser needs to follow it, not that the defines vanished.
    throw new Error(
      "Could not find a `define: { ... }` block in web/astro.config.mjs. " +
        "If the config was restructured, update the parser in uiDefines.test.ts " +
        "so this guard keeps guarding.",
    );
  }
  const keys = new Set<string>();
  for (const line of block[1].split("\n")) {
    const key = line.match(/^\s*([A-Za-z_$][\w$]*)\s*:/);
    if (key) keys.add(key[1]);
  }
  return keys;
}

/**
 * Defines that exist without a consumer in `src/` today. Listed rather than
 * deleted so this assertion stays a live drift detector instead of a permanent
 * red that everyone learns to ignore.
 */
const KNOWN_UNUSED_DEFINES = new Set(["__UI_COMMIT_TIME__"]);

describe("UI build-identity defines", () => {
  it("declares exactly the globals src/env.d.ts promises", () => {
    const declared = new Set(
      (readFileSync(join(SRC_ROOT, "env.d.ts"), "utf8").match(UI_IDENTIFIER) ?? []),
    );
    expect(declared.size).toBeGreaterThan(0);
    // A use without a declaration type-checks against `any` in some setups and
    // fails at runtime in all of them, so keep the two in lockstep.
    for (const [name, sites] of usedIdentifiers()) {
      expect(
        declared.has(name),
        `${name} is used at ${sites.join(", ")} but not declared in src/env.d.ts`,
      ).toBe(true);
    }
  });

  it("defines every __UI_*__ identifier that src actually uses", () => {
    const defined = definedIdentifiers();
    const used = usedIdentifiers();
    expect(used.size).toBeGreaterThan(0);

    const missing: string[] = [];
    for (const [name, sites] of used) {
      if (!defined.has(name)) {
        missing.push(`${name} (used at ${sites.join(", ")})`);
      }
    }

    // This is the assertion that would have caught the original bug: a new
    // `__UI_FOO__` used in a component but never added to `define` type-checks
    // fine, builds fine, and throws "is not defined" only in the browser.
    expect(
      missing,
      `These identifiers are used in src/ but absent from the define block in ` +
        `web/astro.config.mjs:\n  ${missing.join("\n  ")}\n` +
        `Add each one to \`define\`, or stop using it.`,
    ).toEqual([]);
  });

  it("does not leave a stale define that no longer has a consumer", () => {
    const used = usedIdentifiers();
    const stale = [...definedIdentifiers()].filter(
      (name) => !used.has(name) && !KNOWN_UNUSED_DEFINES.has(name),
    );
    expect(
      stale,
      `astro.config.mjs defines ${stale.join(", ")} but nothing in src/ uses ` +
        `them. Add a consumer, drop the define, or -- if it is deliberately ` +
        `reserved -- list it in KNOWN_UNUSED_DEFINES.`,
    ).toEqual([]);
  });
});
