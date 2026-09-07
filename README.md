# ZeroProxy

<div align="center">
  <img src="zeroproxy_illustration.svg" alt="ZeroProxy — local AI router. 40+ providers. Auto-fallback.">
</div>

<div align="center">

![CI](https://img.shields.io/github/actions/workflow/status/atheerium/zeroproxy/ci.yml?branch=main)
![Release](https://img.shields.io/github/v/release/atheerium/zeroproxy?display_name=tag&sort=semver&label=release&color=brightgreen)
![License](https://img.shields.io/badge/License-MIT-blue.svg)
![Install](https://img.shields.io/badge/install-curl%20%7C%20npm-1e90ff)

</div>

**Single-binary AI router for AI coding tools.**  
Routes to 40+ providers with auto-fallback combos. Embedded dashboard, OpenAI-compatible API, RTK compression. Run on `127.0.0.1:4623` — no cloud required.

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#connect-a-cli-tool">Connect a CLI</a> ·
  <a href="#supported-providers">Providers</a> ·
  <a href="#combos-build-a-fallback-chain">Combos</a> ·
  <a href="#for-ai-agents">For AI Agents</a> ·
  <a href="#configuration">Configuration</a>
</p>

<div align="center">

```bash
# Start locally (auto-opens dashboard)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
zeroproxy
```

</div>

---

## 🤖 Agent Quickstart (API / Robot Mode)

Point your coding agent CLI at `http://127.0.0.1:4623`:

```bash
# Claude Code
claude config set proxyUrl http://127.0.0.1:4623

# Codex CLI
codex --proxy http://127.0.0.1:4623

# Any OpenAI-compatible tool
curl http://127.0.0.1:4623/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"claude/claude-opus-4-8","messages":[{"role":"user","content":"hi"}]}'
```

**Dashboard** — open `http://127.0.0.1:4623/` in browser for provider config, combo chains, quota tracking, and request logs.

---

## What it does

Zeroproxy runs as one binary on `127.0.0.1:4623`. Point any tool that speaks the OpenAI Chat Completions API at it (Claude Code, Codex, Cursor, Cline, OpenClaw, Copilot, ...) and Zeroproxy:

- routes the request to a provider you've configured (OAuth, API key, or free)
- falls back to the next provider in your combo when one is rate-limited or errors
- compresses tool-call results via [RTK](https://github.com/rtk-ai/rtk) before they hit the LLM (typical −20–40% input tokens on tool-heavy turns)
- tracks per-account quota so you can use subscription tiers fully before paying for API calls
- serves a local dashboard at `/` for configuration, monitoring, and account management

There is no cloud component required. All state lives in `~/.zeroproxy/` (SQLite database at `zeroproxy.sqlite`).

---

## Install

```bash
# Linux / macOS — x86_64 + aarch64
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
```

```powershell
# Windows (PowerShell 5.1+, x86_64)
irm "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.ps1" | iex
```

Both pull the same prebuilt binary from the same GitHub release. The Linux/macOS curl path drops the binary at `~/.local/bin/zeroproxy`. The Windows PowerShell path drops `zeroproxy.exe` at `%USERPROFILE%\.local\bin`.

```bash
zeroproxy
```

The server binds to `127.0.0.1:4623` and the dashboard auto-opens in your browser. Use `--no-open` for headless / SSH / container contexts.

<details>
<summary>Other install options</summary>

```bash
# Pin a version
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --version v0.1.0

# Install system-wide (may need sudo)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --system

# Add to PATH automatically (~/.bashrc / ~/.zshrc)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --easy-mode

# Build from source (requires cargo + Node 20 + pnpm)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --from-source

# Uninstall
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --uninstall
```

Manual download: https://github.com/atheerium/zeroproxy/releases

</details>

---

## Connect a CLI tool

Most tools ask for an OpenAI base URL and an API key.

| Tool | Setting | Value |
|---|---|---|
| Cursor / Cline / Continue / Roo / Kilo | OpenAI base URL | `http://127.0.0.1:4623/v1` |
| Codex CLI | `OPENAI_BASE_URL` | `http://127.0.0.1:4623` |
| Claude Code | `~/.claude/config.json` `anthropic_api_base` | `http://127.0.0.1:4623/v1` |
| OpenClaw | dashboard → CLI Tools → OpenClaw | one-click apply |

The API key comes from the dashboard. Visit `http://127.0.0.1:4623`, create an API key, paste it into the tool's settings.

Tested CLIs: **Claude Code, Codex, Cursor, Cline, Continue, Roo, Kilo, Copilot, OpenClaw, OpenCode, Antigravity, Droid**.

---

## For AI Agents

Zeroproxy is built to be driven by AI agents (Devin, Claude Code, Codex, Cursor, OpenClaw, …) end-to-end — install, init, configure, and verify without any browser interaction.

A ready-to-use agent skill ships in this repo:

- [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md) — install, `server init`, `server start --detach`, declarative `provider apply`, and wiring CLI tools.

The `install.sh` one-shot installer **automatically drops the same file at `~/.agents/skills/zeroproxy/SKILL.md`** so agents that scan the home directory (Devin, Claude Code, …) pick it up the moment you install zeroproxy. The installer preserves any user-edited skill file (detected via the `name: zeroproxy` frontmatter marker) and exposes two flags:

- `--no-skill` — skip the auto-install entirely.
- `--skill-dest <dir>` — write to a custom skills root (default: `~/.agents/skills`).

The CLI is agent-friendly by design:

- `--robot` emits stable line-delimited JSON envelopes (`zeroproxy.v1.*`, frozen contract — additive only).
- `zeroproxy schema list` / `schema show <resource>` exposes the JSON shape for every `apply`-able resource.
- `zeroproxy provider apply --from-file -` is declarative and idempotent (`--prune` to reconcile).
- `zeroproxy doctor` self-tests the install, data dir, and server reachability.

Minimal autonomous bootstrap (no TTY, no browser, no prompts):

```bash
# 1. Install (drops binary at ~/.local/bin/zeroproxy)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
export PATH="$HOME/.local/bin:$PATH"

# 2. Initialize data dir; capture the admin API key from the JSON envelope
zeroproxy --robot server init | tee /tmp/op-init.json
APIKEY=$(jq -r '.data.admin_key.key' /tmp/op-init.json)

# 3. Start server detached + headless, then self-test
zeroproxy server start --detach --no-open
zeroproxy --robot doctor
zeroproxy --robot server status

# 4. Configure a provider declaratively (idempotent)
cat > /tmp/providers.json <<JSON
{ "providers": [{ "name": "openai", "provider": "openai", "apiKey": "sk-...", "isActive": true }] }
JSON
CIPHERROUTE_API_KEY="$APIKEY" zeroproxy --robot provider apply --from-file /tmp/providers.json

# 5. End-to-end smoke test against the running server
curl -sS http://127.0.0.1:4623/health
curl -sS http://127.0.0.1:4623/v1/models -H "Authorization: Bearer $APIKEY"
```

Full walkthrough — including failure modes, OAuth provider notes, and post-install verification — lives in [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md).

### Copy-paste prompt for your AI agent

Paste the block below into Devin, Claude Code, Codex, Cursor, OpenClaw, or any agent chat. It's a single self-contained instruction set — the agent will install Zeroproxy, start it, and verify it works without asking further questions.

````text
You are setting up Zeroproxy (https://github.com/atheerium/zeroproxy) on this
machine. Zeroproxy is a single-binary AI router exposing an OpenAI-compatible
API on http://127.0.0.1:4623. Follow these steps exactly, stop on the first
error, and report the captured admin API key back to me at the end.

1. Check whether `zeroproxy` is already on PATH:
       command -v zeroproxy && zeroproxy --version || echo "not installed"
   If already installed, skip to step 3.

2. Install the binary. Pick ONE path:
   a) Linux / macOS, x86_64 or aarch64 (preferred):
          curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
          export PATH="$HOME/.local/bin:$PATH"
   b) Windows, x86_64 (PowerShell 5.1+):
          irm "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.ps1" | iex
          $env:Path = "$env:USERPROFILE\.local\bin;" + $env:Path

3. Initialize a data dir and capture the admin API key. Use --robot so the
   output is JSON; never re-run with --force without my explicit approval:
       zeroproxy --robot server init | tee /tmp/op-init.json
       APIKEY=$(jq -r '.data.admin_key.key' /tmp/op-init.json)
       export CIPHERROUTE_API_KEY="$APIKEY"
   If `server init` reports `zeroproxy.sqlite already exists`, STOP and tell me — the
   data dir is pre-populated and I need to decide whether to overwrite.

4. Start the server detached and headless, then self-test:
       zeroproxy server start --detach --no-open
       zeroproxy --robot server status
       zeroproxy --robot doctor

5. Verify end-to-end:
       curl -sS http://127.0.0.1:4623/health
       curl -sS http://127.0.0.1:4623/v1/models \
         -H "Authorization: Bearer $CIPHERROUTE_API_KEY"

6. Report back to me:
   - The exact `zeroproxy --version` output.
   - The admin API key (value of $CIPHERROUTE_API_KEY).
   - Result of step 4's `server status` and `doctor`.
   - Any non-2xx response from step 5.

Do NOT run `zeroproxy server init --force`, do NOT delete ~/.zeroproxy/, and
do NOT add provider API keys unless I gave you values explicitly. If you hit
the failure modes documented in
https://github.com/atheerium/zeroproxy/blob/main/.agents/skills/zeroproxy/SKILL.md
("Common failure modes & fixes"), apply the listed fix; otherwise stop and ask.
````

The same instructions in machine-readable form live at [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md) — agents that auto-discover `.agents/skills/` (Devin, etc.) will pick them up without any copy-paste.

---

## Supported providers

| Tier | Provider | Auth | Notes |
|---|---|---|---|
| OAuth subscription | Claude Code, Codex, GitHub Copilot, Cursor, Antigravity | OAuth (PKCE) | Use your existing subscription quota. Auto-refresh. |
| API key | OpenAI, Anthropic, Gemini, OpenRouter, GLM, Kimi, MiniMax, DeepSeek, Groq, xAI, Mistral, Perplexity, Together, Fireworks, Cerebras, Cohere, NVIDIA, SiliconFlow, Nebius, Chutes, Hyperbolic, custom OpenAI/Anthropic-compatible endpoints | API key | 40+ supported. |
| Free | Kiro AI (Claude 4.5 + GLM-5 + MiniMax), OpenCode Free, Vertex AI ($300 trial credits) | OAuth / no auth / GCP service account | Best for fallback tiers. |

Configure providers from the dashboard (`Providers` tab) or via `zeroproxy provider` CLI subcommands. Each provider supports multiple accounts; Zeroproxy round-robins between them.

---

## Combos: build a fallback chain

A combo is an ordered list of models. Zeroproxy tries them in order, falling back when one is rate-limited or errors. Models are addressed as `<provider-prefix>/<model-id>`:

```
combo: my-stack
  1. cc/claude-opus-4-7      # Claude Pro/Max subscription
  2. glm/glm-4.7             # paid backup ($0.6/1M)
  3. kr/claude-sonnet-4.5    # Kiro free fallback
```

Created from `Combos` in the dashboard or `zeroproxy combo create`. Use the combo name as the model field in your CLI tool — Zeroproxy resolves it.

---

## Configuration

Most operators only set `JWT_SECRET` and leave the rest at defaults. The dashboard password is a random value generated on first boot (see `INITIAL_PASSWORD` below).

| Variable | Default | Purpose |
|---|---|---|
| `JWT_SECRET` | `zeroproxy-default-secret-change-me` | Sign the dashboard session cookie. **Change in production.** |
| `INITIAL_PASSWORD` | _random, generated once_ | First-login password when no saved hash exists. When unset, a random password is generated at first boot and **printed once in the startup banner** (`$DATA_DIR/initial_password` is persisted so it stays stable). Reset it anytime with `zeroproxy auth reset-password`. |
| `DATA_DIR` | `~/.zeroproxy` | Where `zeroproxy.sqlite`, data, and logs live. |
| `PORT` | `4623` | HTTP listen port. |
| `HOSTNAME` | `127.0.0.1` | Bind host. Set `0.0.0.0` to expose on LAN. |
| `BASE_URL` | `http://localhost:4623` | Internal base URL for cloud-sync jobs. |
| `CLOUD_URL` | _unset_ | Cloud-sync endpoint. Leave unset to disable cloud sync. |
| `API_KEY_SECRET` | _auto-generated, persisted_ | HMAC secret for generated API keys. When unset, a random secret is generated at first startup and persisted to `$DATA_DIR/api_key_secret`. |
| `MACHINE_ID_SALT` | `endpoint-proxy-salt` | Salt for the stable machine-ID hash. |
| `AUTH_COOKIE_SECURE` | `false` | Force `Secure` flag on the auth cookie. Set `true` behind HTTPS. |
| `REQUIRE_API_KEY` | `false` | Reject `/v1/*` requests without `Authorization: Bearer …`. Recommended for any internet-exposed deploy. |
| `ENABLE_REQUEST_LOGS` | `false` | Write per-request logs under `logs/`. |
| `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` | _unset_ | Forward outbound provider calls through an HTTP proxy. Lowercase variants also honored. |

### TOML config profiles

For advanced setups (multiple Zeroproxy instances, remote server management) the CLI
reads an optional TOML profile file at `~/.config/zeroproxy/config.toml`
(`%APPDATA%\zeroproxy\config.toml` on Windows; override via `$CIPHERROUTE_CONFIG`).

```toml
default_profile = "work"

[profiles.work]
data_dir = "/home/me/proxy-data"
url = "https://proxy.example.com"
api_key_env = "MY_PROXY_KEY"        # read API key from this env var

[profiles.local]
data_dir = "/tmp/proxy-test"
```

Resolution precedence (highest first):
1. Explicit CLI flags (`--data-dir`, `--url`, `--api-key`, `--profile`, `--port`)
2. `CIPHERROUTE_*` environment variables
3. Selected profile (`--profile <name>` or `default_profile`)
4. Built-in defaults

Profiles are created programmatically by `zeroproxy auth login` / `zeroproxy auth logout`
— you do not normally need to hand-edit the TOML file.

### Compiled-in constants

Unlike 9router, the following are **not** hot-reloadable or config-file driven.
They are compiled into the binary and require a rebuild to change:

| Constant | Source file | Default |
|---|---|---|
| Stream stall timeout | `src/core/config/runtime_config.rs` | 360 s (6 min) |
| First-chunk timeout | `src/core/config/runtime_config.rs` | 200 s |
| Default max tokens | `src/core/config/runtime_config.rs` | 64 000 |
| Retry config (502, 503, 504) | `src/core/config/runtime_config.rs` | 2–3 attempts, 2–3 s delay |
| Provider catalog (models, aliases) | `src/core/model/provider_catalog.json` | ~70 providers, ~200 models |
| OAuth provider registry | `src/oauth/providers.rs` | 18 built-in OAuth configs |
| Gemini CLI version string | `src/core/config/app_constants.rs` | `0.34.0` |
| GitHub Copilot versions | `src/core/config/app_constants.rs` | Chat `0.38.0`, VS Code `1.110.0` |
| Default tool-name decoys | `src/core/config/app_constants.rs` | Claude Code / Antigravity tool sets |
| Kiro suffixes & system prompt | `src/core/config/kiro_constants.rs` | `-agentic`, `-thinking` |
| Thinking-mode signatures | `src/core/config/default_thinking_signature.rs` | Claude, AG, Vertex, Gemini CLI |

---

## CLI reference

```
zeroproxy [FLAGS]                  # default: start server + open browser
zeroproxy --port 4623 --no-open    # foreground, no browser
zeroproxy --web-dir ./web/dist     # serve dashboard from disk (UI dev)
zeroproxy --dashboard-sidecar-url http://127.0.0.1:4624
                                   # reverse-proxy dashboard requests to a dev server

zeroproxy --version
zeroproxy provider list
zeroproxy provider add <name> '<json-config>'
                                   # e.g. zeroproxy provider add openai-paid \
                                   #        '{"provider":"openai","apiKey":"sk-..."}'
zeroproxy combo create --name <name> --models cc/opus,glm/glm-5
zeroproxy key list
zeroproxy key add <name> <secret>  # provide your own secret
zeroproxy key add <name> --auto    # let zeroproxy mint a fresh `op-…` secret
zeroproxy quota list               # subcommands: list / get / reset / refresh
zeroproxy usage summary            # subcommands: summary / daily / chart / history / …
zeroproxy doctor                   # diagnose common config issues

zeroproxy server start [--detach] [--no-open] [--port P]
zeroproxy server status
zeroproxy server stop
zeroproxy server init              # mint the first admin API key

zeroproxy sync 9router [--dry-run] [--prune]
                                   # pull provider/model catalog from atheerium/9router
zeroproxy sync omniroute [--dry-run] [--prune]
                                   # pull provider/model catalog from diegosouzapw/OmniRoute
```

`zeroproxy sync` applies embedded snapshots of sister open-source routers
into `customModels` in the SQLite store, tagged with `source` so a later
`--prune` only removes entries it previously added. Maintainers refresh
the snapshots via `node scripts/sync/normalize-sources.mjs`
([scripts/sync/README.md](scripts/sync/README.md)).

`zeroproxy --help` prints the full reference. Subcommands have their own `--help`.

Output formats: human (default), `--robot` (line-delimited JSON for agent/automation use), `--quiet`.

---

## API

OpenAI-compatible chat completions:

```http
POST /v1/chat/completions
Authorization: Bearer <api-key>
Content-Type: application/json

{
  "model": "cc/claude-opus-4-6",
  "messages": [{"role": "user", "content": "..."}],
  "stream": true
}
```

List available models and combos:

```http
GET /v1/models
Authorization: Bearer <api-key>
```

Health probe (no auth):

```http
GET /health   →   200 OK
```

The dashboard at `/` is the same authenticated API surface in HTML form. Admin endpoints live under `/api/*` and use the dashboard session cookie.

# ZeroProxy

<div align="center">
  <img src="zeroproxy_illustration.svg" alt="ZeroProxy — local AI router. 40+ providers. Auto-fallback.">
</div>

<div align="center">

![CI](https://img.shields.io/github/actions/workflow/status/atheerium/zeroproxy/ci.yml?branch=main)
![Release](https://img.shields.io/github/v/release/atheerium/zeroproxy?display_name=tag&sort=semver&label=release&color=brightgreen)
![License](https://img.shields.io/badge/License-MIT-blue.svg)
![Install](https://img.shields.io/badge/install-curl%20%7C%20npm-1e90ff)

</div>

**Single-binary AI router for AI coding tools.**  
Routes to 40+ providers with auto-fallback combos. Embedded dashboard, OpenAI-compatible API, RTK compression. Run on `127.0.0.1:4623` — no cloud required.

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#connect-a-cli-tool">Connect a CLI</a> ·
  <a href="#supported-providers">Providers</a> ·
  <a href="#combos-build-a-fallback-chain">Combos</a> ·
  <a href="#for-ai-agents">For AI Agents</a> ·
  <a href="#configuration">Configuration</a>
</p>

<div align="center">

```bash
# Start locally (auto-opens dashboard)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
zeroproxy
```

</div>

---

## 🤖 Agent Quickstart (API / Robot Mode)

Point your coding agent CLI at `http://127.0.0.1:4623`:

```bash
# Claude Code
claude config set proxyUrl http://127.0.0.1:4623

# Codex CLI
codex --proxy http://127.0.0.1:4623

# Any OpenAI-compatible tool
curl http://127.0.0.1:4623/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"claude/claude-opus-4-8","messages":[{"role":"user","content":"hi"}]}'
```

**Dashboard** — open `http://127.0.0.1:4623/` in browser for provider config, combo chains, quota tracking, and request logs.

---

## Why ZeroProxy?

ZeroProxy is **the best of three worlds**:

| World | Contribution | Why it matters |
|-------|-------------|----------------|
| **9router** | Proven routing logic (fallback chains, combo resolution, quota tracking) | You get battle-tested fallback behavior that actually works across edge cases |
| **OmniRoute** | Rich provider ecosystem (40+ OAuth/API-key providers, dedicated executors, format translation) | Access to the full breadth of providers with smart per-provider optimizations |
| **freellmapi** | Zero-friction setup (plug API keys → working proxy instantly) | No configuration files, no YAML, no cloud. Just add keys and go |

**The result**: A production-ready, AI-powered proxy that's been battle-tested, feature-complete, and easy to use — all in a single Rust binary.

> "Rather than re-inventing, we chose to curate — taking the strongest foundations from each and making them work together seamlessly."

---

## What it does

ZeroProxy runs as one binary on `127.0.0.1:4623`. Point any tool that speaks the OpenAI Chat Completions API at it (Claude Code, Codex, Cursor, Cline, OpenClaw, Copilot, ...) and ZeroProxy:

- routes the request to a provider you've configured (OAuth, API key, or free)
- falls back to the next provider in your combo when one is rate-limited or errors
- compresses tool-call results via [RTK](https://github.com/rtk-ai/rtk) before they hit the LLM (typical −20–40% input tokens on tool-heavy turns)
- tracks per-account quota so you can use subscription tiers fully before paying for API calls
- serves a local dashboard at `/` for configuration, monitoring, and account management

There is no cloud component required. All state lives in `~/.zeroproxy/` (SQLite database at `zeroproxy.sqlite`).

---

## Install

```bash
# Linux / macOS — x86_64 + aarch64
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
```

```powershell
# Windows (PowerShell 5.1+, x86_64)
irm "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.ps1" | iex
```

Both pull the same prebuilt binary from the same GitHub release. The Linux/macOS curl path drops the binary at `~/.local/bin/zeroproxy`. The Windows PowerShell path drops `zeroproxy.exe` at `%USERPROFILE%\.local\bin`.

```bash
zeroproxy
```

The server binds to `127.0.0.1:4623` and the dashboard auto-opens in your browser. Use `--no-open` for headless / SSH / container contexts.

<details>
<summary>Other install options</summary>

```bash
# Pin a version
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --version v0.1.0

# Install system-wide (may need sudo)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --system

# Add to PATH automatically (~/.bashrc / ~/.zshrc)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --easy-mode

# Build from source (requires cargo + Node 20 + pnpm)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --from-source

# Uninstall
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash -s -- --uninstall
```

Manual download: https://github.com/atheerium/zeroproxy/releases

</details>

---

## Connect a CLI tool

Most tools ask for an OpenAI base URL and an API key.

| Tool | Setting | Value |
|---|---|---|
| Cursor / Cline / Continue / Roo / Kilo | OpenAI base URL | `http://127.0.0.1:4623/v1` |
| Codex CLI | `OPENAI_BASE_URL` | `http://127.0.0.1:4623` |
| Claude Code | `~/.claude/config.json` `anthropic_api_base` | `http://127.0.0.1:4623/v1` |
| OpenClaw | dashboard → CLI Tools → OpenClaw | one-click apply |

The API key comes from the dashboard. Visit `http://127.0.0.1:4623`, create an API key, paste it into the tool's settings.

Tested CLIs: **Claude Code, Codex, Cursor, Cline, Continue, Roo, Kilo, Copilot, OpenClaw, OpenCode, Antigravity, Droid**.

---

## For AI Agents

ZeroProxy is built to be driven by AI agents (Devin, Claude Code, Codex, Cursor, OpenClaw, …) end-to-end — install, init, configure, and verify without any browser interaction.

A ready-to-use agent skill ships in this repo:

- [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md) — install, `server init`, `server start --detach`, declarative `provider apply`, and wiring CLI tools.

The `install.sh` one-shot installer **automatically drops the same file at `~/.agents/skills/zeroproxy/SKILL.md`** so agents that scan the home directory (Devin, Claude Code, …) pick it up the moment you install zeroproxy. The installer preserves any user-edited skill file (detected via the `name: zeroproxy` frontmatter marker) and exposes two flags:

- `--no-skill` — skip the auto-install entirely.
- `--skill-dest <dir>` — write to a custom skills root (default: `~/.agents/skills`).

The CLI is agent-friendly by design:

- `--robot` emits stable line-delimited JSON envelopes (`zeroproxy.v1.*`, frozen contract — additive only).
- `zeroproxy schema list` / `schema show <resource>` exposes the JSON shape for every `apply`-able resource.
- `zeroproxy provider apply --from-file -` is declarative and idempotent (`--prune` to reconcile).
- `zeroproxy doctor` self-tests the install, data dir, and server reachability.

Minimal autonomous bootstrap (no TTY, no browser, no prompts):

```bash
# 1. Install (drops binary at ~/.local/bin/zeroproxy)
curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
export PATH="$HOME/.local/bin:$PATH"

# 2. Initialize data dir; capture the admin API key from the JSON envelope
zeroproxy --robot server init | tee /tmp/op-init.json
APIKEY=$(jq -r '.data.admin_key.key' /tmp/op-init.json)

# 3. Start server detached + headless, then self-test
zeroproxy server start --detach --no-open
zeroproxy --robot doctor
zeroproxy --robot server status

# 4. Configure a provider declaratively (idempotent)
cat > /tmp/providers.json <<JSON
{ "providers": [{ "name": "openai", "provider": "openai", "apiKey": "sk-...", "isActive": true }] }
JSON
CIPHERROUTE_API_KEY="$APIKEY" zeroproxy --robot provider apply --from-file /tmp/providers.json

# 5. End-to-end smoke test against the running server
curl -sS http://127.0.0.1:4623/health
curl -sS http://127.0.0.1:4623/v1/models -H "Authorization: Bearer $APIKEY"
```

Full walkthrough — including failure modes, OAuth provider notes, and post-install verification — lives in [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md).

### Copy-paste prompt for your AI agent

Paste the block below into Devin, Claude Code, Codex, Cursor, OpenClaw, or any agent chat. It's a single self-contained instruction set — the agent will install ZeroProxy, start it, and verify it works without asking further questions.

````text
You are setting up ZeroProxy (https://github.com/atheerium/zeroproxy) on this
machine. ZeroProxy is a single-binary AI router exposing an OpenAI-compatible
API on http://127.0.0.1:4623. Follow these steps exactly, stop on the first
error, and report the captured admin API key back to me at the end.

1. Check whether `zeroproxy` is already on PATH:
       command -v zeroproxy && zeroproxy --version || echo "not installed"
   If already installed, skip to step 3.

2. Install the binary. Pick ONE path:
   a) Linux / macOS, x86_64 or aarch64 (preferred):
          curl -fsSL "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.sh" | bash
          export PATH="$HOME/.local/bin:$PATH"
   b) Windows, x86_64 (PowerShell 5.1+):
          irm "https://raw.githubusercontent.com/atheerium/zeroproxy/main/install.ps1" | iex
          $env:Path = "$env:USERPROFILE\.local\bin;" + $env:Path

3. Initialize a data dir and capture the admin API key. Use --robot so the
   output is JSON; never re-run with --force without my explicit approval:
       zeroproxy --robot server init | tee /tmp/op-init.json
       APIKEY=$(jq -r '.data.admin_key.key' /tmp/op-init.json)
       export CIPHERROUTE_API_KEY="$APIKEY"
   If `server init` reports `zeroproxy.sqlite already exists`, STOP and tell me — the
   data dir is pre-populated and I need to decide whether to overwrite.

4. Start the server detached and headless, then self-test:
       zeroproxy server start --detach --no-open
       zeroproxy --robot server status
       zeroproxy --robot doctor

5. Verify end-to-end:
       curl -sS http://127.0.0.1:4623/health
       curl -sS http://127.0.0.1:4623/v1/models \
         -H "Authorization: Bearer $CIPHERROUTE_API_KEY"

6. Report back to me:
   - The exact `zeroproxy --version` output.
   - The admin API key (value of $CIPHERROUTE_API_KEY).
   - Result of step 4's `server status` and `doctor`.
   - Any non-2xx response from step 5.

Do NOT run `zeroproxy server init --force`, do NOT delete ~/.zeroproxy/, and
Do NOT add provider API keys unless I gave you values explicitly. If you hit
the failure modes documented in
https://github.com/atheerium/zeroproxy/blob/main/.agents/skills/zeroproxy/SKILL.md
("Common failure modes & fixes"), apply the listed fix; otherwise stop and ask.
````

The same instructions in machine-readable form live at [`.agents/skills/zeroproxy/SKILL.md`](.agents/skills/zeroproxy/SKILL.md) — agents that auto-discover `.agents/skills/` (Devin, etc.) will pick them up without any copy-paste.

---

## Supported providers

| Tier | Provider | Auth | Notes |
|---|---|---|---|
| OAuth subscription | Claude Code, Codex, GitHub Copilot, Cursor, Antigravity | OAuth (PKCE) | Use your existing subscription quota. Auto-refresh. |
| API key | OpenAI, Anthropic, Gemini, OpenRouter, GLM, Kimi, MiniMax, DeepSeek, Groq, xAI, Mistral, Perplexity, Together, Fireworks, Cerebras, Cohere, NVIDIA, SiliconFlow, Nebius, Chutes, Hyperbolic, custom OpenAI/Anthropic-compatible endpoints | API key | 40+ supported. |
| Free | Kiro AI (Claude 4.5 + GLM-5 + MiniMax), OpenCode Free, Vertex AI ($300 trial credits) | OAuth / no auth / GCP service account | Best for fallback tiers. |

Configure providers from the dashboard (`Providers` tab) or via `zeroproxy provider` CLI subcommands. Each provider supports multiple accounts; ZeroProxy round-robins between them.

---

## Combos: build a fallback chain

A combo is an ordered list of models. ZeroProxy tries them in order, falling back when one is rate-limited or errors. Models are addressed as `<provider-prefix>/<model-id>`:

```
combo: my-stack
  1. cc/claude-opus-4-7      # Claude Pro/Max subscription
  2. glm/glm-4.7             # paid backup ($0.6/1M)
  3. kr/claude-sonnet-4.5    # Kiro free fallback
```

Created from `Combos` in the dashboard or `zeroproxy combo create`. Use the combo name as the model field in your CLI tool — ZeroProxy resolves it.

---

## Configuration

Most operators only set `JWT_SECRET` and leave the rest at defaults. The dashboard password is a random value generated on first boot (see `INITIAL_PASSWORD` below).

| Variable | Default | Purpose |
|---|---|---|
| `JWT_SECRET` | `zeroproxy-default-secret-change-me` | Sign the dashboard session cookie. **Change in production.** |
| `INITIAL_PASSWORD` | _random, generated once_ | First-login password when no saved hash exists. When unset, a random password is generated at first boot and **printed once in the startup banner** (`$DATA_DIR/initial_password` is persisted so it stays stable). Reset it anytime with `zeroproxy auth reset-password`. |
| `DATA_DIR` | `~/.zeroproxy` | Where `zeroproxy.sqlite`, data, and logs live. |
| `PORT` | `4623` | HTTP listen port. |
| `HOSTNAME` | `127.0.0.1` | Bind host. Set `0.0.0.0` to expose on LAN. |
| `BASE_URL` | `http://localhost:4623` | Internal base URL for cloud-sync jobs. |
| `CLOUD_URL` | _unset_ | Cloud-sync endpoint. Leave unset to disable cloud sync. |
| `API_KEY_SECRET` | _auto-generated, persisted_ | HMAC secret for generated API keys. When unset, a random secret is generated at first startup and persisted to `$DATA_DIR/api_key_secret`. |
| `MACHINE_ID_SALT` | `endpoint-proxy-salt` | Salt for the stable machine-ID hash. |
| `AUTH_COOKIE_SECURE` | `false` | Force `Secure` flag on the auth cookie. Set `true` behind HTTPS. |
| `REQUIRE_API_KEY` | `false` | Reject `/v1/*` requests without `Authorization: Bearer …`. Recommended for any internet-exposed deploy. |
| `ENABLE_REQUEST_LOGS` | `false` | Write per-request logs under `logs/`. |
| `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` | _unset_ | Forward outbound provider calls through an HTTP proxy. Lowercase variants also honored. |

### TOML config profiles

For advanced setups (multiple ZeroProxy instances, remote server management) the CLI
reads an optional TOML profile file at `~/.config/zeroproxy/config.toml`
(`%APPDATA%\zeroproxy\config.toml` on Windows; override via `$CIPHERROUTE_CONFIG`).

```toml
default_profile = "work"

[profiles.work]
data_dir = "/home/me/proxy-data"
url = "https://proxy.example.com"
api_key_env = "MY_PROXY_KEY"        # read API key from this env var

[profiles.local]
data_dir = "/tmp/proxy-test"
```

Resolution precedence (highest first):
1. Explicit CLI flags (`--data-dir`, `--url`, `--api-key`, `--profile`, `--port`)
2. `CIPHERROUTE_*` environment variables
3. Selected profile (`--profile <name>` or `default_profile`)
4. Built-in defaults

Profiles are created programmatically by `zeroproxy auth login` / `zeroproxy auth logout`
— you do not normally need to hand-edit the TOML file.

### Compiled-in constants

Unlike 9router, the following are **not** hot-reloadable or config-file driven.
They are compiled into the binary and require a rebuild to change:

| Constant | Source file | Default |
|---|---|---|
| Stream stall timeout | `src/core/config/runtime_config.rs` | 360 s (6 min) |
| First-chunk timeout | `src/core/config/runtime_config.rs` | 200 s |
| Default max tokens | `src/core/config/runtime_config.rs` | 64 000 |
| Retry config (502, 503, 504) | `src/core/config/runtime_config.rs` | 2–3 attempts, 2–3 s delay |
| Provider catalog (models, aliases) | `src/core/model/provider_catalog.json` | ~70 providers, ~200 models |
| OAuth provider registry | `src/oauth/providers.rs` | 18 built-in OAuth configs |
| Gemini CLI version string | `src/core/config/app_constants.rs` | `0.34.0` |
| GitHub Copilot versions | `src/core/config/app_constants.rs` | Chat `0.38.0`, VS Code `1.110.0` |
| Default tool-name decoys | `src/core/config/app_constants.rs` | Claude Code / Antigravity tool sets |
| Kiro suffixes & system prompt | `src/core/config/kiro_constants.rs` | `-agentic`, `-thinking` |
| Thinking-mode signatures | `src/core/config/default_thinking_signature.rs` | Claude, AG, Vertex, Gemini CLI |

---

## CLI reference

```
zeroproxy [FLAGS]                  # default: start server + open browser
zeroproxy --port 4623 --no-open    # foreground, no browser
zeroproxy --web-dir ./web/dist     # serve dashboard from disk (UI dev)
zeroproxy --dashboard-sidecar-url http://127.0.0.1:4624
                                    # reverse-proxy dashboard requests to a dev server

zeroproxy --version
zeroproxy provider list
zeroproxy provider add <name> '<json-config>'
                                    # e.g. zeroproxy provider add openai-paid \
                                    #        '{"provider":"openai","apiKey":"sk-..."}'
zeroproxy combo create --name <name> --models cc/opus,glm/glm-5
zeroproxy key list
zeroproxy key add <name> <secret>  # provide your own secret
zeroproxy key add <name> --auto    # let zeroproxy mint a fresh `op-…` secret
zeroproxy quota list               # subcommands: list / get / reset / refresh
zeroproxy usage summary            # subcommands: summary / daily / chart / history / …
zeroproxy doctor                   # diagnose common config issues

zeroproxy server start [--detach] [--no-open] [--port P]
zeroproxy server status
zeroproxy server stop
zeroproxy server init              # mint the first admin API key

zeroproxy sync 9router [--dry-run] [--prune]
                                    # pull provider/model catalog from atheerium/9router
zeroproxy sync omniroute [--dry-run] [--prune]
                                    # pull provider/model catalog from diegosouzapw/OmniRoute
```

`zeroproxy sync` applies embedded snapshots of sister open-source routers
into `customModels` in the SQLite store, tagged with `source` so a later
`--prune` only removes entries it previously added. Maintainers refresh
the snapshots via `node scripts/sync/normalize-sources.mjs`
([scripts/sync/README.md](scripts/sync/README.md)).

`zeroproxy --help` prints the full reference. Subcommands have their own `--help`.

Output formats: human (default), `--robot` (line-delimited JSON for agent/automation use), `--quiet`.

---

## API

OpenAI-compatible chat completions:

```http
POST /v1/chat/completions
Authorization: Bearer <api-key>
Content-Type: application/json

{
  "model": "cc/claude-opus-4-6",
  "messages": [{"role": "user", "content": "..."}],
  "stream": true
}
```

List available models and combos:

```http
GET /v1/models
Authorization: Bearer <api-key>
```

Health probe (no auth):

```http
GET /health   →   200 OK
```

The dashboard at `/` is the same authenticated API surface in HTML form. Admin endpoints live under `/api/*` and use the dashboard session cookie.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│ zeroproxy  (single binary, port 4623)      │
│                                             │
│  /            embedded web dashboard       │
│               (Astro static via rust-embed)│
│                                             │
│  /v1/*        OpenAI-compatible API        │
│  /api/*       admin / dashboard data       │
│  /codex/*     Codex OAuth helper           │
│                                             │
│  RTK token compression  ─┐                  │
│  format translation     ─┤                  │
│  quota tracking         ─┼─→ provider HTTP │
│  account fallback       ─┘                  │
└─────────────────────────────────────────────┘
                                                  │
                                                  ↓
                            [ provider APIs: Anthropic, OpenAI, GLM, ... ]
```

Stack: Rust 1.76+, axum 0.8, hyper 1, rusqlite (bundled), Astro 4 (static, embedded), React 19, Tailwind. Storage: SQLite (`zeroproxy.sqlite`) with legacy JSON import on first run.

---

## Build from source

Requires Node ≥ 20.3 and `pnpm` (`corepack enable && corepack prepare pnpm@10.33.2 --activate`, or `npm i -g pnpm`).

```bash
git clone https://github.com/atheerium/zeroproxy.git
cd zeroproxy

pnpm --dir web install
pnpm --dir web run build

cargo build --release --locked
./target/release/zeroproxy
```

UI iteration without rebuilding the binary:

```bash
pnpm --dir web run build
cargo run -- --web-dir ./web/dist
```

UI live-reload via the Astro dev server:

```bash
# Terminal 1
pnpm --dir web run dev   # → http://127.0.0.1:4624

# Terminal 2
cargo run -- --dashboard-sidecar-url http://127.0.0.1:4624
```

Headless build (no embedded dashboard, smaller binary):

```bash
cargo build --release --locked --no-default-features
# Requires --web-dir or --dashboard-sidecar-url at runtime.
```

---

## Deployment

### Docker

Pull the prebuilt image (published to GHCR by the release pipeline):

```bash
docker run -d \
  --name zeroproxy \
  -p 4623:4623 \
  -v zeroproxy-data:/app/data \
  ghcr.io/atheerium/zeroproxy:latest
```

Or build locally:

```bash
docker build -t zeroproxy .
docker run -d \
  --name zeroproxy \
  -p 4623:4623 \
  --env-file ./.env \
  -v zeroproxy-data:/app/data \
  zeroproxy
```

Container defaults: `HOSTNAME=0.0.0.0`, `PORT=4623`, `DATA_DIR=/app/data`. The dashboard is embedded — no separate volume needed for it. Mount `/app/data` to persist the SQLite database, `db_backups/`, and request logs across container restarts.

> First-time pulls from GHCR for this repo may require the package to be set to public at https://github.com/atheerium/zeroproxy/pkgs/container/zeroproxy.

### Behind a reverse proxy

For internet-exposed deploys: set `REQUIRE_API_KEY=true`, `AUTH_COOKIE_SECURE=true`, terminate TLS at the proxy, and forward only `/v1/*` if you don't need the dashboard accessible publicly.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `EADDRINUSE` on `4623` | Port in use | `zeroproxy --port 4624` or `zeroproxy server stop` |
| Dashboard shows blank page | Embedded asset not hashed correctly | Hard reload (`Ctrl+Shift+R`); check `/health` returns 200 |
| OAuth "callback failed" | Browser blocked the redirect | Retry from the dashboard's `Providers → Reconnect` |
| 401 on `/v1/chat/completions` | Wrong API key | Copy fresh from dashboard. Header: `Authorization: Bearer <key>` |
| Quota exhausted message | Subscription / API limit hit | Combo fallback handles this — add a cheaper or free tier as the next entry |
| `cargo build` fails with "web/dist not built" | Embedded build needs the dashboard | `(cd web && pnpm install --frozen-lockfile && pnpm run build)` first |
| First login password rejected | Wrong dashboard password | If you set `INITIAL_PASSWORD`, check `.env` is sourced. Otherwise the password was generated at first boot — look for "Initial dashboard password" in the startup banner or run `zeroproxy auth reset-password --show` |

Logs: enable with `ENABLE_REQUEST_LOGS=true`, then watch `logs/` (or stderr).

---

## Acknowledgments

Built on the work of others:

- **CLIProxyAPI** — Go implementation that inspired the architecture.
- **[RTK](https://github.com/rtk-ai/rtk)** — token compression pipeline. ZeroProxy's `tool_result` compression is a port.
- **[Caveman](https://github.com/JuliusBrussee/caveman)** — caveman-speak prompt that trims output tokens by reframing the system instruction.

---

## License

MIT — see [LICENSE](LICENSE).

