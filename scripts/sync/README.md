# Sync snapshot generator

These scripts produce the JSON snapshots that the `zeroproxy sync` command
applies to the user's `db.json`. They are **maintainer-only** tooling — end
users never need to run them. The committed snapshots live at
`src/core/model/sources/{9router,omniroute}.json` and are embedded into the
release binary via `include_str!`, so the CLI works fully offline.

## Why a Node helper

Both upstreams ship their catalogs as JS / TS source modules with imports
and helper functions. Parsing those reliably from Rust is brittle; running
them as the actual modules from Node is trivial. The helper:

1. Shallow-clones (or refreshes) `decolua/9router` and `diegosouzapw/OmniRoute`
   into `/tmp/zeroproxy-sync-cache/` (override with `ZEROPROXY_SYNC_CACHE`).
2. Dynamically imports each catalog module (`open-sse/config/providerModels.js`
   for 9router, `open-sse/config/providerRegistry.ts` for OmniRoute — the
   latter via `tsx`).
3. Normalises both into the same shape (`{source, ref, providers: [...]}`).
   For OmniRoute it also joins the registry against the 15 family metadata
   modules under `src/shared/constants/providers/`, because the registry
   itself has no notion of which providers are free — `hasFree` / `noAuth`
   live only in the metadata. That join is what makes `--free-only` possible.
4. Writes `src/core/model/sources/<source>.json`.

## Usage

```bash
# Refresh both snapshots (clones the upstreams to /tmp on first run):
node scripts/sync/normalize-sources.mjs

# Refresh only one:
node scripts/sync/normalize-sources.mjs --only=9router
node scripts/sync/normalize-sources.mjs --only=omniroute

# Pin a specific ref (defaults: 9router=master, omniroute=main):
node scripts/sync/normalize-sources.mjs --ref-9router=v0.4.55 --ref-omniroute=v3.8.0

# Use a local clone instead of cloning fresh:
node scripts/sync/normalize-sources.mjs \
  --src-9router=/path/to/9router \
  --src-omniroute=/path/to/OmniRoute
```

After running, commit the updated `src/core/model/sources/*.json` files
alongside any Rust code changes. The runtime `zeroproxy sync` command then
applies them to `db.json` against the user's machine.

## Schema

```jsonc
{
  "source": "9router" | "omniroute",
  "ref": "v0.4.55",                 // git ref or "HEAD"
  "generatedAt": "2026-05-19T...",
  "providerIdToAlias": {            // map provider id -> zeroproxy alias
    "openai": "openai",
    "github-models": "ghm"
  },
  "providers": [
    {
      "id": "openai",
      "alias": "openai",
      "format": "openai",           // present iff upstream exposes it
      "authType": "apikey",
      "baseUrl": "...",
      "free": true,                  // OmniRoute only: upstream hasFree
      "noAuth": true,                // OmniRoute only: needs no credential
      "serviceKinds": ["llm"],       // OmniRoute only, sparse upstream
      "models": [
        { "id": "gpt-5.5", "name": "GPT-5.5", "kind": "llm", "contextLength": 1050000 }
      ]
    }
  ]
}
```

The `kind` field follows zeroproxy's existing classification:
`llm` | `embedding` | `image` | `tts` | `stt` | `search` | `fetch` | `video`.

## Free tier

`zeroproxy sync omniroute --free-only` imports only providers upstream marks
`hasFree` or `noAuth`. A keyless provider costs nothing, so it counts as free
here too. Without the flag the sync imports every provider, paid included,
which is not what this project is for.

Two caveats when reading the snapshot:

- `serviceKinds` is populated for only a small minority of providers upstream,
  and `kind` comes back `"llm"` for essentially every model. Neither can
  narrow the list to LLM providers specifically, so there is deliberately no
  `--llm-only`; the free list already contains almost nothing but LLM
  providers.
- Because free providers frequently ship no static model list, a free-only
  sync will bring in providers that contribute the provider but no models.
  That is expected — the models appear via live discovery or a later sync.
