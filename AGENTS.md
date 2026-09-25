# ZeroProxy — Rust AI Proxy Router

# Repositories to refer to:
https://github.com/diegosouzapw/OmniRoute
https://github.com/decolua/9router
https://github.com/quangdang46/openproxy/
https://github.com/tashfeenahmed/freellmapi

dont reinvent a solution before looking into one of these repositories (in order).
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

- **CI runs `cargo test --lib --all-features` on Linux + macOS only.** Integration tests under
  `tests/` are intentionally excluded (3 known auth-helper failures), so a green local
  `cargo test` on `tests/` is *not* the gate.
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
