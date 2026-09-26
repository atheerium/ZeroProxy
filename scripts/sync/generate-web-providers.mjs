#!/usr/bin/env node
// Generate the dashboard's addable-provider list from the OmniRoute snapshot.
//
// WHY THIS EXISTS
//   `web/src/shared/constants/providers.ts` is hand-maintained: ~130 entries
//   across five literal maps. The committed snapshot
//   `src/core/model/sources/omniroute.json` is generated and free-tier-marked,
//   and carries 137 free providers. 93 of those were not offerable in the UI at
//   all. The Rust `sync omniroute --free-only` path only writes *models* into
//   the db, so it can never close that gap — this script closes the
//   provider-level half.
//
// THE DIVISION OF LABOUR
//   This file emits only providers that are ABSENT from the hand-maintained
//   maps, and it never rewrites an existing entry. Hand-curated entries carry
//   icons, colour, notices, deprecations and per-kind config that cannot be
//   derived from a snapshot; generated entries carry the minimum the `Provider`
//   type requires so the rest stays authoritative by hand.
//
// USAGE
//   node scripts/sync/generate-web-providers.mjs [--check] [--out <path>]
//
//   --check  verify the committed output is current; exit 1 if it is stale.
//            Intended for a pre-push hook, not for rewriting.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(HERE, "..", "..");
const SNAPSHOT = join(REPO_ROOT, "src", "core", "model", "sources", "omniroute.json");
const HAND_MAINTAINED = join(REPO_ROOT, "web", "src", "shared", "constants", "providers.ts");
const DEFAULT_OUT = join(REPO_ROOT, "web", "src", "shared", "constants", "providers.generated.ts");

// The dashboard draws colour only as a background behind white text; every hue
// here is dark enough for that to stay legible.
const PALETTE = [
  "#2563EB", "#7C3AED", "#DB2777", "#DC2626", "#EA580C", "#CA8A04", "#16A34A",
  "#0D9488", "#0891B2", "#4F46E5", "#9333EA", "#C026D3", "#E11D48", "#F97316",
  "#65A30D", "#059669", "#0284C7", "#6D28D9", "#A21CAF", "#BE123C",
];

// Material Symbols names are only used for `icon`, which no component renders
// as a glyph: every logo goes through `ProviderIcon`, which takes an SVG asset
// path and falls back to an initial-letter tile. Kept valid so the field stays
// honest if that ever changes.
const ICON = "hub";

const args = process.argv.slice(2);
const CHECK = args.includes("--check");
const outIdx = args.indexOf("--out");
const OUT = outIdx >= 0 ? args[outIdx + 1] : DEFAULT_OUT;

/** Keys declared by the hand-maintained maps, so we never shadow a curated entry. */
function handMaintainedIds() {
  const src = readFileSync(HAND_MAINTAINED, "utf8");
  const ids = new Set();
  const maps = [
    "FREE_PROVIDERS",
    "FREE_TIER_PROVIDERS",
    "OAUTH_PROVIDERS",
    "APIKEY_PROVIDERS",
    "WEB_COOKIE_PROVIDERS",
  ];
  for (const name of maps) {
    const m = src.match(new RegExp(`export const ${name}[^=]*=\\s*\\{`));
    if (!m) continue;
    let i = m.index + m[0].length - 1;
    let depth = 0;
    for (let j = i; j < src.length; j++) {
      if (src[j] === "{") depth++;
      else if (src[j] === "}") {
        depth--;
        if (depth === 0) {
          i = j;
          break;
        }
      }
    }
    // Walk the map body, tracking nesting, and take each depth-0 `key:`.
    const body = src.slice(m.index + m[0].length, i);
    let d = 0;
    let cur = "";
    for (const ch of body) {
      if (ch === "{" || ch === "[" || ch === "(") d++;
      else if (ch === "}" || ch === "]" || ch === ")") d--;
      else if (ch === "," && d === 0) cur = "";
      else if (ch === ":" && d === 0) {
        const key = cur.trim().replace(/^["'`]|["'`]$/g, "");
        if (key) ids.add(key);
        cur = "";
      } else cur += ch;
    }
  }
  return ids;
}

/** A short, stable, human-typable alias. OmniRoute often has none, or repeats its id. */
function aliasFor(id, taken) {
  const cleaned = id.replace(/[^a-z0-9]/gi, "").toLowerCase();
  for (let n = 2; n < 100; n++) {
    // Slice from the front so the alias reads like the provider name, then
    // lengthen until unique — 2 chars first because that is the house style.
    const base = cleaned.slice(0, n) || id.slice(0, n);
    if (!taken.has(base)) {
      taken.add(base);
      return base;
    }
  }
  const fallback = `${cleaned}-${taken.size}`;
  taken.add(fallback);
  return fallback;
}

function titleize(id) {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/**
 * `authType` in the snapshot is an upstream enum. The `Provider` type accepts
 * only "oauth" | "apikey" | "cookie", so anything else is dropped rather than
 * cast — an unrecognised auth mode must not be rendered as a confident claim.
 */
const AUTH_TYPE = { apikey: "apikey", oauth: "oauth", cookie: "cookie" };

function entryFor(provider, color, alias) {
  const entry = {
    id: provider.id,
    alias,
    name: titleize(provider.id),
    icon: ICON,
    color,
  };
  if (provider.baseUrl) {
    // `URL.origin` returns the *string* "null" for a non-http scheme (e.g. a
    // `cloudcode:` or `vscode:` style endpoint), and JSON.stringify would then
    // emit a literal website: "null" — a broken link. Only http(s) is a site.
    try {
      const url = new URL(provider.baseUrl);
      if (url.protocol === "http:" || url.protocol === "https:") {
        entry.website = url.origin;
      }
    } catch {
      // A malformed base URL is not worth failing generation over; the entry
      // is still useful for display.
    }
  }
  const auth = AUTH_TYPE[provider.authType];
  if (auth) entry.authType = auth;
  if (provider.noAuth) entry.noAuth = true;
  if (provider.passthroughModels) entry.passthroughModels = true;
  return entry;
}

function renderMap(name, doc, entries) {
  // Keys are JSON-quoted: provider ids contain hyphens, which are not valid
  // in a bare TS identifier and fail the build with `Expected "}" but found "-"`.
  const lines = entries.map(
    (e) => `  ${JSON.stringify(e.id)}: { ${Object.entries(e)
      .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
      .join(", ")} },`
  );
  return `/** ${doc} */\nexport const ${name}: Record<string, Provider> = {\n${lines.join(
    "\n"
  )}\n};\n`;
}

const snapshot = JSON.parse(readFileSync(SNAPSHOT, "utf8"));
const all = snapshot.providers ?? [];
const isFree = (p) => p.free === true || p.noAuth === true;

// Free first — this is the set the project optimises for (see AGENTS.md Mission).
const ordered = [
  ...all.filter((p) => isFree(p) && p.id),
  ...all.filter((p) => !isFree(p) && p.id),
];

const existing = handMaintainedIds();
const takenAliases = new Set();
const free = [];
const rest = [];

ordered.forEach((provider, index) => {
  if (existing.has(provider.id)) return; // hand-maintained entry always wins
  const entry = entryFor(
    provider,
    PALETTE[index % PALETTE.length],
    aliasFor(provider.id, takenAliases)
  );
  (isFree(provider) ? free : rest).push(entry);
});

const header = `// GENERATED FILE — do not edit by hand, and do not hand-maintain a
// second copy of any of this. Regenerate with:
//     node scripts/sync/generate-web-providers.mjs
//
// Source: src/core/model/sources/omniroute.json @ ${snapshot.ref ?? "unknown"}
// Only providers absent from ./providers.ts appear here, so curated entries stay
// authoritative. Each entry is emitted with just the five fields the \`Provider\`
// type requires plus what the snapshot can state truthfully.

import type { Provider } from "../../types";
`;

const out =
  header +
  "\n" +
  renderMap(
    "GENERATED_FREE_TIER_PROVIDERS",
    `Free-tier providers missing from the curated maps. Sorted ahead of everything\n * else so the free tier is what a user sees first.`,
    free
  ) +
  "\n" +
  renderMap(
    "GENERATED_OTHER_PROVIDERS",
    "Non-free providers, kept in a separate map so the free tier stays a single\n * import and AGENTS.md's free-tier-only scope stays easy to audit.",
    rest
  ) +
  `
/** Derived, not hand-maintained: the curated FREE_TIER_PROVIDER_IDS list had drifted. */
export const GENERATED_FREE_TIER_PROVIDER_IDS: string[] = Object.keys(
  GENERATED_FREE_TIER_PROVIDERS
);
`;

if (CHECK) {
  let current = "";
  try {
    current = readFileSync(OUT, "utf8");
  } catch {
    /* missing counts as stale */
  }
  if (current !== out) {
    console.error(
      `web provider list is STALE — regenerate:\n  node scripts/sync/generate-web-providers.mjs`
    );
    process.exit(1);
  }
  console.log(
    `web provider list current (${free.length} free + ${rest.length} other generated)`
  );
} else {
  writeFileSync(OUT, out);
  console.log(
    `wrote ${OUT}\n  free-tier added: ${free.length}\n  other added:    ${rest.length}\n  curated kept:    ${existing.size}`
  );
}
