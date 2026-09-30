# ZeroProxy — Rust AI Proxy Router

## Mission — read this first, it is not up for renegotiation

> **ZeroProxy is "a lightweight OmniRoute Rust alternative for all free-tier LLM providers."**

That one phrase is the whole product. Everything below is subordinate to it.

**The goal is OmniRoute. The 9router/openproxy era is over.** We already replicated almost every
feature from 9router and quangdang46/openproxy, and both of those repos update slowly, so chasing
their release notes has low return. OmniRoute is the only upstream that still moves fast enough to
be worth mirroring, so:

- **Parity target = OmniRoute's FREE-TIER provider list + model catalog + model conventions.**
- **The free tier is the optimisation target, not an exclusive whitelist.** Free-tier providers come
  first everywhere: they sort into the Free Tier section, which structurally sits above API Key, and
  the "free only" filter hides everything else. Non-free providers OmniRoute lists are still carried
  — in a **separate `GENERATED_OTHER_PROVIDERS` map** so they stay inert until someone opts in with
  one import. Do not treat their absence as a gap; do not let them crowd out free-tier work.
- **Lightweight is a hard constraint, not a nice-to-have.** If a port drags in MCP, A2A/ACP,
  Electron/PWA/VNC, memory/skills frameworks, cloud sync, Telegram, or chaos engineering, it is
  the wrong port. See the "do NOT port" list.
- **LLM providers are the real targets**, free-tier ones first. Search/fetch/TTS/embedding/image
  providers are lower value — do not let them crowd out LLM work.
- **Seven providers are the floor, not a sample.** The maintainer uses these hourly and they must
  "always be up to date and have no errors": **NVIDIA (NIM), OpenRouter, Kilo, OpenCode, Kiro,
  Gemini + Antigravity, AgentRouter.** They are 7 logical providers but **12 catalog ids**
  (`opencode` + `opencode-go` + `opencode-zen`; `kilocode` + `kilo-gateway`; `gemini` +
  `gemini-cli`), and an id is what the code keys on. Free-tier breadth does not outrank a
  regression in these; when they conflict, they win. Only 3 of the 7 are OAuth (`kiro`,
  `kilocode`, `antigravity` — all with a `pub fn` in `src/oauth/providers.rs`); the other four are
  API-key only, so an OAuth-config guard does not cover them. See Trap 19 for the catalog/snapshot
  alias reconciliation these providers need.

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

**If you were asked to implement a feature, read
[Agent Orchestration](#agent-orchestration--the-maintainers-loop-and-how-to-stay-out-of-each-others-way)
NOW, before your first edit.** It defines the maintainer's 4-step loop, the branch-per-agent rule,
and why port 4623 is the single most common way two concurrent agents destroy each other's work.
The short version, which overrides any instinct to be helpful:

1. **Never merge to `main` without being asked in that same conversation.**
2. **Never commit at the end of an implementation** — the maintainer tests before committing.
3. **Never edit in the main worktree** — make a branch and a worktree first.
4. **Never touch port 4623** unless you are the maintainer's own test instance. Use `PORT=4631` —
   but only for runs that start nothing; `PORT=` does not isolate you from the shared
   `zeroproxy.service` unit (Trap 17).

## Current state (2026-09-26) — read before planning more work

**The analytics rewrite IS on `main`** (it was merged, then the remote branch was deleted on
2026-09-27). Fixes the success-rate lie (failures were never persisted) and adds
`/dashboard/analytics` + `/dashboard/provider-stats`. **This line was stale for a week** — it
claimed the work was unmerged, and an agent nearly preserved a branch that had nothing left in it.
If you ever doubt a "current state" claim here, check the artifact, not the sentence:
`git merge-base --is-ancestor <branch> main` and `git ls-tree -r --name-only main | grep <the file>`.

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
- The 30 "translator snapshot regressions" were **not** regressions: insta derives the snapshot
  filename from the crate name, so the rename had orphaned every reference. They were renamed, not
  re-accepted — see Trap 10 for why `cargo insta accept` would have destroyed them.
- **Immediately after the merge, `./scripts/dev.sh` could not start at all**, because its port check
  matched a Tailscale funnel socket on another interface (Trap 4b). That is fixed on `main` at
  `d5d1637f`. If the dev loop ever dies with "port 4623 still held" while `pgrep -x zeroproxy` is
  empty, that is the cause — and note `fuser -k <port>/tcp` was on the path that would have killed
  `tailscaled`.
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

### The re-verified `src/`-free shortlist (measured 2026-09-26, against `main` @ `bf042d59`)
An earlier shortlist of these 27 was built **before** the 25 free-tier commits landed, and it is now
**wrong** — do not use it. Each of the 27 was re-tested with `git show <sha> --format= | git apply
--check -` against today's `main` (upstream is unmoved at `0b77f8fe`, 259 ahead, base `6eec13d8`).
Result: **10 apply cleanly, 17 now conflict.**

- **Applies clean AND brand-clean in the diff (8)** — the only ones worth picking:
  `c58b35f7` pass `providerId` at remaining ProviderIcon sites · `4b05b4bd` stop opening the chat
  page from erasing chat history · `57dd2e29` XiaomiMimoAuthModal · `c3c0eb95` providerIcon alias
  resolve + 404 session cache · `d690880a` pxpipe restore details · `c37d42c4` pricing explainer +
  top-5 overview · `d0737b17` + `20936f79` quota UI + quota utils (**a pair — take both or
  neither**). Six of these show `brand_refs=1`, but every one is a `Refs openproxy-…` **beads line
  in the commit message body**, not diff content.
- **Applies clean but excluded (1):** `6925a3ef` has **11 in-diff** brand refs, including
  `OPENPROXY_CODEX_TOKEN_URL` — a name `src/` does **not** read (it reads `ZEROPROXY_CODEX_TOKEN_URL`),
  so it is the Trap-6 env-var trap. It is also `tests/`-only; the `src/` fix it tests lives in a
  different commit, so picking it alone is useless.
- **Now conflicted (17)** — the previously-promised Tier 1 `29c97c10` (a11y model picker) is in
  here, as are `cf667502`/`588cb9fc`, `952c937b`/`ecf9bde3`, `e8d858ed`, `0d9a5e4e`, `b9dc17e1`,
  `96f28409`, `33a69252`, `406f00cb`, `ac1d7116` (already superseded by `19d8e0eb`), `e5db61ab`
  (settled: do not port), and the four `sim-` commits `4619a421`/`701bdc7e`/`c381d4dd`/`8635fc2a`.

`git apply --check` tests each commit **in isolation**, so a commit whose partner already conflicted
can still read CLEAN. Pairs are called out above; respect them.

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

- **`gh` resolves to the WRONG REPOSITORY unless you pass `--repo`.** This checkout has **two**
  remotes: `origin` = `atheerium/ZeroProxy` (ours) and `upstream` = `quangdang46/openproxy` (our
  parent). **Measured: bare `gh repo view` returns `quangdang46/openproxy`, not ZeroProxy** — so
  `gh pr list` shows *upstream's* PRs (#468-470) and not ours (#14-#18). This repo and upstream
  **both have a PR #18**, so `gh pr view 18` cheerfully reported an unrelated upstream PR as
  `MERGED`, and `gh pr merge 18` was a no-op against it one command away from merging in the wrong
  repository. **Always pass `--repo atheerium/ZeroProxy`,** or go through
  `gh api repos/atheerium/ZeroProxy/...`. Verify with
  `gh repo view --json nameWithOwner` before trusting any bare `gh` output. The `github_*` MCP
  tools are unambiguous — they take an explicit owner/repo.
- **The shell is zsh, which does not word-split unquoted variables** the way bash does. `for b in
  $LIST; do git branch -d "$b"; done` passes the ENTIRE list as ONE argument — it failed here with
  25 branch names handed to a single `git branch -d`, and a `git commit` wrapped in a variable
  (`G="git -c user.name=…"; $G commit`) dies with `command not found`. This bit twice in one
  session. **Use a `while read` loop instead:** `git for-each-ref … | while read -r b; do …; done`
  — piping splits correctly because the split happens in the subshell, not on parameter expansion.
  Related: **`$PIPESTATUS` does not exist in zsh**, so `cmd | tail; echo $?` reports `tail`'s exit
  code and `echo ${PIPESTATUS[0]}` prints empty. Redirect to a file and read `$?` instead.
- **`git grep -c` counts matching LINES, not occurrences.** Use `git grep -o <pat> <ref> | wc -l`.
  Using `-c` reported 86/95 crate refs where the real numbers were 152/143.
- **`git show --stat` prints the path BEFORE the pipe.** Classify with
  `git show --name-only | grep -c '^src/'`, never `grep '| src/'` — the latter matches nothing
  and silently classified all 171 commits as `src/`-free.
- **`cargo clippy` aborts at the first failing integration target.** Truncated output undercounts
  badly. Iterate until clean; never trust a single run's error list.
- **`pnpm build` does not type-check** (Astro skips `tsc`) and the repo has ~492 pre-existing tsc
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
- **`tsc` total is not proof; a per-file filter is.** A total equal to the 492 baseline can hide
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

**Settled 2026-09-30: OmniRoute's quota *scheduling engine* is a NO-GO. Do not port it.** The
maintainer's call, answering "if OmniRoute doesn't support it, it's probably very hard for us
because OmniRoute is more mature than us." OmniRoute *does* have it, so that heuristic does not
apply here — this is a deliberate scope decision instead, and it is the lightweight constraint
above winning. The distinction matters, because the two halves are not the same work:

- **Quota FETCHING: we already have it, and it is not a port.** `src/core/usage/quota_fetcher.rs` is
  3,158+ lines with **14** `fetch_*_quota` functions — `fetch_kiro_quota`, `fetch_antigravity_quota`,
  `fetch_gemini_cli_quota`, `fetch_codex_quota`, `fetch_github_quota`, `fetch_claude_quota`,
  `fetch_glm_quota`, `fetch_minimax_quota`, `fetch_qoder_quota`, `fetch_vercel_ai_gateway_quota`,
  `fetch_codebuddy_quota`, `fetch_grok_cli_quota`, `fetch_ollama_quota` — dispatched by
  `fetch_oauth_quota` (`src/server/api/usage.rs:55`, match at `:67-79`), with a `zeroproxy quota`
  CLI at `src/cli/quota.rs:44` and `fetch_oauth_quota_with_refresh` at `usage.rs:1161`. **If a quota
  feature is ever requested, the work is a UI/aggregation view over data we already collect — do not
  re-derive the fetchers or assume we lack them.**
- **Quota SCHEDULING/BUDGET POLICY: that is the no-go.** OmniRoute adds a whole policy engine on top
  of fetching: `QuotaDimensionSchema`, `QuotaScheduleSchema` (IANA-timezone day windows with
  `startMinute`/`endMinute` wrapping midnight, `mode: allow|block`, up to **50** schedules),
  per-window **reserves** (`any|5h|hourly|daily|weekly|monthly`, fractional percent, where `any`
  lets the most-consumed window decide), its own `budgetValue`/`budgetUnit` in
  `requests|tokens|usd`, `QuotaStoreSettingsSchema` (sqlite|redis driver), `QuotaPreviewQuerySchema`
  and an `AuditLogQuerySchema`. That is a scheduling layer, a persistence abstraction, and an audit
  log — none of which a lightweight proxy needs to route a request.

One genuinely worth stealing from OmniRoute's telemetry spec, and cheap: its five truthful states —
`healthy` / `approaching_limit` / `exhausted` / `unavailable` / `unknown` — plus the rule **"Unknown
is not exhausted and does not disable a provider"** and **"local estimates are never presented as
provider billing data."** We already fail closed on an unset encryption key (see the crypto trap); do
not repeat that pattern for quota.

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

### 3. `HARD_CAPS` and `model_has_capability` must stay single-source
There is exactly **one** definition of each, both in `src/core/combo/mod.rs`:
`HARD_CAPS` (`:446`) and `model_has_capability` (`:653`), both `pub(crate)` because
`src/core/auto/mod.rs` and `src/core/combo/capacity_adapter.rs` now import them.

**This used to be FOUR `HARD_CAPS` and THREE copies of `model_has_capability`.** An
earlier version of this file said "two `HARD_CAPS` definitions" and was wrong, which is
itself the lesson. The real inventory:

| Site | State before the collapse |
|---|---|
| `combo/mod.rs` `HARD_CAPS` | live — the tier gate at `:722` |
| `auto/mod.rs` `HARD_CAPS` | live — the auto-candidate **drop** |
| `combo/capacity_adapter.rs` `CAPABILITY_KEYS` | live, under a **different name**, so `rg HARD_CAPS` never found it |
| `combo/capabilities.rs` `HARD_CAPS` | dead — the whole module has zero consumers |

The three `model_has_capability` copies were byte-identical after normalising comments
(1123 chars each), so they had **not** drifted yet — the hazard was latent, not realised.
A grep for the constant name alone is not how you find this class of bug; the third
copy was filed under a different identifier.

Guarded by `src/core/combo/hard_caps_single_source_tests.rs` (7 tests), which scans
`src/**` and fails if a second definition appears, and pins the contents plus the
heuristic's answers. It lives in `src/` rather than `tests/` because `cargo test --lib`
is the CI gate and `tests/` is excluded from it.

`combo/capabilities.rs` itself was **deleted** once its dead `HARD_CAPS` was gone — 817
lines, zero consumers. But it hosted `reorder_floats_capable_models_to_front`, which
tests the **live** `reorder_by_capabilities` and is its only direct test, so that one
test was **relocated into `combo/mod.rs`'s test module** rather than deleted with the
file. The general lesson: a dead file can still be the only home of a live test. Check
what each `#[test]` actually calls before removing a module wholesale.

**Do not add a local `HARD_CAPS` or a second `model_has_capability`**, including a
"temporary" one in a submodule. The two live call sites deliberately *differ* — the
combo gate **de-motes** a hard-cap mismatch, the auto path **drops** it — and they are
only guaranteed consistent because they share one function and one list. The guard test
`hard_cap_mismatch_demotes_rather_than_removes` pins that contrast.

### 4. `zeroproxy.service` respawns and steals the port
The systemd user unit has `Restart=always`; killing the process alone lets it return in ~5 s and
the next start dies with `EADDRINUSE`. `dev.sh`/`restart.sh` stop the unit first. **Never** use
bare `pkill`, `nohup`, or a hand-rolled start. `start_server.sh` is deleted on purpose.

### 4b. A port held on *another* interface is not a conflict — `dev.sh` used to think it was
`zeroproxy` binds `127.0.0.1:$PORT` (its argv literally says `--host 127.0.0.1`), so a socket on a
different interface cannot block the bind. `dev.sh` nevertheless grepped `ss -tlnp` for the bare
`:4623 ` in **three** places, and a `tailscale serve`/funnel listener — which the dashboard's
"Tunnel"/"Tailscale" toggles create — sits on this machine's own Tailscale IP. Symptoms, all three
misleading:
- `wait_for_port_free` declared the port held and **refused to start the server**, so the documented
  workflow was dead for as long as any funnel was enabled. This is the failure that prompted the fix.
- `verify_fresh_binary` resolved the pid from that socket, so it compared **`tailscaled`'s**
  `/proc/<pid>/exe` to our build and could report a bogus "running pid uses DIFFERENT binary".
- `kill_port` ran `fuser -k "${PORT}/tcp"`, and **fuser kills by port, not address** — it would
  have killed `tailscaled`, a system service. The neighbouring `pkill` patterns match only our own
  cmdlines, so this was the one path that could take down a system service.

All three now go through `loopback_socket_lines()` / `loopback_port_held()`, which match the
**address** (`127.0.0.1`, `[::1]`, plus `0.0.0.0`/`[::]` because a wildcard bind genuinely does
block us). **Do not simplify those back to `:${PORT} `** — that is the bug, and the `fuser` guard
looks redundant until you notice what it is keeping alive.

Diagnose with `ss -tlnp | grep 4623` and compare against `100.115.170.56` (this box's Tailscale
IP). Remember `pgrep -f 'zeroproxy.*4623'` self-matches; use `pgrep -x zeroproxy`.

### 4c. `server start --detach` used to silently drop `--web-dir` — a green health check
meant nothing about the dashboard
`run_start` (`src/cli/server.rs`) does not fork-and-exec with the parent's argv. In the detached
branch it **builds a fresh argv from scratch**, and it used to emit only `--no-open --host --port
--data-dir`. `dev.sh` invokes `"$BIN" --web-dir "$REPO_ROOT/web/dist" server start --detach …`, so
`--web-dir` reached only the short-lived parent; **the process that actually served never received
it** and fell back to build-time-embedded assets. Same for `--dashboard-sidecar-url` (the `:4624`
Astro dev proxy), so `pnpm dev` HMR was broken the same way.

Why it is expensive: **it fails silently and looks successful.** `run_start` probes
`/api/health` after spawning, and the health endpoint does not care where assets came from — so
dev.sh printed "binary verified / health ok", exited 0, and `verify_fresh_binary` passed. Only the
dashboard was wrong. Symptoms, all of which look like something else:
- every `/dashboard/*` page 404s into the `dashboard.html` SPA shell, which renders
  `EndpointPageClient` — a *plausible-looking* endpoint page, not an obvious 404;
- `web/src` edits appear to do nothing until you rebuild the binary;
- a page you just built is "missing" from the live server even though `web/dist` contains it.

**Assert the flag, never the health check:** `pgrep -x zeroproxy` must show `--web-dir` in the
argv. Fixed by carrying both flags in `StartOptions` and re-emitting them via `detached_argv`,
which has a regression test — but the argv is still a hand-maintained allowlist, so a new
`StartOptions` field must be added there too or it will be dropped the same silent way.

Two related things that cost the same debugging time: **`--web-only` never starts the server** (it
builds and exits telling you to run `--fast detach`; use `--web-only detach`), and
`dev.sh --fast` from a worktree is pointless if that worktree has no `target/` — `verify_fresh_binary`
compares `readlink -f /proc/<pid>/exe` against `$REPO_ROOT/target/debug/zeroproxy`, so a cold
worktree needs either a real build or a **hardlink** (not a symlink) from another checkout.

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
  them were tracked here. Before deleting any, prove they hold nothing unique. This is the same
  three-layer rename trap: the rename fixed the *test files* in `6f31d535` but not the snapshot
  *filenames*.
- **The catalog sync is model-level, not provider-level** — invisible until the maintainer asked
  why the dashboard showed 21 providers. `sync omniroute` writes *models* into `customModels`; it
  never adds a provider you can *add*. Three different numbers are all correct and none is the
  catalogue: the Providers page lists **configured connections** (21), the addable catalogue is
  `AI_PROVIDERS` (hand-maintained **130**; `GENERATED_FREE_TIER_PROVIDERS` + `GENERATED_OTHER_PROVIDERS`
  now bring it to **326**), and the snapshot holds 270.

### 11. Two generator-output traps — fix the generator, never the emitted file
`scripts/sync/generate-web-providers.mjs` emits `web/src/shared/constants/providers.generated.ts`.
Both of these shipped a broken file before being caught, and both were fixed **in the generator**,
which is the entire point of having one:
- **`new URL("scheme://x").origin` returns the string `"null"`, not a URL.** `JSON.stringify("null")`
  then renders a literal `website: "null"` in the UI. Require `protocol === "http:" || "https:"`
  before assigning. This skipped 7 providers legitimately: `auggie`, `codex-app-server`, `zcode`,
  `devin-cli`, `devin-cli-agentic` (`*://` schemes) and `copilot-web`, `copilot-m365-web` (`wss://`).
- **Provider ids contain hyphens, which are invalid in a bare TS object key.** 80 keys were emitted
  unquoted and esbuild failed with `Expected "}" but found "-"`. Use `JSON.stringify(id)` for keys.

**Verify with `node scripts/sync/generate-web-providers.mjs --check`** (exit 1 if the committed file
is stale) — the same shape as the existing `--prune` staleness idea, and suitable for a pre-push hook.

### 12. The sync normalizer projects each provider field TWICE
`scripts/sync/normalize-sources.mjs` builds a provider twice: the inner `omnirouteLoaderSource`
(the `tsx` script) assembles `out.registry[id]`, then the outer `loadOmniroute` re-projects an
**explicit field list** into the final array. A field added to only one projection **vanishes
with no error, no warning, and a plausible-looking snapshot** — `modelsUrl` was carried correctly
through the inner projection and still came out `0` across all 270 providers. Always add to both,
and verify by counting the field in the regenerated JSON rather than by the absence of an error.
- **`omnirouteLoaderSource` is a JS template literal.** A **backtick inside a comment terminates
  the string** and the whole normalizer dies with a syntax error pointing at unrelated text. No
  backticks in that block.
- **Use a fresh clone for data.** `~/dev/OmniRoute` is pinned to an old release and is ~1 month
  stale. Clone fresh or pass `--src-omniroute=<fresh clone>`; re-measure the free/provider counts
  afterwards rather than trusting the old checkout.
- **One `modelsUrl` in 96 is relative (`volcengine-coding-plan` → `"/models"`) — leave it failing
  closed. Do not "fix" it by joining onto `baseUrl`.** `Url::join("/models")` replaces the whole
  path and yields `…volces.com/models`, dropping `/api/coding/v3`, which returns **401 per the
  provider's own registry comment**; joining onto the base's *directory* instead yields
  `…/api/coding/v3/chat/models`, still wrong. The value actually served needs **two** path segments
  stripped, which no principled rule produces. **OmniRoute never resolves it either** — its
  discovery route uses a hardcoded `NAMED_OPENAI_STYLE_PROVIDERS` set, and
  `providerModelsConfig.ts:495` states outright *"The registry has no modelsUrl, so without this
  entry the route fell back to a stale 6-model seed."* Their `modelsUrl` is documentation, not a
  fetch input, so this limitation is **shared, not a ZeroProxy gap**. The guard rejects it, the
  provider keeps its `does not support models listing` error, and
  `models_url_guard_rejects_non_http_schemes` pins the case so it is not deleted as a typo.

### 13. `core::dns::is_private_ip` fails OPEN on link-local — do not use it to guard a URL
It takes a **bare IP string**, not a URL or hostname, and returns `false` for anything unparseable
(`src/core/dns/mod.rs:346`). It covers `10/8`, `127/8`, `172.16/12`, `192.168/16`, `::1`,
`::ffff:127/104` — **not** `169.254.0.0/16`. So `is_private_ip("http://169.254.169.254/…")`
returns `false`, i.e. *permissive*, on exactly the cloud-metadata case a guard exists to catch.
The correct primitive is `resolve_public_ip` in
`src/core/translator/helpers/image_helper.rs` — stricter, DNS-resolving, and its own tests assert
it blocks `169.254.169.254`, `100.64.0.1`, and `240.0.0.1`. It is `pub(crate)` for reuse by
`provider_models.rs`, which guards its `modelsUrl` fetch with it.
- Residual, documented rather than fixed: `resolve_public_ip`'s caller binds `let _pinned_ip = …`
  and then fetches **by hostname**, so the check guards *before* the request but leaves a TOCTOU
  re-resolution window. That is the house pattern; widening it into a real connection pin is its
  own change.

### 14. A change rendering live is NOT a change that is saved — report the state, then ask
A `--web-dir` server reads `web/dist` from disk, and a binary in a worktree runs whatever was compiled
into it a minute ago. So **uncommitted code is served byte-for-byte like committed code**: every route
200s, every screenshot looks right, and the work still dies with the worktree. This repo has already
lost work exactly this way — a full analytics feature was built, gated green (`1909 passed`,
`--full detach` exit 0), screenshotted in a real browser, and reported as done while sitting
**uncommitted** in `wt-analytics`. It surfaced much later, while debugging something unrelated, and the
reasoning was gone by then. Seeing it work was read as *merged*, which it was not.

**Seeing the change is not evidence that it is saved.** Never let "it's live", "it's running", or "it
works" stand in for either. Before the final report on any work that touched files, run the real state
— do not recall it:

```bash
git -C <worktree> status --porcelain           # anything uncommitted?
git -C <worktree> log --oneline <base>..HEAD   # committed on this branch, and how many
git branch -r --contains HEAD                   # pushed?
gh pr view --json state,mergedAt               # open, or merged?
```

Then close the loop **explicitly, every time**, in one of these forms:

| State | What the report must actually say |
|---|---|
| uncommitted | "**Nothing is committed.** N files changed in `<worktree>` on branch `<b>` — do you want me to commit?" |
| committed, not on `main` | "Committed as N commits on `<b>`, **not on `main`**. Want me to open a PR?" |
| PR open | "PR #N is **open, not merged**." |
| merged | only *now* may a report call the work part of `main` |

Asking is the default. **Neither committing unprompted nor staying silent is right** — an unasked
commit can land on the wrong branch or sweep in unrelated files, which is its own organizational
error. Report the state, ask once, act on the answer. This applies to this file too: a `docs(agents)`
rule written but left uncommitted protects nobody.

### 15. `apply_normalization_hooks` was DEAD CODE for every OpenAI→OpenAI request
The request dispatch in `src/server/api/chat.rs` was `if passthrough {…} else if
plan.needs_translation() { translate_request_with_strip(…) }` — **with no third arm**. And
`needs_translation()` is `source_format != target_format`, so an OpenAI client talking to an
OpenAI-compatible provider (OpenRouter, Kilo — both of which `get_target_format_for_provider`
maps to `_ => Format::OpenAi`) matched *neither* branch. `translate_request` is the only other
caller of the normalization pass, so **`ensure_tool_call_ids`, `normalize_developer_role` and
`fix_missing_tool_responses` never ran for those requests either** — not just the new
degenerate-tool-call repair. The hooks' own doc comment already claimed they were "always run
regardless of translation"; the call site never honoured it, so the comment was the lie.

Fixed by exporting `registry::normalize_openai_messages(body, target)` and calling it from a new
`else` arm. The two arms are mutually exclusive, so nothing is applied twice. **The lesson is
general: adding a repair inside `translate_request` does not mean it runs.** A defensive
normalization belongs on the path that serves the *untranslated* request, and "the request
format equals the provider format" is by far the most common case in production — check the
dispatch `if`/`else if` chain, not just the helper.

Cost of the gap: a Cohere 400 reached the client verbatim
(`invalid request: invalid message provided at index 1: must have non-empty content or tool
calls.`, surfaced inside OpenRouter's `metadata.raw`). Two independent upstream realities
compounded — free-tier reasoning models answer with `content: null` because the whole turn went
into `reasoning`, and free-tier small models emit truncated tool calls — and nothing on our side
repaired either. Note `content: ""` is **not** a repair: Cohere answers `missing required
parameter: 'messages[1].content'` for it, so a zero-information assistant turn must be **dropped**,
not blanked. Dropping is safe because `filter_to_openai_format` already retains blank-content
messages away, so the behaviour only changes on the path that had no such filter.

### 16. "Invalid client provided" on Kiro/AWS Builder ID — a bad client id, self-inflicted
`Invalid client provided` is the literal text AWS `sso-oidc` returns for
`StartDeviceAuthorization`'s **`InvalidClientException` (HTTP 401)**: "the `clientId` or
`clientSecret` in the request is invalid … an incorrect `clientId` or an expired `clientSecret`".
It means the request carried a client AWS never issued. It is **never** a credential problem, so
do not go looking at tokens, refresh, or region.

Kiro's Builder ID and Identity Center flows both speak AWS IAM Identity Center OIDC
(`sso-oidc`, host `oidc.{region}.amazonaws.com`), and they run **three steps**:

1. `POST {oidc}/client/register` — AWS *issues* a `clientId` + `clientSecret`. Required members
   are **`clientName` and `clientType`** (only `public` is supported); optional `scopes`,
   `grantTypes`, `issuerUrl`, `redirectUris`. **Every member is camelCase.** There is no
   `client_id`, `client_name`, `client_type`, `grant_types`, `token_endpoint_auth_method` or
   `expires_at` member — those were all invented here at one point. The response is camelCase
   too: `clientId`, `clientIdIssuedAt`, `clientSecret`, `clientSecretExpiresAt`, plus
   `authorizationEndpoint` / `tokenEndpoint`.
2. `POST {oidc}/device_authorization` — `{clientId, clientSecret, startUrl}`, where `startUrl`
   is the **AWS access portal** (`https://view.awsapps.com/start` for Builder ID). Returns
   `deviceCode`, `userCode`, `verificationUri`, `verificationUriComplete`, `interval`, `expiresIn`.
3. `POST {oidc}/token` — `{clientId, clientSecret, deviceCode, grantType:
   "urn:ietf:params:oauth:grant-type:device_code"}`. `authorization_pending` / `slow_down` /
   `expired_token` / `access_denied` come back **inside a 200 body**, so this call must not
   status-check. The credentials from step 1 must be persisted and replayed here, or step 2 will
   fail again.

**The bug, and the reason it kept coming back.** `start_device_code_compat`
(`src/server/api/oauth.rs`) gated its Kiro branch on
`let is_kiro = provider == "kiro" && query.start_url.is_some();`. `web/…/OAuthModal.tsx` only
sends `start_url` for the **IDC** tab; AWS **Builder ID** sends none, because its start URL is
implied by the registration. So Builder ID fell through to the *generic* provider branch, which
has no client id for kiro (`providers::kiro().client_id` is `""`) and substituted the literal
string **`"zeroproxy"`**. AWS has no such client → 401 → the modal's red "Invalid client
provided". The working registration code was in the same function, one `&&` away, unreachable.
Fixed 2026-09-28 to `provider == "kiro"`. **If you ever see that `start_url` condition come
back, you have reintroduced this bug.**

The same class of bug existed a second time in `kiro_register_client()`
(`src/oauth/mod.rs`, the `POST /api/oauth/{provider}/device_code` route): it sent the snake_case
body above, then grepped the response for `client_id`/`client_secret`, and rescued both misses
with `.unwrap_or(&client_id)` — **a locally invented `zeroproxy-<uuid>`** — and
`.unwrap_or_default()` (an empty secret). That silent fallback is what turned a bad request
into an opaque upstream error instead of a local one. **A missing `clientId`/`clientSecret` is
now a hard error; never substitute a placeholder for a credential the server must issue.**

Rules that follow, because both bugs were the same mistake:
- **Never invent a client id.** It must come from `RegisterClient`, or the call is meaningless.
- **The three call sites share one body.** `device_code::kiro_registration_body()` and
  `device_code::parse_kiro_registration_response()` in `src/oauth/mod.rs` are the single source
  of truth for the AWS parameters and the response, used by both routes. Copying the JSON into a
  second handler is how the two copies drifted apart in the first place.
- **`scope` on `/token` is a no-op** — the access token always carries every scope from
  registration. Do not try to narrow it there.
- **Registered credentials expire** (`clientSecretExpiresAt`, 3600s here). Re-register rather
  than caching them; the dashboard registers per connect attempt, which is why that is fine.
- **A THIRD copy of the AWS device-code flow used to live in `src/oauth/kiro.rs`** — a second
  `register_client` / `start_device_authorization` / `poll_device_token`, with zero callers, next
  to the two broken copies that caused this whole saga. **Deleted 2026-09-30**, which is what
  makes "there must be exactly one implementation" checkable by reading the tree. What remains in
  that file is `oidc_base_url` (which delegates to `mod.rs`), the social/import/external-IdP
  flows, `normalize_kiro_external_idp_auth`, and the `KiroAuthMethod` enum. **Do not re-add a
  device-code implementation there** — and note the module doc used to be actively wrong about
  this, listing BuilderId/Idc as "supported in this module" when the device flow lives in
  `mod.rs` + `server/api/oauth.rs`. If you audit "is this flow wired?", the single source of
  truth is `kiro_registration_body()` + `parse_kiro_registration_response()` in `mod.rs`.
- **Open question, deliberately not "fixed":** we send `issuerUrl =
  https://identitycenter.amazonaws.com/ssoins-722374e8c3c8e6c6` while the Kiro IDE appears to
  send `https://view.awsapps.com/start`. AWS documents `issuerUrl` as optional and does not
  constrain `startUrl` against it, and the flow works, so changing it on a hunch is how this
  gets "fixed" into a new bug. Leave it unless a real 4xx points at it.

**Verify a Kiro connect end-to-end, not by inspection** — the failure is invisible to every
local gate, because both bugs are runtime-only and the gate never calls AWS:
`./scripts/dev.sh --fast detach` then
`curl -s 'localhost:4623/api/oauth/kiro/device-code' | jq '{user_code, verification_uri}'`
must return a real user code. A `clientId` of `zeroproxy` or an empty secret in that path is the
bug, even if the tests are green.

### 17. `PORT=4631` is NOT a multi-instance escape — dev.sh stops the shared unit anyway
This line previously told agents `PORT=4631 ./scripts/dev.sh` was a "sanctioned multi-instance
escape". **That is false for any run that starts a server**, and it cost a live instance.

**Measured (2026-09-28):** `PORT=4631 ./scripts/dev.sh --full detach` stopped the
`zeroproxy.service` unit (pid 1214, the maintainer's 4623 instance) and left nothing on 4623. The
`PORT` value is read when *binding*, but the startup path stops the unit / whatever holds the port
**regardless of `PORT`**. Recovery: `timeout 600 ./scripts/dev.sh --fast detach` (default port).
No data lost — the sqlite file is not touched by a stop.

**So `PORT=` is a way to bind a second listener, not a way to isolate yourself.** What is safe:

| Run | Safe? | Why |
|---|---|---|
| `--check` | yes | lint only, starts nothing |
| `--backend-only`, `--web-only` | yes | build only, starts nothing |
| `--fast detach`, `--full detach`, `run` | **no** | stops the shared unit first |

**Run the gate as cargo directly:** `cargo test --lib --all-features` ·
`cargo clippy --all-targets --all-features` · `cargo fmt --check`. The web half of the gate
(`cd web && pnpm install --frozen-lockfile && pnpm run build`, vitest, `astro check`) touches only
`web/` and is safe from a worktree.

**Related worktree gotcha (same day, same cause class):** `./scripts/setup-hooks.sh` **cannot be
run from a worktree.** It does `mkdir -p .git/hooks`, but `.git` is a regular *file* inside a
worktree, so `set -euo pipefail` aborts — loudly, but it installs nothing. Git shares hooks across
worktrees through the common dir, so install there instead:
`cp .githooks/* "$(git rev-parse --git-common-dir)"/hooks/ && chmod +x "$(git rev-parse --git-common-dir)"/hooks/*`
Always verify a hook against the **installed** bytes, never the committed file — a stale installed
copy already bit this repo once (it blocked a legitimate push).

### 18. Env vars are `ZEROPROXY_*` but a blind rename silently drops the legacy fallback
Every environment variable was renamed `CIPHERROUTE_*` → `ZEROPROXY_*` on 2026-09-28. **The old
name is still honoured**, deliberately, so an existing shell profile keeps working.

- **Read a ZeroProxy env var through `crate::core::env::var` / `::var_os`, never
  `std::env::var` directly.** `src/core/env.rs` tries the new name, then rewrites the brand
  token to the legacy spelling and retries. `var` keeps `std::env::var`'s exact signature
  (`Result<String, VarError>`) so the ~35 existing `.ok()` / `.and_then(|v| v.parse().ok())`
  call sites needed a one-token path change instead of a rewrite. A bare `std::env::var`
  "works" and silently loses backward compatibility — this is the failure mode to look for
  in review, and it already happened once (five sites in `src/cli/config.rs`).
- **`legacy_name` is a SUBSTRING rewrite, not a prefix strip.** That is what makes
  `JCODE_ZEROPROXY_API_KEY` correctly fall back to `JCODE_CIPHERROUTE_API_KEY`. Do not
  "simplify" it to `key.strip_prefix(BRAND)` — that silently breaks the Jcode key.
- **`JCODE_CIPHERROUTE_API_KEY` is permanently exempt from the rename** (5 sites in
  `src/server/api/cli_tools/jcode_settings.rs`, 2 in `web/src/components/cli-tools/JcodeToolCard.tsx`).
  It is not an environment variable at all: it is a **key name written into a generated
  `jcode.toml`** and read back from the file already on disk. Renaming it orphans the
  maintainer's existing config. The bulk rename used the negative lookbehind
  `(?<!JCODE_)CIPHERROUTE_` for exactly this reason — keep it if you ever re-run it.
- **The new name wins when both are set.** That is also how you override an inherited
  legacy value.
- **clap has NO dual-env support — `#[arg(env = "...")]` takes exactly one name.** So the
  five clap-backed globals (`--profile`, `--url`, `--api-key`, `--web-dir`, `--no-open` in
  `src/cli/mod.rs`) are reconciled by `Cli::apply_legacy_env`, called from
  `src/main.rs:33` right after `Cli::parse()`. Its rule is **not** the helper's rule: the
  legacy value applies only when the new variable is **entirely absent from the
  environment** (`std::env::var_os(key).is_some()`), *not* when the parsed field merely
  looks empty. Checking the field instead would let `ZEROPROXY_NO_OPEN=false` be
  overridden by `CIPHERROUTE_NO_OPEN=true`, inverting precedence.
- **A test that needs a variable to be ABSENT must clear BOTH spellings.**
  `core::env::var` falls back, so removing only the new name leaves the legacy one live and
  the test silently starts depending on the developer's shell. `src/cli/config.rs`'s
  `clear_env()` and `tests/sync_cli.rs` both do this deliberately, with a comment saying so.
  **A test that SETS the new name needs only the new name** — setting it shadows any
  inherited legacy value, so there is nothing to isolate (`tests/proxy_pools_api.rs`'s
  `VercelApiEnvGuard` is the correct counterexample). Match the rule to whether the
  variable must be absent, not to a blanket "clear both".
- **One test is intentionally left on the legacy name**:
  `tests/oauth_kiro_device_code_api.rs` `kiro_device_code_defaults_match_cipherroute_builder_id_flow`
  sets `CIPHERROUTE_KIRO_OIDC_BASE_URL`. Every sibling was renamed. That one is the only
  end-to-end proof the production fallback works, so **do not "tidy" it.**
- The **dashboard** has the same pattern, already shipped and independent of this one:
  `web/src/lib/brandMigration.ts` exports `ZEROPROXY_*` / `LEGACY_*` key pairs with the rule
  *read new-then-legacy, write new only*. Every lowercase `cipherroute` left in `web/src`
  is an intentional legacy key — it is a localStorage key, not an env var.
- Deliberately **out of scope**: lowercase `cipherroute` as the product name in prose
  (`README.md`, historical `docs/parity-9router*.md` audits, `web/package.json`'s
  description, `web/public/i18n/` strings where the English text IS the lookup key). That
  is a branding change, not the env-var prefix, and mass-renaming i18n values without
  their keys breaks translation lookups.

### 19. A snapshot provider's alias need not match the built-in catalog's — and sync deduped on the wrong one
There are **three** alias key spaces, and they are not interchangeable:

| space | built from | key → value |
|---|---|---|
| `provider_catalog.json` `providerIdToAlias` | our own data | provider **id** → short alias (`antigravity` → `ag`) |
| `provider_catalog.json` `providerModels[].alias` | our own data | short alias → the built-in model list |
| the snapshot's own `provider_id_to_alias` | upstream | upstream's id → upstream's alias |

`src/cli/sync.rs::plan_sync` built one map, `existing_ids`, from **two** of them — every existing
custom model's `provider_alias` *and* each built-in `entry.alias` — then looked models up under
**only the snapshot's alias**. When the two disagree, the built-in half of the map is unreachable
under that key and the dedup silently misses.

**Measured on the embedded OmniRoute snapshot (270 providers), the 7 priority providers that
disagree:**

| provider id | catalog alias | snapshot alias | snapshot models |
|---|---|---|---|
| `antigravity` | `ag` | `antigravity` | 10 |
| `agentrouter` | `ar` | `agentrouter` | 3 |
| `kilo-gateway` | `kgw` | `kg` | 6 |

`antigravity` is the live damage: **8 of its 10 snapshot model ids already ship in the built-in
catalog** (`gemini-3.7-flash-high`, `gemini-pro-agent`, `claude-sonnet-4-6`, …), so a full
`sync omniroute` re-imported every one of them as a second custom model under the snapshot alias.
Nothing in any gate can see this — the plan is well-formed, the write succeeds, the model simply
exists twice.

Fixed by making the lookup fall back to the catalog's alias for that provider id
(`static_alias_for_provider(&provider.id)`), and pinned by
`cli::sync::tests::sync_does_not_reimport_a_builtin_model_when_the_snapshot_alias_differs`, which
fails on the unfixed code and carries an `assert_ne!` that **invalidates itself** if upstream ever
reconciles the two spellings. `sync omniroute --free-only` is unaffected today because all three
providers are `free: None` and get skipped — which is exactly why this sat unnoticed.

Two things deliberately **not** concluded from this, after both were checked and found wanting:
- **A short alias alone is not proof of a bad row.** `resolve_provider_alias`
  (`src/core/model/mod.rs:162`) resolves against `ALIAS_TO_PROVIDER_ID`, a *hand-maintained static*
  — a fourth space, not derived from the catalog. The snapshot also ships its own
  `provider_id_to_alias`, so a snapshot alias has a resolution path independent of ours. Do not
  assert a model is "unroutable" from alias shape alone; trace the lookup first.
- **The custom-model key is `providerAlias`, not `provider`.** `SELECT
  json_extract(value,'$.provider') … GROUP BY` returns 0 rows against a 1601-row `kv` table — the
  path is simply absent. Aggregate on `$.providerAlias` or the measurement is a false zero.

Related: catalog aliases are **local, stable, and user-visible** (combos, `disabledModels`, the
picker reference them), so do not "fix" a mismatch by renaming a catalog alias. Reconcile at the
sync boundary, where this now happens.

### 20. A successful health probe did not clear the error fields — a recovered provider still showed a stale error forever
`src/core/health/daemon.rs`. The health daemon writes `healthStatus` / `healthCheckedAt` /
`degradedUntil` into `conn.extra` on every transition, but it **never touched `error_code`,
`last_error`, `last_error_at`, `consecutive_errors`, or `backoff_level`.** Those were cleared only on
**request-success** paths — `chat.rs:3353`, `usage.rs:1261`, `web_fetch.rs:741`, `oauth.rs:1390`,
`credential_manager.rs:454`/`527`, `proxy/mod.rs:289` — all of which run the same 5-field clear.

**Measured on the live db (2026-09-30), `agentrouter`:** `errorCode: 503`, `lastErrorAt:
2026-09-22T18:21:57`, `consecutiveErrors: 1`, `backoffLevel: 1` — while `healthStatus: "healthy"`
from a `2026-09-29T22:32` probe. The 503 was AgentRouter's own *"当前分组 default 下对于模型 glm-5.3
无可用渠道"* (no channel for that model), correctly answered with a `modelLock_glm-5.3`. Nothing was
broken; the row just carried an 8-day-old error next to a 1-day-old green health dot, and **the
dashboard has no way to know they are different eras.** `/health` reported that provider
`healthy: 1` at the same moment `/api/providers` reported `errorCode: 503`.

**This misled me personally.** I first reported it to the user as "agentrouter has a live 503" and
had to walk it back after reading the timestamps. That is the argument for the fix: the defect
manufactures a false current error, and it will keep doing so to whoever reads that field next.

Fixed by `clear_sticky_error` (called from `persist_records` when `!record.status.is_failure()`),
whose field set is copied from the request-success clears so the two cannot drift. Three things
worth not re-deriving:
- **A probe IS proof of life.** `probe_connection` (`probe.rs:80`) sends the connection's own
  credential — the auth header is attached to a GET of the provider's `/models` sibling — so a 2xx
  is exactly as good as a successful chat request. Without that fact this looks like an unjustified
  widening of what a background daemon may clear.
- **`needs_persist` is a change-detector, so it needed a second trigger.** A connection that
  recovers *without* a status change was never written, so the clear would only ever run on the one
  tick where the status flipped, and any later request-path error would stick again. The extra
  `has_sticky_error(conn)` clause is what makes the fix hold; it looks redundant next to the status
  comparison and is not.
- **`test_status` is deliberately NOT cleared.** It holds the result of a user-initiated "test this
  connection" action, which a background probe must not overwrite — that is why the live
  `agentrouter` row shows `testStatus: unavailable` beside `healthStatus: healthy`, and both are
  correct.
- Only **API-key** connections are probed at all (`tick_inner` filters on `is_api_key_auth`; the
  module doc explains OAuth liveness checks would burn subscription quota). So an OAuth-only stale
  error is still not cleared by this fix.

4 tests in `src/core/health/tests.rs`, RED-proven by making the clear a no-op and short-circuiting
the `needs_persist` clause: exactly the 2 tests targeting those 2 edits failed. The other 2 hold
either way by design — one pins that a *clean* healthy connection is not rewritten every tick
(without it the daemon would write on every probe forever), the other pins that `has_sticky_error`
is a complete disjunction, so "simplifying" it cannot strand a connection holding only
`backoff_level`.

## Invariants (must not break)

1. **Capability filter before routing.** `HARD_CAPS = ["vision","pdf","audioInput","videoInput"]`
   (`src/core/combo/mod.rs:446`). `detect_required_capabilities` (`:450`) runs *before*
   `reorder_by_capabilities` (`:711`), which tier-sorts then falls back. A hard-cap mismatch
   **skips the model entirely** — it is not a fallback trigger. See Trap 3: the constant and
   `model_has_capability` are single-source and guarded.
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
- **Persistence**: SQLite (WAL) + AES-GCM encrypted credential columns (`src/db/`).
  `encrypt_connection`/`decrypt_connection` cover `access_token`, `refresh_token`, `id_token`,
  `api_key`, **and `provider_specific_data`** (whole-map ciphertext under a single
  `__zeroproxy_psd_encrypted__` key, so it inherits the same `opxenc2:` versioning and fail-closed
  semantics; a legacy plaintext map is passed through untouched).
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
- Logs: `$DATA_DIR/zeroproxy.log` (`src/cli/server.rs:197`) — **not** `log.txt` — or
  `journalctl --user -u zeroproxy -f`.
- Release build: `cargo build --release --locked --no-default-features && ./target/release/zeroproxy --web-dir ./web/dist`.

## Testing

- **Only Linux actually runs the tests.** The `rust` job matrixes `[ubuntu-latest, macos-latest]`,
  but its test step is `- name: cargo test (Linux only)` / `if: runner.os == 'Linux'`. So **macOS
  proves fmt + clippy only**, and a green macOS run says nothing about tests. Do not read a macOS
  pass as "tests pass" — that misreading happened here once already.
- **The gate is green: `cargo test --lib --all-features` → 1986 passed, 0 failed.** Keep it that
  way; a red merge is not worth landing, because it destroys the only thing that makes the gate
  worth having. **Count it, do not recall it.** This line has now been wrong seven times (1905 →
  1913 → 1936 → 1947 → 1950 → 1954 → 1987 → 1994), because the recorded number is the count *at the moment the suite was last
  run* and every test-adding commit silently invalidates it. A branch whose diff touches no `.rs`
  file must produce the same number; if it does not, this baseline is stale. Re-measure and correct
  it whenever the count moves. **It can also legitimately go *down*** — deleting dead code drops
  dead tests, and that is not a regression: 1994 → 1986 was 4 dead tests in `src/oauth/kiro.rs`
  plus a net 4 from deleting `src/core/combo/capabilities.rs` (5 tests gone, 1 relocated into
  `combo/mod.rs`). Compare the *delta against your own diff*, not against the last recorded number.
- Integration tests under `tests/` are intentionally excluded (their build was repaired separately
  — they now compile, but their pass rate is unmeasured), so a green local `cargo test` on `tests/`
  is *not* the gate.
- Unit tests live in `src/**` as `#[cfg(test)]` modules (~224 files) — this is why `--lib` is
  the CI gate; parity locks need no network.
- Web: `pnpm --dir web test` (vitest, **4** suites: `analyticsFormatting`, `availableModels`,
  `providersPage`, `chatStream` — 71 tests. `brandMigration` is a 5th, added by the 2026-09-28
  brand sweep).
- `astro check` is **advisory** everywhere (`|| true` in CI, `|| echo advisory` in dev.sh) — fix
  new errors, don't chase the existing backlog.
- `cargo clippy --all-targets --all-features` — no `-D warnings`; it fails only on real errors.
- `scripts/parity-smoke.sh` is the intent, not the source of truth (see Trap 1).

## CI order (matters)

`lint-branch-name` → `web` (install → astro check* → vitest → `pnpm build` → upload `web/dist`)
→ `rust` (downloads `web/dist`, then fmt → clippy → `cargo test --lib`). The Rust job
**depends on the web artifact** because `build.rs` needs `web/dist/index.html`.

## Git hygiene

**Closing the loop is part of the task.** Any work that touched files ends in exactly one of four
states — uncommitted / committed-but-not-merged / PR-open / merged — and the report must say which
one, out loud, every time. Never end a report on changed code without stating it. And never treat
"it renders fine locally" as evidence of any of them: a `--web-dir` server serves uncommitted code
exactly like committed code, which is how a finished feature once sat uncommitted for weeks. Read
**Trap 14** and follow its four-state table. Committing without being asked is *also* an error — ask
once, then act.

Install hooks once per clone: `./scripts/setup-hooks.sh` (copies `.githooks/*` → `.git/hooks/`).
Re-run after pulling hook changes.

**Always pass `--repo atheerium/ZeroProxy` to every `gh` command.** Bare `gh pr <n>` resolves
against `upstream` (`quangdang46/openproxy`), and this repo and upstream both have a PR #18 — so
`gh pr merge 18` would act on *upstream's* PR 18. Measured, not theoretical; see Verification
traps. The `github_*` MCP tools take an explicit owner/repo and are immune.

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

## Agent Orchestration — the maintainer's loop, and how to stay out of each other's way

**This repo is worked by multiple AI agents, often concurrently, and the maintainer holds no
project state in his head. Read this section before your first edit.**

### The maintainer's loop — what he does and what you do

| step | maintainer | you |
|---|---|---|
| 1 | asks for a feature | create branch + worktree, implement, run the gate, **stop** |
| 2 | tests it himself | idle. **Do not commit. Do not merge.** |
| 3 | says "it works, commit and merge" | run the gate again, commit, open a PR, **stop** |
| 4 | merges, or asks you to | merge, return to `main`, clean up |

**Steps 1 and 2 are where agents go wrong.** You are NOT authorised to commit at the end of an
implementation, and you are NEVER authorised to merge to `main` without being asked in that
conversation. A commit is cheap to add later; an unwanted merge to `main` is expensive and has
happened here. If you finish a feature and nobody has tested it, say so and stop.

### One agent = one branch = one worktree. Never share a branch.

```bash
cat .opencode/claims/* 2>/dev/null; git worktree list   # ALWAYS check who else is live
git worktree add ../wt-<agent>-<slug> -b <agent>/<type>/<kebab>
cd ../wt-<agent>-<slug> && ./scripts/dev.sh              # build + run, detached, non-blocking
```

Branch names must match `^([a-z0-9._-]+/)?(feat|fix|docs|chore|refactor|test|build|ci|perf)/[a-z0-9._-]+$`
or be `main|dev|pr-*`. The pre-push hook enforces this. Never `--no-verify`.

### ⭐ Port 4623 is the #1 conflict between two concurrent agents

Only **one** server can hold `:4623`. A second agent starting one will either fail to bind or —
worse — **quietly kill or be killed by the first agent's**, leaving the maintainer testing a
binary he did not expect.

**Use a private port for anything that is not the maintainer's test instance — but
`PORT=` does NOT make every dev.sh run safe (measured 2026-09-28, see below):**

```bash
PORT=4631 ./scripts/dev.sh --check          # SAFE: lint only, starts nothing
PORT=4631 ./scripts/dev.sh --backend-only   # SAFE: builds only, starts nothing
PORT=4631 ./scripts/dev.sh --fast detach    # NOT SAFE — see the warning below
```

**⚠️ `PORT=4631` does not stop dev.sh from killing the maintainer's 4623 instance.** Any dev.sh
run that *starts* a server (including `--full detach`) stops the `zeroproxy.service` unit — or
whatever holds the port — as part of its own startup, and it does that **regardless of `PORT`**.
Measured: `PORT=4631 ./scripts/dev.sh --full detach` took down the maintainer's instance (pid 1214)
and left nothing on 4623. Recovery was `timeout 600 ./scripts/dev.sh --fast detach` (default port);
no data was lost. `PORT=` is therefore only a way to *bind* a second listener, **not** a way to
isolate yourself from the shared unit.

**Run the full gate as cargo directly, not through dev.sh:**
`cargo test --lib --all-features` · `cargo clippy --all-targets --all-features` ·
`cargo fmt --check`. dev.sh additionally runs the web layer (`astro check`, `pnpm build`, vitest),
which are safe to run from a worktree on their own — `cd web && pnpm run build` touches only `web/`.

Rules that follow from this:
- **The default `PORT=4623` belongs to the maintainer.** Do not stop it to free a port.
- To stop **your own** server, resolve the pid from the port's listener — never `pkill zeroproxy`
  and never `pkill -x zeroproxy`. Both have killed the maintainer's instance here. Use:
  `ss -tlnp | grep "127.0.0.1:$PORT" | sed -n 's/.*pid=\([0-9]*\).*/\1/p'` then `kill <pid>`.
- Before you start or stop anything, run `./scripts/dev.sh --doctor`. It reports which build
  layers are stale **and** whether the serving process actually has `--web-dir` (a green `/health`
  cannot tell you that — see Trap 4c).

### Before you touch anything shared

`src/server/api/chat.rs`, `web/src/shared/constants/providers.ts`, and
`src/core/model/provider_catalog.json` are **hot files** — two features touching the same one will
conflict on merge. Check `git status` in the main worktree first: if another session is mid-flight
you will see its dirty files, and you must not overwrite, revert, or commit them.

**Never move or rewrite a git ref while another session is live in a worktree.** Only ref-only
operations are safe (`git fetch origin main:main` when `main` is not checked out). Deleting a
worktree kills any server running from it.

### Finishing a feature

```bash
./scripts/dev.sh --full detach        # the real gate: fmt + clippy + astro + tests
git status --porcelain                # MUST be empty before you report done
git log --oneline main..HEAD          # your commits, countable
```

Then report: the branch name, the commit shas, the gate results, and **explicitly that it is not
merged**. Uncommitted work is not saved (Trap 14) — a finished analytics feature once sat
uncommitted in a worktree for a month and was nearly lost.

### Recovering from a confused repo

```bash
git worktree list                          # what exists
git for-each-ref --format='%(refname:short) %(upstream:short)' refs/heads
git rev-list --left-right --count main...origin/main   # 0  0 means synced
```

If `git branch -d` refuses with "not fully merged", that is often **wrong** — it only asks whether
the local tip is merged into its *own upstream*, which trips when you are ahead. Prove it with
`git merge-base --is-ancestor <branch> main` and use `-D` if that says yes.

Full spec + recovery: `docs/agent-orchestration.md` (tracked). Note it still contains a few
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
