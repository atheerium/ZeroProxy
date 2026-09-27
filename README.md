# ZeroProxy

A lightweight, OpenAI-compatible LLM proxy that routes to **40+ providers** with format
translation, account fallback, token refresh, usage tracking, and SSE streaming.

Single Rust crate, single binary. Release build holds **~46 MB RSS**.

```bash
zeroproxy server start            # start detached; the dashboard auto-opens
# → dashboard on http://127.0.0.1:4623
```

---

## Contents

- [Why it exists](#why-it-exists)
- [Install](#install)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [API](#api)
- [CLI](#cli)
- [The four invariants](#the-four-invariants)
- [Provider catalog](#provider-catalog)
- [Development](#development)
- [Repository layout](#repository-layout)
- [License](#license)

---

## Why it exists

ZeroProxy is a **lightweight OmniRoute alternative aimed at free-tier LLM providers**. Providers
are sorted so the free tier comes first, and a "free only" filter hides the rest.

It is a fork of [`quangdang46/openproxy`](https://github.com/quangdang46/openproxy), itself a
Rust rewrite of [`decolua/9router`](https://github.com/decolua/9router). The active parity target
is [`diegosouzapw/OmniRoute`](https://github.com/diegosouzapw/OmniRoute), a TypeScript fork of
9router — we take its provider list, model catalog, and model conventions, and skip its heavier
subsystems (MCP, A2A/ACP, Electron/PWA, memory frameworks, cloud sync, Telegram).

Upstream catalogs are **imported, not hand-maintained**: `zeroproxy sync omniroute --free-only`
pulls OmniRoute's free-tier providers in as first-class models.

---

## Install

```bash
# From source (requires a Rust toolchain and a built dashboard)
cd web && pnpm install --frozen-lockfile && pnpm run build && cd ..
cargo build --release

# Run in place
./target/release/zeroproxy --web-dir ./web/dist server start
```

`cargo build` needs `web/dist` to exist — the `embed-web` feature bakes the dashboard into the
binary and `build.rs` panics without it. In release builds it also refuses to package a
`web/src` newer than `web/dist`.

---

## Quick start

```bash
zeroproxy server init             # create the data dir, print the first admin key
zeroproxy server start            # start detached; dashboard auto-opens
zeroproxy server status           # is it running?
```

Point any OpenAI-compatible client at the proxy:

```
base_url = http://127.0.0.1:4623/v1
api_key  = <your ZeroProxy key from the dashboard>
```

In an agentic harness, that is the whole integration. Both `/v1/...` and `/v1/v1/...` are served,
because harnesses differ on whether they append the path to a `base_url` that already ends in
`/v1` — **both spellings are registered on purpose and neither is a duplication bug.**

Check it end to end:

```bash
curl http://127.0.0.1:4623/health
curl http://127.0.0.1:4623/v1/models
```

---

## Configuration

| | |
|---|---|
| Port | `4623` (binds `127.0.0.1`) |
| Data dir | `~/.zeroproxy` |
| Database | `$DATA_DIR/zeroproxy.sqlite` (SQLite, WAL) |
| Logs | `~/.zeroproxy/zeroproxy.log` |

Environment variables actually read by the code:

| Variable | Purpose |
|---|---|
| `DATA_DIR` | Override the data directory (read far more widely than the namespaced form) |
| `ZEROPROXY_URL` | Proxy's own base URL, for generated client configs |
| `ZEROPROXY_API_KEY` | Admin/API key |
| `ZEROPROXY_PROFILE` | Config profile name |
| `ZEROPROXY_CONFIG` | Explicit config path |
| `ZEROPROXY_GIT_SHA`, `ZEROPROXY_BUILD_TIME` | Baked in at compile time, read-only |
| `--robot` / `--color` | Global flags: JSON envelope output, colour preference |

`CIPHERROUTE_*` variables from the pre-rename era are **still read** for per-provider OAuth
endpoints. They are not deprecated aliases you can drop — see [Legacy names](#legacy-names).

### Legacy names

The project was renamed twice: `openproxy` → `cipherroute` → `zeroproxy`. Two things were
intentionally **not** renamed:

- **`hasCipherRoute`** is a live API field, emitted by 18 backend sites and read by 11 dashboard
  components. It is an identifier, not brand text.
- **`CIPHERROUTE_*` env vars** are still the names production reads for most provider OAuth
  configuration. Production itself is mid-migration: `ZEROPROXY_*` and `CIPHERROUTE_*` are both
  live, for different variables.

---

## API

### Inference — OpenAI-compatible

```
/v1/chat/completions        /v1/messages          /v1/responses
/v1/embeddings              /v1/models            /v1/health
/v1/audio/speech            /v1/audio/transcriptions   /v1/audio/music
/v1/audio/voices            /v1/images/generations     /v1/images/edits
```

`/v1/messages` is the Anthropic-shaped surface; request and response bodies are translated
between formats automatically, so a Claude client and an OpenAI client can both point here.

### Management

```
/api/health        /api/version      /api/settings     /api/db/export
/api/providers     /api/nodes        /api/keys         /api/combos
/api/models        /api/proxy-pools  /api/cache/stats  /api/catalog
/api/observability/logs             /api/usage/*      /api/quota/*
/api/cli-tools/*                    /metrics         (Prometheus, at the root)
```

All payloads are versioned under the **`zeroproxy.v1`** envelope namespace, which is **frozen and
additive-only** across 13 resources. A breaking change requires a new namespace, not a silent
field rename.

`/api/version` also reports build and repository freshness, which is what the dashboard navbar
badge reads. A green `/api/health` does **not** prove the dashboard is being served from disk —
assert `--web-dir` is in the serving process's argv if you are debugging a stale UI.

---

## CLI

One flat namespace, 23 top-level subcommands:

```
provider   key      pool     combo    models   tunnel   mitm     tool
translator media    route    completion  schema  doctor  auth
usage      logs     quota    chat     settings db      sync     server
```

`server` carries the daemon lifecycle (`start` / `stop` / `status` / `init`); the rest take their
own subcommands — `provider list`, `combo create`, `auth login`, and so on. Most read from the same
SQLite store the server uses, so the CLI works while the server is down:

```bash
zeroproxy provider list
zeroproxy combo create --name coding \
  --models "groq/llama-3.3-70b,openrouter/gpt-4o-mini" \
  --strategy fallback          # or round-robin / sticky-round-robin
zeroproxy sync omniroute --free-only --dry-run
zeroproxy schema show custom-model        # inspect the frozen envelope
zeroproxy db export --out backup.json     # --scopes apiKeys,combos to narrow
```

`--robot` emits the same `zeroproxy.v1` envelope as JSON instead of human text, for scripting.

---

## The four invariants

These are load-bearing. Changing one without the others has broken the router before.

1. **Capability filter runs before routing.** `HARD_CAPS = ["vision", "pdf", "audioInput",
   "videoInput"]` is defined in **two** places — `src/core/combo/mod.rs` and
   `src/core/combo/capabilities.rs`. A hard-cap mismatch **skips the model**; it is not a
   fallback trigger.
2. **`context_window` caps history.** Only for models added by the capacity adapter, budget
   `(context_window || 200_000) * 0.8 * 4`.
3. **Fallback happens only on eligible errors**, via one classifier. A `404` is a **300 s model
   lock**, not an auth failure, and a `retryAfter` header beats any body message.
4. **Error classification has exactly one source** — `error_config::classify_error`. Never
   re-implement status→fallback logic inside an executor.

---

## Provider catalog

The addable-provider list is **generated from the OmniRoute snapshot**, not hand-written:

```bash
node scripts/sync/normalize-sources.mjs --only=omniroute --src-omniroute=<clone>
node scripts/sync/generate-web-providers.mjs --check   # exits 1 if the committed file is stale
```

**326 addable providers**: 130 curated by hand, plus 196 generated from the snapshot (93 free-tier,
103 other). Free-tier entries sort into a section that sits **above** API Key, so free comes first
structurally rather than by a sort key.

Models arrive two ways:

- **Static lists** from the snapshot, for providers that publish one.
- **Live discovery** for the 96 providers whose registry entry records a `modelsUrl`. The fetch is
  lazy — it happens when someone opens that provider's model list, never on a timer, and costs one
  outbound request. Idle costs nothing.

Three different numbers are all correct and none of them is "the catalog":

| Number | What it counts |
|---|---|
| ~21 | Configured provider **connections** on the Providers page |
| **326** | The addable provider **catalogue** |
| 270 | Providers present in the **snapshot** |

The sync is **model-level, not provider-level**: it writes models, never a provider you can add.

---

## Development

```bash
./scripts/dev.sh                    # build + run (foreground)
./scripts/dev.sh --fast detach      # rebuild and restart in the background (~10-20 s)
./scripts/dev.sh --web-only         # dashboard changed; no cargo
./scripts/dev.sh --backend-only     # Rust changed; no pnpm
./scripts/dev.sh --full detach      # pre-push gate: fmt + clippy + astro + tests
./scripts/dev.sh --check            # lint only
PORT=4624 ./scripts/dev.sh detach   # second instance without touching 4623
```

The gate is `cargo test --lib --all-features` — **1917 passing, 0 failing** on a clean tree.
Integration tests under `tests/` compile but are excluded from CI.

In `--web-dir` mode (what `dev.sh` uses) the dashboard is read from disk per request, so
`pnpm run build` alone makes UI changes visible. In release mode it is embedded at compile time.

**Do not use bare `pkill`, `nohup`, or a hand-rolled start.** A systemd user unit respawns the
server and steals the port; `dev.sh` and `restart.sh` stop it first.

Type-checking is separate from building — the dashboard build does **not** run `tsc`:

```bash
cd web && pnpm exec tsc --noEmit -p tsconfig.json    # must run from web/, not the repo root
```

---

## Repository layout

```
src/
  core/       domain: model catalog, combo routing, executors, translator, oauth, rtk
  server/     axum HTTP + dashboard embedding
  cli/        clap CLI and --robot JSON envelopes
  db/         SQLite (WAL) + AES-GCM encrypted credential columns
  oauth/      token refresh flows
web/          Astro 5 + React 19 dashboard, built to web/dist
scripts/      dev loop and the upstream catalog sync
tests/        integration tests (compile; excluded from CI)
```

Start with **`AGENTS.md`** — it is the operational memory for this repo and records the traps that
have each produced a wrong conclusion at least once. `docs/ARCHITECTURE.md` is the structural map;
`CONTRIBUTING.md` covers git conventions.

### Where things are

| Concern | Location |
|---|---|
| Provider base URLs | `src/core/executor/default.rs` → `PROVIDER_CONFIGS` |
| Built-in model catalog | `src/core/model/provider_catalog.json` (embedded at compile time) |
| Upstream snapshots | `src/core/model/sources/{omniroute,9router}.json` |
| Addable provider list | `web/src/shared/constants/providers.generated.ts` (**generated** — edit the generator) |
| Routing / fallback | `src/core/combo/` |
| Error classification | `src/core/config/error_config.rs` |

---

## License

Intended as **MIT**.

Two gaps to be aware of: **no `LICENSE` file is present in the repository**, and `Cargo.toml` has
no `license` field, so the crates.io metadata is incomplete. Both are outstanding, not
deliberate — the previous README linked to a `LICENSE` file that does not exist.

---

## Acknowledgments

Built on the work of [9router](https://github.com/decolua/9router),
[OmniRoute](https://github.com/diegosouzapw/OmniRoute) (provider ecosystem and model conventions),
[openproxy](https://github.com/quangdang46/openproxy) (the Rust rewrite this forks), and
[freellmapi](https://github.com/tashfeenahmed/freellmapi) (zero-config setup philosophy).
