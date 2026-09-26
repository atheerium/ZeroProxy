# ZeroProxy — Rust AI Proxy Router

## Mission — read this first, it is not up for renegotiation

> **ZeroProxy is "a lightweight OmniRoute Rust alternative for all free-tier LLM providers."**

That one phrase is the whole product. Everything below is subordinate to it.

**The goal is OmniRoute. The 9router/openproxy era is over.** We already replicated almost every
feature from 9router and quangdang46/openproxy, and both of those repos update slowly, so chasing
their release notes has low return. OmniRoute is the only upstream that still moves fast enough to
be worth mirroring, so:

- **Parity target = OmniRoute's FREE-TIER provider list + model catalog + model conventions.**
- **Free tier only. Paid providers are explicitly out of scope.** Do not add them, do not research
  them, do not report their absence as a gap.
- **Lightweight is a hard constraint, not a nice-to-have.** If a port drags in MCP, A2A/ACP,
  Electron/PWA/VNC, memory/skills frameworks, cloud sync, Telegram, or chaos engineering, it is
  the wrong port. See the "do NOT port" list.
- **Only free-tier LLM providers are real targets.** Search/fetch/TTS/embedding/image providers are
  lower value — do not let them crowd out LLM work.

**Do not ask the user to restate these goals.** They are recorded here permanently. If a task seems
to conflict with this section, this section wins, and you should say so rather than quietly
re-scoping.

## This repo is written by an AI agent — AGENTS.md is the only memory

**The maintainer does not hold project state in their head and does not want to be asked for it.**
Treat this file as the sole source of truth for anything you were not told in this conversation:

- Anything learned, decided, corrected, or measured goes **here**, not into a chat reply that will
  scroll away. "The user said it once" is not storage.
- If you discover the maintainer misremembers something, correct the record here rather than
  silently working around it.
- The Mission section above is permanent. Everything else may be superseded — check `git log` on
  this file when state looks stale.

## Current state (2026-09-26) — read before planning more work

**The free-tier goal is met and PR #14 is MERGED into `main` (merge commit `c9d72419`, 2026-09-26).**

- PR #14 (https://github.com/atheerium/ZeroProxy/pull/14) was **merged with a merge commit** (not
  squash), so `git log main` shows the individual commits. It carried 25 commits relative to the old
  local `main`, and 35 relative to `origin/main` — because the old local `main` had **10 unpushed
  commits** of its own which the branch also contained. Nothing was lost; verify with
  `git rev-list --count c9d72419^..HEAD` (36 = merge + 25 + 10).
- The merge was deliberately held back while CI was red, then unblocked. The 40 test failures were
  **not** "latent stale-test debt" as first recorded — the broken test build had been *hiding two
  live production bugs*:
  - `TtftConfig::from_value` read the transposed keys `"tthtTimeoutMs"` / `"tthtQuarantineSecs"`,
    so **every per-combo TTFT knob was dead** and always used the 5000 ms default. (`ttftAdaptive`
    was spelled correctly, so adaptive mode worked while both scalar knobs did not.)
  - `error_config::classify_error` let a status-only rule match *before* the "upstream returned"
    carve-out, so **a request that failed to translate took the provider out of rotation for
    120 s** instead of falling through. This is OmniRoute PR #14830; ours had the bug.
- Delivered: `zeroproxy sync omniroute --free-only` imports OmniRoute's free-tier providers as
  first-class models (270 providers in the snapshot, 137 free), each carrying a `providerFreeTier`
  marker that the dashboard renders as a **FREE** badge. Live db currently shows ~880 free models
  across ~105 providers. Run it with `zeroproxy sync omniroute --free-only`.
- The navbar badge now tells you **which version you are actually running**. It previously showed
  the backend's sha and process uptime (`0m`), which made a stale dashboard look freshly restarted.
  It now reports the dashboard's own build identity against repo HEAD as fresh / stale / unknown,
  names the stale layer, and gives the exact remedy command (Trap 9).
- Two supporting fixes the feature depends on: the snapshot normalizer had been **unrunnable since
  2026-07-02** (missing `cwd` on `spawnSync` made a tsconfig alias resolve against our repo), and
  custom models are now keyed `alias/model-id` instead of bare `model-id` (Trap 8).

**Three claims that were wrong and are corrected here so they are not re-derived:**

1. **The pre-push gate was not gating until `43afa7cc`.** `run_checks` was guarded by
   `MODE == "build"` while `build` runs for `MODE=detach` too, so `--full detach` — the gate
   AGENTS.md documents — ran *zero* checks and still printed success. Any "gate passed" statement
   made before that commit was hollow. It is fixed and now genuinely runs fmt+clippy+astro+tests.
2. **The correct synced-row count is 905, not 999.** 94 of the 999 free models are already
   built-in, so 905 synced + 94 built-in = 999 covered. The pre-fix loss was **132** models, not
   the 226 first measured.
3. **The live db was not empty.** It held 548 `customModels` rows (529 from the background
   `auto_sync` models.dev daemon, 11 user-created, 8 imported), already rekeyed by the Trap 8
   migration. Do not assume a clean slate.

**The one decision still open: whether to cherry-pick upstream's 169 substantive commits.** The
free-tier + freshness work is merged and done. Upstream (`quangdang46/openproxy`, our own `upstream`
remote) has 142 commits touching `src/` that carry the full `openproxy::` → `zeroproxy::` rename
cost — see Trap 7 for the measured numbers; 27 are `src/`-free and cheap. Recommendation on record:
cherry-pick the `src/`-free tier, do **not** merge the branch. The custom-model toggle question is
**settled** — see below.

**Settled: custom models stay permanently un-disable-able — they are the escape hatch.** The
maintainer's call, in response to being offered the change. Do **not** port upstream's
`e5db61ab fix(web): honour disabled state for custom models end-to-end`, and do not "complete" the
half-implemented state: `availableModels.ts:165` computes `disabled` for custom rows and
`ProviderDetailPageClient.tsx:1174` filters on it, but nothing can set it, and that is the intent.
`ModelRow.tsx:113` gates the disable button on `!isCustom` deliberately.

**Why, so this is not re-litigated:** when a provider's whole catalog is disabled — quota exhausted,
an outage, a bad model — a synced custom model is the user's only way through. Making it
disable-able would let a bulk action lock them out of their own router. An escape hatch that can be
disabled by the same bulk operation you use to disable everything else is not an escape hatch.
Consequence to respect: a custom model can never be hidden by the Available Models toggle, so
"disable everything" will still leave custom models selectable. That is correct, not a bug — if you
need it gone, delete the model.

## Verification traps — each of these produced a wrong conclusion at least once

- **`git grep -c` counts matching LINES, not occurrences.** Use `git grep -o <pat> <ref> | wc -l`.
  Using `-c` reported 86/95 crate refs where the real numbers were 152/143.
- **`git show --stat` prints the path BEFORE the pipe.** Classify with
  `git show --name-only | grep -c '^src/'`, never `grep '| src/'` — the latter matches nothing
  and silently classified all 171 commits as `src/`-free.
- **`cargo clippy` aborts at the first failing integration target.** Truncated output undercounts
  badly. Iterate until clean; never trust a single run's error list.
- **`pnpm build` does not type-check** (Astro skips `tsc`) and the repo has ~507 pre-existing tsc
  errors. Capture the error list, `git stash`, re-run, and `comm` the two sets — comparing totals
  hides offsetting changes.
- **SQLite JSON booleans:** use `json_type(value,'$.k')='true'`. `json_extract(...)='true'` never
  matches because it returns integer `1`.
- **Back up a live db with `sqlite3 "$DB" ".backup '$OUT'"`, never `cp`** — the server holds it
  open and `cp` risks a torn copy.
- **`pgrep -f 'zeroproxy.*4999'` self-matches the invoking shell** and hangs. Use `pgrep -x zeroproxy`.
- **Grep web bundles for a distinctive literal, not a shared utility class** — `bg-green-500/10`
  matches an unrelated "Connected" chip; `FREE` is the distinctive one.
- **The server caches the db in memory.** A sync that writes sqlite out-of-process is invisible to
  the API until `./scripts/restart.sh` (DB-only change ⇒ no cargo rebuild needed).
- **A Vite `define` that fails to substitute is invisible to every gate.** `pnpm build` succeeds,
  vitest passes, and `tsc` is clean — then the browser throws `__UI_GIT_SHA__ is not defined` at
  runtime. Prove substitution by grepping the *built* bundle for a bare `__UI_*` identifier:
  `grep -rIlE '(^|[^A-Za-z0-9_$])__UI_(BUILT_AT|GIT_SHA|COMMIT_TIME)__' web/dist/_astro/` must
  print nothing. The navbar badge reads these (see Trap 9).
- **`tsc` total is not proof; a per-file filter is.** A total equal to the 507 baseline can hide
  offsetting errors. `pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep <file>` proving *zero*
  errors in the files you touched is the real check.

## Project lineage — all four repos are one family

```
9router (decolua/9router) — JavaScript/Next.js, the ORIGINAL
   ├── OmniRoute   (diegosouzapw/OmniRoute)   — TypeScript fork of 9router + CLIProxyAPI port
   └── openproxy   (quangdang46/openproxy)    — Rust REWRITE fork of 9router
          └── ZeroProxy (atheerium/ZeroProxy) — our fork of openproxy; renamed `zeroproxy`
```

Verified, not assumed: OmniRoute's own README states it *"started as a fork of 9router and a
TypeScript port of the Go project CLIProxyAPI"*, and its credits table calls 9router *"The
original project this fork is built on"*. **openproxy's** Rust rewrite of 9router is founder-stated.

**Consequences for agents — this is why "the same stuff" keeps showing up:**

- The repos share a directory layout: 9router and OmniRoute **both** have `open-sse/config/`.
  Our own `scripts/sync/normalize-sources.mjs` reads `open-sse/config/providerModels.js` from
  9router and `open-sse/config/providerRegistry.ts` from OmniRoute — near-identical shapes.
- Concepts invented in 9router propagate to all descendants: `transports[]` multi-endpoint tables
  (we mirror them in `src/core/chat/mod.rs:provider_transports`), `PROVIDER_ID_TO_ALIAS`,
  `providerModels` catalogs, combo/fallback semantics, the `*-free` zero-priced tier convention.
- **When something looks inexplicably familiar in 9router, OmniRoute, and here at once, it is
  shared ancestry, not coincidence.** Diffing one against the other is usually more informative
  than reading our own code alone.
- A fix in 9router may already exist in OmniRoute and vice versa. Check both before implementing.

## Why this fork exists

openproxy was chosen as the base **because it is Rust** — the goal is an efficient, lightweight
LLM proxy. ZeroProxy's purpose is to stay lightweight while moving **closer to OmniRoute**
(feature parity on catalogs/providers/capabilities), *without* inheriting OmniRoute's bloat
(MCP, A2A/ACP, Electron/PWA, memory frameworks, cloud sync, Telegram, chaos engineering).

**Port from OmniRoute. Do not port its non-essential subsystems.** See "do NOT port" below.

## Repositories to refer to

Look here before reinventing anything, in this order. OmniRoute is first because it is the active
parity target; 9router and openproxy are demoted to reference-only since their feature parity work
is effectively done.

| Order | Repo | Why |
|---|---|---|
| 1 | https://github.com/diegosouzapw/OmniRoute | **The parity target.** Free-tier providers, model catalog, model conventions. Diff against this first. |
| 2 | https://github.com/quangdang46/openproxy | Our direct parent; 259 commits ahead of our `main`. Reference for Rust idioms only — see the rename trap. |
| 3 | https://github.com/decolua/9router | Root of the family; where shared conventions originate. Parity essentially achieved. |
| 4 | https://github.com/tashfeenahmed/freellmapi | Occasionally useful, unmaintained |

**`quangdang46/openproxy` is our `upstream` git remote, not a third-party reference.** Its recent
PRs are largely authored by `atheerium` (us). "Porting a feature from it" means *reconcile with our
own upstream* — and note that merging it is currently **blocked**, see the rename trap below.

## 7 Questions Every Agent Asks

| Q | A | Deep-dive |
|---|---|---|
| What is this binary | OpenAI-compatible Rust proxy routing to 40+ providers: format translation, fallback, OAuth refresh, usage tracking, SSE | [ARCHITECTURE.md](docs/ARCHITECTURE.md) (tracked) |
| How do I build/test | `./scripts/dev.sh --fast detach` then `curl http://127.0.0.1:4623/health` | [Dev workflow](#dev-workflow) · `CONTRIBUTING.md` |
| How do I add a provider | `src/core/model/provider_catalog.json` + executor in `src/core/executor/default.rs`; check OmniRoute first | [PROVIDERS.md](docs/PROVIDERS.md) — **local-only, see [Docs trap](#docs-trap)** |
| How does routing work | Parse model → detect capabilities → capability-aware order → round-robin/fallback → executor | `src/core/combo/` |
| Where does a combo come from | `src/core/combo/mod.rs` — ordered `provider/model` pairs scored by `ComboStrategy` | [ROUTING.md](docs/ROUTING.md) — **local-only** |
| How is auth refreshed | 401/403 → single `dispatch_oauth_refresh`; 404 → 300 s model-specific lock (not a refresh) | `src/oauth/token_refresh.rs` |
| How is usage tracked | `src/core/usage/tracker.rs` → SQLite; SSE `/api/usage/stream` | `src/server/api/usage.rs` |

## Traps (verified — these bite agents)

### 1. `cargo test -p cipherroute` does not exist
Single crate, **no cargo workspace**; the package is `zeroproxy`. `-p cipherroute` fails with
*package ID specification did not match any packages*. Two committed files still document the
broken form — `scripts/parity-smoke.sh` and `tests/parity/README.md`. Run filters directly:

```bash
cargo test --lib parity_tests     # or stream_flags | combo | error_config | chat::
cargo test -p zeroproxy --lib provider_models   # what dev.sh --check runs
```

### 2. `web/dist` is gitignored → fresh clone cannot `cargo build`
The `embed-web` feature is **default-on**, and `build.rs` *panics* if `web/dist/index.html` is
missing. Always build the dashboard first:

```bash
cd web && pnpm install --frozen-lockfile && pnpm run build
```

In **release** builds `build.rs` additionally panics if `web/src` is newer than `web/dist` — a
deliberate guard against shipping a stale dashboard. Escape hatch:
`cargo build --release --no-default-features`.

### 3. Two `HARD_CAPS` definitions
`src/core/combo/mod.rs:438` and `src/core/combo/capabilities.rs:20`. Changing one without the
other desyncs the capability gate from the fallback path.

### 4. `zeroproxy.service` respawns and steals the port
The systemd user unit has `Restart=always`; killing the process alone lets it return in ~5 s and
the next start dies with `EADDRINUSE`. `dev.sh`/`restart.sh` stop the unit first. **Never** use
bare `pkill`, `nohup`, or a hand-rolled start. `start_server.sh` is deleted on purpose.

### 5. Docs trap — most of `docs/` is gitignored
`.gitignore` has `docs/*` with only these allow-listed (verified via `git ls-files docs/`):
`ARCHITECTURE.md`, `agent-orchestration.md`, `git-conventions.md`, `parity-9router.md`,
`parity-9router-FULL.md`, `parity-9router-impl.md`, `residual-gaps.md`.

`ROUTING.md`, `PROVIDERS.md`, `TRAPS.md`, `STATE.md`, `OMNIROUTE_PROVIDER_PARITY.md`, `ADRs/`
exist **only on this machine**. Don't cite them as repo truth, and don't assume a fresh clone
has them. `docs/STATE.md` is also a stale session log from a merged branch — not current state.

### 6. Renamed over time — grep will mislead
Binary `zeroproxy` (was `cipherroute`, was `openproxy`); schema namespace `zeroproxy.v1` (was
`cipherroute.v1`); data dir `~/.zeroproxy` (was `~/.cipherroute`). Legacy names survive in
`pkill` patterns and comments by design. Check `Cargo.toml` `name` before trusting a doc.

### 7. Merging `upstream` is blocked by a one-sided rename — do NOT attempt it casually
`upstream/main` (`quangdang46/openproxy`) is **259 commits ahead** of us, but the branch is
**diverged** (90 commits unique to us). A test-merge yields **49 conflicted files**
(`src/server/api` ×13, `src/core/executor` ×7, `tests` ×5, plus `src/db/sqlite`, `src/core/model`,
`src/core/auth`, `web/src/components/providers`).

The real blocker is not the conflicts — it is that **we renamed the crate and upstream did not**:

| | ours | upstream |
|---|---|---|
| `Cargo.toml` `[package] name` | `zeroproxy` v0.3.1 | `openproxy` v0.3.0 |
| crate refs in `src/` | 152 × `zeroproxy::` / 0 `openproxy::` | 0 × `zeroproxy::` / 143 × `openproxy::` |
| crate refs, whole repo | 567 × `zeroproxy::` / 2 × `openproxy::` | 0 × `zeroproxy::` / 700 × `openproxy::` |
| schema namespace, whole repo | 254 × `zeroproxy.v1` (frozen, 13 resources) | 0 × `zeroproxy.v1` / 264 × `openproxy.v1` |

The two trees are **mutually exclusive** in brand spelling — there is no overlap to reconcile,
so every one of the 143 `openproxy::` paths and 264 `openproxy.v1` emit sites must be rewritten.
Count with `git grep -o <pat> <ref> | wc -l`, **not** `git grep -c` (that counts matching
*lines*, which undercounts a lot).

Our rename (`bf807c8a`, 2026-08-29) landed **after** the merge-base (`6eec13d8`, 2026-08-25).
Both sides also genuinely changed the same files, so there is no wholesale `-X ours` /
`-X theirs` escape.

**The dangerous part is the 49 conflicts, not the 234 files that merge clean.** Measured: of the
files upstream changed, **49 conflict and 234 auto-merge with no conflict marker at all.** Those
234 land silently, carrying `openproxy::` into a crate named `zeroproxy` — so the compile cascade
and the rewrite of the frozen `zeroproxy.v1` namespace arrive with *no* conflict to alert you,
only a wall of `E0433`/`E0432` afterwards. Budget the rename as the dominant cost, not the
conflicts.

**Treat the full merge as a separate rebranding project needing explicit user sign-off.** It is
NOT a prerequisite for provider work: `src/cli/sync.rs`, `scripts/sync/normalize-sources.mjs`, and
`src/core/model/sources/*.json` already exist **on our side** and predate the divergence. The only
upstream commit touching the sync subsystem (`f2b9dfcb`, a 9router snapshot refresh) contains **zero
brand references** — so single commits can be cherry-picked safely. (An earlier revision of this trap
claimed `src/cli/sync.rs` leaked an `openproxy.v1.sync.apply` envelope. False: our copy emits
`zeroproxy.v1.sync.apply` at `src/cli/sync.rs:588`; it was the *upstream* copy that said `openproxy`.)

**What the 259 are actually worth** (measured): 107 `fix` + 58 `feat` = 165 substantive, plus 23
`beads` bookkeeping and 29 `chore`. **Zero dependabot** — unlike what the PR list suggests, this is
hand-written parity work, not dependency noise. So the value is real (that is where the
`fix(chat): resolve the target format transport-first`, `fix(providers): probe OpenAI-compatible`,
and `feat(providers): import catalog from live provider models` work lives), but it is
**not** the free-tier catalog goal: that is already met on our branch via `sync omniroute
--free-only`. Cherry-pick the individual parity commits you want; do not merge the branch.

### 8. `kv` keys custom models by `alias/model-id` — never revert to a bare model id
Custom models live in the `kv` table under `scope = 'customModels'`, primary key `(scope, key)`.
The key is now the composite **`"{provider_alias}/{model_id}"`**, written by both
`patch.rs::custom_models_map` and `sqlite/import.rs`. Model ids are only unique *within* a
provider, so the earlier bare-`id` key let two providers offering the same id overwrite each other.

Free-tier catalogs collide hard: in the current OmniRoute snapshot 999 free `(alias, model)` pairs
collapse to 773 distinct **ids**. Worst: `deepseek-v4-flash` (10 providers), `openai/gpt-oss-120b`
(9), `gpt-oss-120b` (8), `glm-5.2` (7). That was 132 of 905 genuinely-lost models (23%).

The key is **write-only** — `export.rs::kv_scope_to_array` selects by scope and discards the key,
and no production path looks a model up by key — so the composite form breaks no reader, and `/`
inside an alias or id is harmless because the key is never parsed back.

`rekey_custom_models` in `sqlite/migrations.rs` converts pre-existing bare-id rows on open. It is
**required, not redundant**: `diff_kv_scope` derives its "old" side from the in-memory `Vec`, never
from the table, so without the migration a legacy row is invisible to the delete loop *and* is read
back by `export_all` as a duplicate. It runs on every open alongside `add_api_keys_budget_column`,
with no `SCHEMA_VERSION` bump.

Verify with a real row count, never the CLI's diff: `sync` reports created + unchanged, and
unchanged models are served by the built-in catalog, so the expected free-sync row count is
**905, not 999** (94 of the 999 already exist as built-ins).

### 9. The dashboard has no identity of its own — do not read the backend's sha as the UI's
In `--web-dir` mode (what `dev.sh` always uses) `web/dist` is read from disk per request, so a
freshly compiled binary happily serves a days-old bundle. "Binary current, UI stale" is invisible,
and the navbar badge used to report exactly the wrong thing: the **backend's** commit sha, plus
process uptime (`0m`) in the position a build age belongs. Uptime resets on restart, so it renders
an old build as newly fresh.

The UI now carries its own identity, substituted at build time by `vite.define` in
`web/astro.config.mjs`: `__UI_BUILT_AT__`, `__UI_GIT_SHA__` (a `--short` sha), `__UI_COMMIT_TIME__`
(declared in `web/src/env.d.ts`). `/api/version` grew a purely additive `freshness` block;
`?since=<sha>` returns that sha's distance from repo HEAD, which is how the badge measures its own
staleness instead of guessing.

Consequences worth remembering:
- The badge shows **three** states. `unknown` (no git checkout, an older binary predating the
  field, or a failed fetch) must **not** be styled as fresh — a release tarball reports null
  throughout, and a green dot there would recreate the original lie.
- `commits_since` hex-validates before shelling out; `since` is attacker-controlled, and a leading
  `-` would otherwise be read as a git flag. Verify with
  `curl 'localhost:4623/api/version?since=--upload-pack=touch+/tmp/pwn'` → expect
  `sinceCommitsBehind: null` and no file created.
- Out of a checkout every probe returns `None`; never let a probe failure break the endpoint.
- Probes are cached 15s because the badge polls on a 20s timer and each uncached probe spawns two
  `git` processes.
- **Both states have been seen rendering**, so do not re-verify from scratch. Chromium screenshots
  of the navbar chip: stale reads `● v0.3.1  1cd7344  UI+bin 3`, fresh reads `● v0.3.1  57ff7f9`
  with no chip. Sampled dot pixels are `rgb(245,158,11)` (amber-500, stale) vs `rgb(146,229,196)`
  (emerald, fresh) — an unmistakable hue gap, so a colour-blind or washed-out render still differs.
  Reproduce: `pnpm build` + `./scripts/dev.sh --fast detach`, then Playwright to
  `http://127.0.0.1:4623/dashboard`. Note the chip is in a width-constrained navbar row, so keep any
  future label change short or it will wrap.

### 10. A crate rename silently orphans every insta snapshot
`insta` derives the snapshot **filename from the crate name**. Renaming `cipherroute` → `zeroproxy`
left all 35 committed references named `cipherroute__*.snap`, so insta found no reference and
reported **`+new results` with zero `-` lines** — "missing", not "changed". 30 translator tests
failed this way while the translation output was byte-identical.

- **Read the diff before touching a snapshot.** `+new results` with no `-` lines means the
  reference is missing; only real `-`/`+` pairs mean behaviour changed. Running `cargo insta
  accept` (or `INSTA_UPDATE`) on the former **overwrites the reference with the current output and
  permanently blinds the test** — it would have "fixed" 30 tests that were never broken. The
  snapshot bodies here were verified byte-identical to the tracked `.snap.new` artifacts first.
- **Fix the rename, not the snapshot:** `git mv` each `cipherroute__*.snap` to `zeroproxy__*.snap`.
- **`*.snap.new` is not gitignored**, so insta's staging files get committed by accident. 35 of
  them were tracked here. Before deleting any, prove they hold nothing unique.

## Invariants (must not break)

1. **Capability filter before routing.** `HARD_CAPS = ["vision","pdf","audioInput","videoInput"]`
   (`src/core/combo/mod.rs:438`). `detect_required_capabilities` (`:442`) runs *before*
   `reorder_by_capabilities` (`:697`), which tier-sorts then falls back. A hard-cap mismatch
   **skips the model entirely** — it is not a fallback trigger.
2. **`context_window` cap.** History is trimmed by `strip_history_for_context`
   (`src/core/combo/capacity_adapter.rs:254`) — only for capacity-adapter-added models.
   Budget = `(context_window || 200_000) * 0.8 * 4`.
3. **Fallback only on eligible errors.** `check_fallback_error` (`src/core/combo/mod.rs:729`)
   delegates to `error_config::classify_error` (`src/core/config/error_config.rs:253`) →
   `ErrorClassification::{Backoff, Cooldown, NoMatch, Permanent}`. `retryAfter` header beats body.
   **404 → 300 s model lock; it is NOT an auth failure.**
4. **Error classification has one source.** Never re-implement status→fallback logic inside an
   executor; go through `error_config::classify_error`.

## Core: What / Why / How

OpenAI-compatible endpoint routing to 40+ AI providers with format translation, account
fallback, token refresh, usage tracking, and SSE streaming. A stripped Rust clone of OmniRoute —
**avoid OmniRoute's bloat**.

**Pipeline**: `model parsing → format detection → request translation → capability-aware
ordering → provider execution → response translation → SSE streaming`

- **Executor trait**: `ProviderExecutor` — default + per-provider impls (`src/core/executor/`)
- **Persistence**: SQLite (WAL) + AES-GCM encrypted credential columns (`src/db/`)
- **Security**: HMAC API keys, bcrypt/JWT auth, SSRF protection (`src/server/auth/`)

**Layout**: `src/core` (domain) · `src/server` (axum HTTP + dashboard embedding) · `src/cli`
(clap CLI + `--robot` JSON envelopes) · `src/db` (SQLite) · `src/oauth` (refresh flows) ·
`web/` (Astro 5 + React 19 dashboard, built to `web/dist`).

### Guiding principle — check OmniRoute before inventing

`~/dev/OmniRoute` (present on this machine) already implements most features. Look there before
designing something new. **Do NOT port**: MCP servers, A2A/ACP, Electron/PWA/VNC, memory/skills
frameworks, analytics beyond minimal usage, cloud/sync backends, Telegram bots, chaos engineering.

## Core Product Surfaces (TOP PRIORITY)

These 4 surfaces *are* the product. Prioritize their regressions above all else.

1. **Providers page** — `/dashboard/providers/<provider>`: Available Models toggle
   (disable/enable/custom), persisted in SQLite, survives rebuilds.
2. **CLI tools config** — `/dashboard/cli-tools/opencode`.
3. **Combos page** — `/dashboard/combos`.
4. **`web/src/shared/components/ModelSelectModal.tsx`** — the single model-picker used
   everywhere. It **must mirror** the provider page's Available Models (same disabled map +
   custom rows + catalog merge). Any model-list change applies to both.

Hot files prone to cross-agent conflicts: `src/server/api/chat.rs`,
`web/src/shared/constants/providers.ts`, `src/core/model/provider_catalog.json`.

Never break: configure provider → customize available models → create combos → select models
for opencode CLI config.

## Dev Workflow — backend + dashboard

Backend and dashboard are **separate builds** served by one binary. Run `./scripts/dev.sh`
from repo root only (it resolves its own root, so cwd doesn't matter).

### Presets

| Situation | Command | Cost |
|---|---|---|
| Iterate on one layer | `--fast` (default) | ~10-20 s |
| Daily loop (build + restart) | `--fast detach` | ~10-20 s |
| Only `web/src` changed | `--web-only` | no cargo |
| Only `src/` changed | `--backend-only` | no pnpm |
| **Before any push / done claim** | `--full detach` | ~2-5 min, runs fmt+clippy+astro+tests |
| Lint only, no build | `--check` | — |
| Suspect stale `web/dist` | `--web-only` or `--full` | forces `pnpm build` |

Modes: `run` (foreground) · `detach` · `build` · `check` · `check-stale`. `--release` switches
to the release profile. Full flags: `./scripts/dev.sh --help`.

### How web assets are served

- **`--web-dir` mode (what dev.sh uses)**: server reads `web/dist/` from disk at runtime, so
  `pnpm build` alone makes UI changes visible — **no binary rebuild needed**.
- **Embedded mode (release)**: `web/dist` baked in via `rust-embed`
  (`src/server/dashboard/`, `#[folder = "web/dist/"]`). Requires `cargo build`.

> **Never run the release binary for dev work** — stale embedded assets. dev.sh always passes
> `--web-dir`.

### Reload contract (run this yourself — never ask the user)

| Change | Command | Why |
|---|---|---|
| `src/**` Rust | `./scripts/dev.sh --fast detach` | cargo rebuild required |
| `src/core/model/provider_catalog.json` | `./scripts/dev.sh --fast detach` | catalog is `include_str!`-embedded at compile time — **restart alone is not enough** |
| `web/src/**` | `./scripts/dev.sh --web-only` | `--web-dir` serves disk live |
| DB / config / model-lock only | `./scripts/restart.sh` | zero cargo rebuild, no stale-binary risk |
| Claiming completion / pre-push | `./scripts/dev.sh --full detach` | full gate |

**Never report a fix as done without a reload + health check.** `detach` runs
`verify_fresh_binary` (confirms the listening PID's `/proc/<pid>/exe` matches the freshly built
binary) and gates on `curl -sf http://127.0.0.1:4623/health`. If either fails, abort and report
the mismatch — a stale binary is the #1 silent regression source here.

Raw Astro dev: `cd web && pnpm dev` → `:4624`, proxies `/api`, `/v1`, `/health`, `/oauth` to
`:4623` (HMR is disabled in `web/astro.config.mjs`).

### Runtime state

- Port `4623`; data dir from `$DATA_DIR`, default `~/.zeroproxy`; SQLite at
  `$DATA_DIR/zeroproxy.sqlite` (mandatory store, no fallback).
- Logs: `~/.zeroproxy/log.txt`, or `journalctl --user -u zeroproxy -f`.
- Release build: `cargo build --release --locked --no-default-features && ./target/release/zeroproxy --web-dir ./web/dist`.

## Testing

- **Only Linux actually runs the tests.** The `rust` job matrixes `[ubuntu-latest, macos-latest]`,
  but its test step is `- name: cargo test (Linux only)` / `if: runner.os == 'Linux'`. So **macOS
  proves fmt + clippy only**, and a green macOS run says nothing about tests. Do not read a macOS
  pass as "tests pass" — that misreading happened here once already.
- **The gate is green: `cargo test --lib --all-features` → 1905 passed, 0 failed.** Keep it that
  way; a red merge is not worth landing, because it destroys the only thing that makes the gate
  worth having.
- Integration tests under `tests/` are intentionally excluded (their build was repaired separately
  — they now compile, but their pass rate is unmeasured), so a green local `cargo test` on `tests/`
  is *not* the gate.
- Unit tests live in `src/**` as `#[cfg(test)]` modules (~224 files) — this is why `--lib` is
  the CI gate; parity locks need no network.
- Web: `pnpm --dir web test` (vitest, 2 suites: `availableModels`, `providersPage`).
- `astro check` is **advisory** everywhere (`|| true` in CI, `|| echo advisory` in dev.sh) — fix
  new errors, don't chase the existing backlog.
- `cargo clippy --all-targets --all-features` — no `-D warnings`; it fails only on real errors.
- `scripts/parity-smoke.sh` is the intent, not the source of truth (see Trap 1).

## CI order (matters)

`lint-branch-name` → `web` (install → astro check* → vitest → `pnpm build` → upload `web/dist`)
→ `rust` (downloads `web/dist`, then fmt → clippy → `cargo test --lib`). The Rust job
**depends on the web artifact** because `build.rs` needs `web/dist/index.html`.

## Git hygiene

Install hooks once per clone: `./scripts/setup-hooks.sh` (copies `.githooks/*` → `.git/hooks/`).
Re-run after pulling hook changes.

- **pre-commit**: `cargo fmt --check` if `.rs` staged; secret scan; **blocks staging
  `opencode.json`, `.env`, `admin.key`, `db.json`**.
- **commit-msg**: Conventional Commits `type(scope): imperative` — types
  `feat|fix|docs|chore|refactor|test|build|ci|perf|revert`.
- **pre-push**: branch-name lint, secret scan, stale-`web/dist` warning, advisory astro check.
- **CI lints branch names**: only `main`, `dev`, `pr-*`, `<type>/<kebab>`, `<agent>/<type>/<kebab>`
  where type ∈ the list above. A non-conforming branch fails fast.
- Never commit: `opencode.json`, `.env`, `*.pem`, `admin.key`, API keys (`sk-…`, `Bearer …`,
  `refresh_token` assignments). If one lands, rotate it and purge history (`git filter-repo`).

Full rules: `CONTRIBUTING.md` + `docs/git-conventions.md` (both tracked) + `.github/pull_request_template.md`.

## Agent Orchestration — branch-per-agent, worktree isolation

**Rule: one agent = one branch = one worktree. Never share a branch.** Shared-branch edits
previously produced a 26-file stash and cross-branch cherry-picks.

```bash
cat .opencode/claims/* 2>/dev/null; git worktree list   # check claims first
git worktree add ../wt-<agent>-<slug> -b <agent>/<type>/<kebab>
./scripts/claim-branch.sh <agent>/<type>/<kebab>        # NOT ../cipherroute/... — that path is gone
./scripts/claim-branch.sh --release <branch>           # when done
```

- Claim files live in gitignored `.opencode/claims/<branch-with-__slashes>`; the script also
  rejects names that already exist in git.
- `dev.sh` warns on a dirty tree; hooks block wrong-branch commits.
- Return to `main` when finished — don't park the repo on a feature branch.
- Full spec + recovery: `docs/agent-orchestration.md` (tracked). Note it still contains a few
  stale `../cipherroute` paths.

## Schema stability

`zeroproxy.v1` envelope namespace is **frozen, additive-only** — 13 resources (`provider`,
`provider-node`, `combo`, `key`, `pool`, `settings`, `custom-model`, `model-alias`,
`usage-event`, `log-event`, `chat-event`, `quota`, `oauth-status`), each with schema + example
enforced by tests. Inspect with `zeroproxy schema list|show|example|stability`
(`src/cli/schema.rs`). Breaking changes require a new `zeroproxy.v2` namespace.

## Beads / parity status

Parity work tracks 9router v0.5.55 behavior. Tracked docs: `docs/parity-9router.md`,
`docs/parity-9router-FULL.md`, `docs/parity-9router-impl.md`, `docs/residual-gaps.md`. Beads
epic `cipherroute-9router-parity-v0550-pnc`; the `br` / `bv` CLIs are **not on PATH** — treat
the markdown files as the source of truth. Smoke (one filter per invocation — `cargo test` takes
a single TESTNAME): `cargo test --lib parity_tests`.
