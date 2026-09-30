#!/usr/bin/env node
/**
 * Post-build half of the `vite.define` substitution guard (AGENTS.md Trap 9).
 *
 * The vitest suite `src/__tests__/uiDefines.test.ts` checks the source-level
 * invariant (every `__UI_*__` used is declared and defined), which is the half
 * that runs in CI -- the `web` job order is install -> astro check -> vitest ->
 * pnpm build, so a test that only read `dist/` would skip on every run.
 *
 * This script checks the other half: that substitution actually happened in the
 * emitted bundle. A `define` that silently fails to substitute leaves a bare
 * `__UI_GIT_SHA__` in the JS, which `pnpm build`, vitest, `astro check` and `tsc`
 * all pass -- and which then throws "__UI_GIT_SHA__ is not defined" in the
 * browser, on the freshness badge, in production only.
 *
 * Run after `pnpm build`. Exits 1 and names the offending files on failure.
 */
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const WEB_ROOT = new URL("..", import.meta.url).pathname;
const DIST = join(WEB_ROOT, "dist");

// Matches the identifier only when it is NOT a property access or a member of a
// longer name, i.e. a real free variable that would throw ReferenceError.
const BARE = /(^|[^A-Za-z0-9_$.])__UI_(BUILT_AT|GIT_SHA|COMMIT_TIME)__/g;

if (!existsSync(join(DIST, "index.html"))) {
  console.error(
    `check-ui-defines: ${relative(process.cwd(), DIST) || "dist"}/index.html is missing.\n` +
      `Run \`pnpm build\` first -- this check inspects build output, not source.`,
  );
  process.exit(1);
}

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

const targets = existsSync(join(DIST, "_astro"))
  ? walk(join(DIST, "_astro"))
  : walk(DIST);

const offenders = [];
let scanned = 0;
for (const file of targets) {
  if (!/\.(js|mjs|css|html)$/.test(file)) continue;
  scanned += 1;
  const text = readFileSync(file, "utf8");
  const hits = [...text.matchAll(BARE)].map((m) => m[0].trim());
  if (hits.length) {
    offenders.push({ file: relative(WEB_ROOT, file), hits: [...new Set(hits)] });
  }
}

if (offenders.length) {
  console.error("check-ui-defines: unsubstituted build-identity defines in the bundle.\n");
  console.error("These identifiers survived the build, so the browser will throw");
  console.error("\"__UI_*__ is not defined\" at runtime. Every gate above this point");
  console.error("(pnpm build, vitest, astro check, tsc) passes anyway.\n");
  for (const { file, hits } of offenders) {
    console.error(`  ${file}`);
    for (const hit of hits) console.error(`      ${hit}`);
  }
  console.error("\nUsual cause: the identifier is missing from the `define:` block in");
  console.error("web/astro.config.mjs, or the build is stale relative to src/.");
  process.exit(1);
}

console.log(`check-ui-defines: ok (${scanned} built files scanned, 0 unsubstituted)`);
