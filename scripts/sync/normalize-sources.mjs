#!/usr/bin/env node
// Maintainer-only helper. Pulls 9router + OmniRoute catalogs, normalises
// them into JSON snapshots under src/core/model/sources/.
//
// See scripts/sync/README.md for usage.

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "..", "..");
const OUT_DIR = join(REPO_ROOT, "src", "core", "model", "sources");
const CACHE_DIR = process.env.CIPHERROUTE_SYNC_CACHE || "/tmp/cipherroute-sync-cache";

const SOURCES = {
  "9router": {
    repo: "https://github.com/decolua/9router.git",
    defaultRef: "master",
    modulePath: "open-sse/config/providerModels.js",
    loader: load9router,
  },
  omniroute: {
    repo: "https://github.com/diegosouzapw/OmniRoute.git",
    defaultRef: "main",
    modulePath: "open-sse/config/providerRegistry.ts",
    loader: loadOmniroute,
  },
};

// OmniRoute splits provider metadata by family under
// src/shared/constants/providers/. This is the ONLY place free-tier
// information lives (`hasFree` = has a usable free tier, `noAuth` = keyless);
// the provider registry has no notion of "free". Paths are relative to that
// directory. A missing entry is skipped at runtime, so adding a new family
// upstream degrades gracefully instead of breaking the refresh.
const OMNIROUTE_META_FILES = [
  "apikey/gateways",
  "apikey/frontier-labs",
  "apikey/inference-hosts",
  "apikey/enterprise-cloud",
  "apikey/regional",
  "apikey/specialty-media",
  "noauth",
  "oauth",
  "web-cookie",
  "local",
  "search",
  "audio",
  "upstream-proxy",
  "cloud-agent",
  "system",
];

function parseArgs(argv) {
  const args = { only: null, refs: {}, srcs: {} };
  for (const raw of argv.slice(2)) {
    if (raw === "--help" || raw === "-h") {
      printHelp();
      process.exit(0);
    }
    if (raw.startsWith("--only=")) {
      args.only = raw.slice("--only=".length);
      continue;
    }
    const refMatch = raw.match(/^--ref-(9router|omniroute)=(.+)$/);
    if (refMatch) {
      args.refs[refMatch[1]] = refMatch[2];
      continue;
    }
    const srcMatch = raw.match(/^--src-(9router|omniroute)=(.+)$/);
    if (srcMatch) {
      args.srcs[srcMatch[1]] = srcMatch[2];
      continue;
    }
    console.error(`unknown argument: ${raw}`);
    process.exit(2);
  }
  if (args.only && !SOURCES[args.only]) {
    console.error(`--only=${args.only} not recognised (use 9router or omniroute)`);
    process.exit(2);
  }
  return args;
}

function printHelp() {
  const help = `Usage: node scripts/sync/normalize-sources.mjs [options]

Options:
  --only=<9router|omniroute>      Refresh only one snapshot
  --ref-9router=<ref>             Git ref for 9router (default: master)
  --ref-omniroute=<ref>           Git ref for OmniRoute (default: main)
  --src-9router=<path>            Use a local clone of 9router
  --src-omniroute=<path>          Use a local clone of OmniRoute
  -h, --help                      Show this help
`;
  process.stdout.write(help);
}

function ensureCache() {
  if (!existsSync(CACHE_DIR)) mkdirSync(CACHE_DIR, { recursive: true });
}

function ensureClone(name, repo, ref, override) {
  if (override) {
    if (!existsSync(override)) {
      throw new Error(`--src-${name}=${override} does not exist`);
    }
    console.log(`[${name}] using local clone at ${override}`);
    return override;
  }
  const dest = join(CACHE_DIR, name);
  if (!existsSync(dest)) {
    console.log(`[${name}] cloning ${repo} -> ${dest}`);
    execFileSync("git", ["clone", "--depth", "50", repo, dest], { stdio: "inherit" });
  } else {
    console.log(`[${name}] fetching latest in ${dest}`);
    execFileSync("git", ["-C", dest, "fetch", "--depth", "50", "origin", ref], { stdio: "inherit" });
  }
  execFileSync("git", ["-C", dest, "checkout", ref], { stdio: "inherit" });
  return dest;
}

function describeRef(dir) {
  try {
    const tag = execFileSync("git", ["-C", dir, "describe", "--tags", "--always"], {
      stdio: ["ignore", "pipe", "ignore"],
    })
      .toString()
      .trim();
    return tag || "HEAD";
  } catch (_e) {
    return "HEAD";
  }
}

function classifyKind(model) {
  if (model.type) {
    const t = String(model.type).toLowerCase();
    if (["embedding", "image", "tts", "stt", "search", "fetch", "video", "audio"].includes(t)) {
      return t;
    }
    if (t === "chat" || t === "llm") return "llm";
  }
  if (Array.isArray(model.capabilities) && model.capabilities.length > 0) {
    if (model.capabilities.some((c) => /image|text2img|edit/i.test(c))) return "image";
  }
  return "llm";
}

async function load9router(rootDir) {
  const ref = describeRef(rootDir);
  const modulePath = pathToFileURL(join(rootDir, "open-sse", "config", "providerModels.js")).href;
  const providersPath = pathToFileURL(join(rootDir, "open-sse", "config", "providers.js")).href;
  const mod = await import(modulePath);
  let providersMod = {};
  try {
    providersMod = await import(providersPath);
  } catch (_e) {
    // optional — providers.js holds base URL + format metadata, not strictly required.
  }
  const PROVIDER_MODELS = mod.PROVIDER_MODELS;
  const PROVIDER_ID_TO_ALIAS = mod.PROVIDER_ID_TO_ALIAS || {};
  const providersById = providersMod.PROVIDERS || {};
  const aliasToId = {};
  for (const [id, alias] of Object.entries(PROVIDER_ID_TO_ALIAS)) {
    aliasToId[alias] = id;
  }
  const providers = [];
  for (const alias of Object.keys(PROVIDER_MODELS).sort()) {
    const id = aliasToId[alias] || alias;
    const meta = providersById[id] || {};
    const rawModels = PROVIDER_MODELS[alias] || [];
    const models = rawModels.map((m) => ({
      id: m.id,
      name: m.name || m.id,
      kind: classifyKind(m),
      ...(m.contextLength ? { contextLength: m.contextLength } : {}),
      ...(m.maxOutputTokens ? { maxOutputTokens: m.maxOutputTokens } : {}),
    }));
    providers.push({
      id,
      alias,
      ...(meta.format ? { format: meta.format } : {}),
      ...(meta.authType ? { authType: meta.authType } : {}),
      ...(meta.baseUrl ? { baseUrl: meta.baseUrl } : {}),
      models,
    });
  }
  return {
    source: "9router",
    ref,
    generatedAt: new Date().toISOString(),
    providerIdToAlias: PROVIDER_ID_TO_ALIAS,
    providers,
  };
}

async function loadOmniroute(rootDir) {
  const ref = describeRef(rootDir);
  const moduleAbs = join(rootDir, "open-sse", "config", "providerRegistry.ts");
  const metaFiles = OMNIROUTE_META_FILES.map((rel) => join(rootDir, "src", "shared", "constants", "providers", rel));
  const result = spawnSync(
    "npx",
    ["--yes", "tsx@4", "-e", omnirouteLoaderSource(moduleAbs, metaFiles)],
    {
      stdio: ["ignore", "pipe", "inherit"],
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      // Must run with cwd inside the OmniRoute checkout: the registry and its
      // transitive imports use the `@/*` tsconfig path alias, and tsx only
      // resolves those against the nearest tsconfig.json. Running from our
      // repo root fails with "Cannot find module '@/shared/constants/...'".
      cwd: rootDir,
    },
  );
  if (result.status !== 0) {
    throw new Error(`omniroute loader failed (exit ${result.status})`);
  }
  const data = JSON.parse(result.stdout);
  const providers = [];
  for (const id of Object.keys(data.registry).sort()) {
    const entry = data.registry[id];
    const meta = data.meta[id] || {};
    const alias = entry.alias || id;
    const models = (entry.models || []).map((m) => ({
      id: m.id,
      name: m.name || m.id,
      kind: classifyKind(m),
      ...(m.contextLength ? { contextLength: m.contextLength } : {}),
      ...(m.maxOutputTokens ? { maxOutputTokens: m.maxOutputTokens } : {}),
    }));
    providers.push({
      id,
      alias,
      ...(entry.format ? { format: entry.format } : {}),
      ...(entry.authType ? { authType: entry.authType } : {}),
      ...(entry.baseUrl ? { baseUrl: entry.baseUrl } : {}),
      // Free-tier markers come from the metadata module, NOT the registry:
      // `hasFree` marks "this provider has a usable free tier", `noAuth`
      // marks keyless providers. Both are needed to answer "is this free?".
      ...(meta.hasFree ? { free: true } : {}),
      ...(meta.noAuth ? { noAuth: true } : {}),
      ...(Array.isArray(meta.serviceKinds) && meta.serviceKinds.length
        ? { serviceKinds: meta.serviceKinds }
        : {}),
      models,
    });
  }
  return {
    source: "omniroute",
    ref,
    generatedAt: new Date().toISOString(),
    providerIdToAlias: data.aliasMap || {},
    providers,
  };
}

function omnirouteLoaderSource(moduleAbs, metaFiles) {
  // String we hand to tsx. Loads providerRegistry (wire config: format,
  // authType, baseUrl, models) plus the 15 provider-metadata family modules,
  // then joins them by provider id so free-tier markers survive
  // normalisation. Free markers live ONLY in the metadata modules — the
  // registry has no notion of "free".
  //
  // We deliberately import the family modules directly instead of the
  // `constants/providers.ts` barrel: that barrel does not re-export the
  // provider records, and its own `FREE_PROVIDERS` export is an empty object.
  // Each family module is optional — a new/renamed family upstream degrades to
  // "no free information for that family" instead of failing the refresh.
  return `
import * as mod from ${JSON.stringify(moduleAbs)};
const reg = mod.REGISTRY || mod.PROVIDERS || {};
const aliasMap = (typeof mod.generateAliasMap === "function") ? mod.generateAliasMap() : {};

// Async IIFE, not top-level await: tsx emits CJS for -e, which rejects it.
(async () => {
  const metaFiles = ${JSON.stringify(metaFiles)};
  const meta = {};
  for (const file of metaFiles) {
    let family = {};
    try {
      family = await import(file);
    } catch (e) {
      process.stderr.write("[omniroute] metadata module skipped (" + file + "): " + (e && e.message) + "\\n");
      continue;
    }
    // Don't hardcode export names: iterate every export and keep the ones that
    // are records carrying a string \`id\`. This survives upstream renames of
    // APIKEY_PROVIDERS_GATEWAYS & friends. Sets and helper functions are
    // skipped naturally — they have no own enumerable record values.
    for (const exported of Object.values(family)) {
      if (!exported || typeof exported !== "object" || Array.isArray(exported)) continue;
      for (const entry of Object.values(exported)) {
        if (!entry || typeof entry !== "object" || typeof entry.id !== "string") continue;
        const prev = meta[entry.id] || {};
        meta[entry.id] = {
          hasFree: prev.hasFree || entry.hasFree === true,
          noAuth: prev.noAuth || entry.noAuth === true,
          serviceKinds: entry.serviceKinds || prev.serviceKinds,
        };
      }
    }
  }

  const out = { registry: {}, aliasMap, meta };
  for (const id of Object.keys(reg)) {
    const e = reg[id];
    if (!e || typeof e !== "object") continue;
    out.registry[id] = {
      alias: e.alias,
      format: e.format,
      authType: e.authType,
      baseUrl: e.baseUrl,
      models: Array.isArray(e.models) ? e.models : [],
    };
  }
  process.stdout.write(JSON.stringify(out));
})();
`;
}

async function main() {
  ensureCache();
  const args = parseArgs(process.argv);
  const targets = args.only ? [args.only] : Object.keys(SOURCES);

  for (const name of targets) {
    const def = SOURCES[name];
    const ref = args.refs[name] || def.defaultRef;
    const dir = ensureClone(name, def.repo, ref, args.srcs[name]);
    console.log(`[${name}] normalising at ${describeRef(dir)}`);
    const payload = await def.loader(dir);
    if (!existsSync(OUT_DIR)) mkdirSync(OUT_DIR, { recursive: true });
    const outPath = join(OUT_DIR, `${name}.json`);
    writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n", "utf8");
    const providers = payload.providers.length;
    const models = payload.providers.reduce((acc, p) => acc + p.models.length, 0);
    console.log(`[${name}] wrote ${outPath} (${providers} providers, ${models} models)`);
  }
}

main().catch((err) => {
  console.error(err.stack || String(err));
  process.exit(1);
});
