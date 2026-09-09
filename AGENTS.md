# ZeroProxy — Rust AI Proxy Router

## 7 Questions Every Agent Asks

| Q | A | Deep-dive |
|---|---|---|
| What is this binary | OpenAI-compatible Rust proxy routing to 40+ providers with format translation, fallback, token refresh, usage tracking, SSE | [Architecture](docs/ARCHITECTURE.md) · [Routing engine](docs/ROUTING.md) |
| How do I build/test | `./scripts/dev.sh --fast detach` (rebuilds stale `web/dist` + cargo, restarts `:4623`) | [Dev workflow](#dev-workflow) · `CONTRIBUTING.md` |
| How do I add a provider | Add entry to `provider_catalog.json` + `src/core/executor/default.rs` registry; check OmniRoute first | [Providers](docs/PROVIDERS.md) |
| How does routing work | Parse model → capability detect → capability-aware order → round-robin/fallback → provider | [Routing](docs/ROUTING.md) |
| Where does a combo come from | `src/core/combo/mod.rs` — ordered list of `provider/model` pairs, scored by strategy | [Routing](docs/ROUTING.md#combos) |
| How is auth refreshed | `src/oauth/token_refresh.rs` — 401/403 → `dispatch_oauth_refresh`; 404 → 300 s model lock | `src/core/combo/mod.rs:576-603` |
| How is usage tracked | `src/core/usage/tracker.rs` → SQLite `UsageDb.history`; SSE `/api/usage/stream` | `src/server/api/usage.rs:244` |

## Invariants (must not break)

1. **Capability filter before routing** — `HARD_CAPS=["vision","pdf","audioInput","videoInput"]` (`src/core/combo/mod.rs:283`); `detect_required_capabilities` (`src/core/combo/mod.rs:289-296`) runs before `reorder_by_capabilities` (`src/core/combo/mod.rs:560-572`), which tier-sorts (tier0/tier1/tier2) then falls back. Hard-cap mismatch skips a model entirely.
2. **`context_window` cap** — `combo.max_context` in design = max `context_window` of members (`provider_catalog.json`); strip history via `strip_history_for_context` (`src/server/api/chat.rs:646-652`) only for capacity adapter-added models, budget = `(context_window || 200_000)*0.8*4`.
3. **Fallback only eligible** — `check_fallback_error` (`src/core/combo/mod.rs:576-603`) delegates to `error_config::classify_error` → `ErrorClassification::{Backoff,Cooldown,NoMatch,Permanent}`; `retryAfter` header wins over body; 401/403 → single `dispatch_oauth_refresh`; 404 → 300 s model-specific lock.
4. **Error classification single source** — always route through `error_config::classify_error`; do not re-implement status→fallback logic in executors.

## Core: What / Why / How

Zeroproxy is an AI proxy router written in Rust — OpenAI-compatible endpoint that routes requests to 40+ AI providers with format translation, account fallback, token refresh, usage tracking, and SSE streaming.

**Why**: Replace 9router (Node.js) with a faster, safer Rust implementation (235+ JS bugs avoided). Type-safe format handling, encrypted secrets, immutable data flow, thread-safe by design.

**How (pipeline)**: `model parsing → format detection → request translation → capability-aware ordering → provider execution → response translation → SSE streaming`

- **Account mgmt**: credential selection → token refresh → model-level fallback → combo/fusion
- **Executor trait**: `ProviderExecutor` with default+specialized impls (`src/core/executor/mod.rs`)
- **Persistence**: SQLite WAL + encrypted columns (`src/db/`)
- **Security**: HMAC API keys, bcrypt auth, SSRF protection (`src/server/auth/`)

## Guiding Principle — Lightweight OmniRoute Clone

Zeroproxy is a stripped Rust clone of OmniRoute — **avoid OmniRoute's bloat**. When any feature's objective is unclear, **consult `~/dev/OmniRoute` (fallback `/tmp/omniroute_v3850`) first** — every feature already exists there in some form.

**Lookup order**: `~/dev/OmniRoute/src/` + `~/dev/OmniRoute/open-sse/` → `/tmp/omniroute_v3850/src/managed/`

**Out-of-scope (do NOT port):**
- MCP servers, A2A/ACP protocols, Electron/PWA/VNC, memory/skills frameworks, analytics beyond minimal usage, cloud/sync backends, Telegram bots, chaos engineering. See `docs/OMNIROUTE_PROVIDER_PARITY.md`.
- Header-fidelity parity: OpenRouter sends `HTTP-Referer`+`X-Title`; nvidia/llm7 omit `HTTP-Referer`; gemini sends `x-goog-api-key`; kiro/opencode/free-providers use OAuth token flow.

## Core Product Surfaces (TOP PRIORITY)

These 4 surfaces ARE the product. Always prioritize regressions + improvements here:

1. **Providers page** — `/dashboard/providers/<provider>`: toggle Available Models (disable/enable/custom), persisted in SQLite, survives rebuilds.
2. **CLI tools config** — `/dashboard/cli-tools/opencode`.
3. **Combos page** — `/dashboard/combos`.
4. **`ModelSelectModal.tsx`** — the single model-picker used everywhere; **must mirror** the provider page's Available Models (same disabled map + custom rows + catalog merge). Any model-list logic change applies to both.

Core workflow that must never break: configure provider → customize available models → create combos → select models for opencode CLI config.

## Dev Workflow — backend + dashboard

Single smooth loop — backend and dashboard are **separate builds** served by the same binary. Run `./scripts/dev.sh` from repo root only.

### How web assets are served

**`--web-dir` mode (default in dev.sh)**: The server reads `web/dist/` from disk at runtime (`--web-dir web/dist`). Web rebuilds are instantly visible — **no binary rebuild needed**. This is the permanent solution to the stale-embedded-assets problem.

**Embedded mode (release builds)**: `web/dist/` is baked into the binary via `rust-embed` (`#[folder = "web/dist/"]`). Requires `cargo build` to pick up web changes. Used for single-binary distribution only.

> **Never run the release binary for dev work** — it has stale embedded assets. Always use `dev.sh` which adds `--web-dir`.

**Presets:**
| Change | Command | Notes |
|---|---|---|
| Only `web/src` | `--web-only` or `--fast detach` | Rebuild web, restart (no cargo needed) |
| Only `src/` | `--backend-only detach` or `--fast detach` | cargo build + restart |
| Both | `--fast detach` | ~10-20s |
| Before push | `--full detach` | web + cargo + fmt/clippy/astro/tests |
| Stale `web/dist` suspected | `--web-only` or `--full` | Forces `pnpm build` |
| Lint only | `--check` | No build |

After ANY backend change: `./scripts/dev.sh --fast detach && curl http://127.0.0.1:4623/health`. Never report a fix done without rebuild+restart — stale binary is the #1 silent regression source.

Full flags: `./scripts/dev.sh --help`. Raw web: `cd web && pnpm dev` (Astro `:4624` with API proxy to `:4623`).

## Contributing & Git Hygiene

Two linked documents govern workflow: `CONTRIBUTING.md` (workflow/standards/testing) + `docs/git-conventions.md` (enforceable rules). PRs use `.github/pull_request_template.md`; CI (`.github/workflows/ci.yml`) checks `web` → `rust` on Ubuntu+macOS.

## Agent Orchestration — Branch-per-Agent, Worktree Isolation

**Rule: one agent = one branch = one worktree. Never share a branch.**

- **Branch-per-agent** (`<agent>/<type>/<kebab>`). Shared-branch edits caused 26-file stashes and cross-branch cherry-picks.
- **Worktree isolation**: `git worktree add ../wt-<agent>-<slug> -b <agent>/<type>/<kebab>`; claim with `../cipherroute/scripts/claim-branch.sh <branch>`. Hot files: `src/server/api/chat.rs`, `web/src/shared/constants/providers.ts`.
- **Claim file** `.opencode/claims/<branch>` — check before claiming.
- **Dirty-tree guard**: `scripts/dev.sh` warns on dirty tree; hooks block wrong-branch commits.
- **Hooks**: `scripts/setup-hooks.sh` installs pre-commit (fmt+secret scan), commit-msg (Conventional Commits), pre-push (branch+secret scan).
- See [`docs/agent-orchestration.md`](docs/agent-orchestration.md) for full spec + recovery.

## Beads / Status

Parity work: epic `cipherroute-9router-parity-v0550-pnc` (v0.5.50 → cipherroute, 122 specs). See `br ready` / `bv --robot-next`. Parity docs: `docs/parity-9router.md`, `docs/OMNIROUTE_PROVIDER_PARITY.md`.

Smoke test: `cargo test -p cipherroute --lib parity_tests stream_flags`.

## Schema Stability & Secrets

- `cipherroute.v1.*` envelope is **frozen, additive-only** — 13 resources (`provider`, `provider-node`, `combo`, `key`, `pool`, `settings`, `custom-model`, `model-alias`, `usage-event`, `log-event`, `chat-event`, `quota`, `oauth-status`), each schema+example enforced by tests. `cipherroute schema list/show/example/stability`.
- **Never commit** local config: `opencode.json`, `.env`, `*.pem`, `~/.cipherroute/db.json`, API keys (`sk-`/`Bearer`/`refresh_token`). Rotate immediately + purge history (`git filter-repo`) if accidentally committed.

