# ZeroProxy Architecture

Single Rust crate `zeroproxy` (`src/`) serving both an OpenAI-compatible proxy API
and the Astro dashboard (`web/`). One process, one port.

> Renamed `openproxy` → `cipherroute` → `zeroproxy` over time. Grep for the old
> names and you will find deliberate legacy hits: `hasCipherRoute` is a **live API
> field** (18 emit sites in `src/`, 11 consumers in `web/`) and `CIPHERROUTE_*`
> env vars are still read by production. Do not "fix" either.

## Where to look for what

The crate is ~197k lines across 370 files. Two directories dominate and are the
places to start reading.

| Directory | Files | Lines | What lives here |
|---|---:|---:|---|
| `src/server/api/` | 50 | 50,630 | every HTTP route; the single densest area |
| `src/core/executor/` | 36 | 29,730 | `ProviderExecutor` impls, one per provider family |
| `src/core/combo/` | 10 | 6,012 | routing: capability filter, ordering, fallback |
| `src/core/rtk/` | 8 | 5,900 | request/response compression |
| `src/core/translator/` | 6 | 5,426 | format translation registry |
| `src/core/usage/` | 6 | 5,052 | usage tracking, pricing, quota fetchers |
| `src/core/utils/` | 14 | 3,920 | shared helpers |
| `src/cli/` | — | — | clap CLI, `--robot` JSON envelopes |
| `src/db/` | — | — | SQLite WAL persistence, migrations, encrypted columns |
| `src/oauth/` | — | — | token refresh flows per provider |
| `src/types/` | — | — | shared data types |

### The twelve largest files — the complexity hot spots

`oauth.rs` 5,983 · `chat.rs` 5,742 · `cursor.rs` 3,570 · `quota_fetcher.rs` 3,500 ·
`cli_tools.rs` 3,273 · `usage.rs` 3,133 · `api/mod.rs` 2,918 · `compat.rs` 2,658 ·
`grok_web.rs` 2,575 · `provider_models.rs` 2,535 · `executor/default.rs` 2,092 ·
`cli/mod.rs` 2,088.

Anything over 2k lines mixes several concerns. When you touch one, expect to
need a test that pins the behaviour *before* changing it — these are the files
where a confident-looking refactor silently breaks a provider.

### Core subsystems the top of this file used to omit

`core/config/` (error classification, the single source of status→action) ·
`core/auth/` (credential manager) · `core/model/` (provider catalog + the
`sources/*.json` snapshots) · `core/account_fallback/` (per-account failover) ·
`core/health/` (health daemon) · `core/auto/` (auto combo presets) ·
`core/cache/` · `core/performance/` · `core/guardrails/` · `core/mitm/` ·
`core/dns/` (SSRF helpers) · `core/compress/` · `core/proxy/` · `core/eval/` ·
`server/auth/` (API-key + admin auth) · `server/dashboard/` (embedded assets).

## Request flow

```
client → server/api/chat.rs  (forward_with_provider_fallback)
  → format detection            core/chat/mod.rs:resolve_transport
  → combo resolution + ordering core/combo/mod.rs
      · capability filter runs BEFORE ordering and skips a model outright
      · errors classified by core/config/error_config.rs:classify_error
  → provider execution          core/executor/<provider>.rs
  → response translation        core/translator/
  → SSE or JSON back to client
  → usage recorded             core/usage/tracker.rs → types::UsageDb.history
```

Four invariants hold this together. They are stated in `AGENTS.md` and each is
enforced at the code noted:

1. **Capability filter before routing** — `HARD_CAPS` is defined **twice**, in
   `core/combo/mod.rs` and `core/combo/capabilities.rs`. Change both or they
   desync.
2. **`context_window` cap** — history trimmed by
   `core/combo/capacity_adapter.rs:strip_history_for_context`.
3. **Fallback only on eligible errors** — `check_fallback_error` →
   `classify_error` → `ErrorClassification`. A 404 is a 300 s model lock, *not*
   an auth failure.
4. **One source of error classification** — never re-implement status→action
   inside an executor.

## Routes

OpenAI-compatible inference is registered **twice**, and both spellings work:

- `/v1/models`, `/v1/chat/completions`, `/v1/messages`, `/v1/responses` — via a
  `/v1` nest
- `/v1/v1/…` — the same handlers, for clients that append `/v1` to a base URL
  that already ends in `/v1`

The doubled prefix is a **deliberate compatibility layer, not a bug.** Do not
"deduplicate" it; some harnesses emit `base_url` with `/v1` already present and
then append the standard path. Verified against the live server: both return 200
on `/v1/models`.

Dashboard and admin JSON live under `/api/*`. Health is at `/health` and
`/v1/health`.

## Dashboard

`web/` is Astro 5 + React 19, built to `web/dist/` and served by the binary.
In `--web-dir` mode (what `scripts/dev.sh` always uses) `web/dist` is read from
disk per request, so `pnpm run build` alone changes the UI — no cargo rebuild.
`web/dist` is gitignored and `build.rs` **panics** without it.

Model lists reach the UI through one chain, and all of these must agree:
`sync` writes custom models → `CustomModel.extra` (flattened, so markers arrive
as **top-level** JSON keys) → `web/src/shared/utils/providerCustomModels.ts` →
`web/src/shared/models/availableModels.ts` → the provider page and
`ModelSelectModal.tsx`.

## Key data types

| Type | File | Purpose |
|---|---|---|
| `UsageEntry` | `src/types/mod.rs` | one request log row |
| `UsageDb` | `src/types/mod.rs` | request history + daily summaries |
| `CustomModel` | `src/types/mod.rs` | a synced or user-added model; `extra` is `#[serde(flatten)]` |
| `AppDb` | `src/types/mod.rs` | the whole persisted document |
| `ProviderConfig` | `src/core/executor/default.rs` | provider endpoint + auth config; `PROVIDER_CONFIGS` is the base-URL source of truth |
| `ComboStrategy` | `src/core/combo/mod.rs` | round-robin / fallback / balanced |
| `SourceProvider` | `src/cli/sync.rs` | one upstream catalog entry as read from a snapshot |

Custom models are stored in the `kv` table keyed **`{provider_alias}/{model_id}`**.
Model ids are only unique *within* a provider, so a bare-id key silently
overwrites across providers. See Trap 8 in `AGENTS.md`.

## Static data embedded in the binary

`core/model/provider_catalog.json` (168 KB) and `core/model/sources/*.json`
(134 KB + 427 KB) are `include_str!`-embedded, so a change to them needs a
**cargo rebuild**, not just a restart.

## Further reading

`AGENTS.md` is the operational memory: mission, invariants, and a numbered list
of verification traps that have each produced a wrong conclusion at least once.
Read it before planning work. `docs/git-conventions.md` covers the commit and
branch rules.
