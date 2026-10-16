# Free-tier provider limits — reference for a free-tier-only router

**Compiled 2026-10-16.** Owner decision recorded: this router is used **only** for free-tier
providers, so a provider is worth keeping only if its free allowance can actually carry real
work.

## The bar used in this document

A free tier is flagged **TRASH** when any of these hold:

- fewer than **20 requests/day**, or
- fewer than **20M tokens/day**, or
- the allowance is a **one-time pool** rather than a recurring daily budget, or
- the provider **publishes no limits at all**, so capacity cannot be planned for.

"not documented" is a real answer here and is never filled in with a guess. Where a vendor's own
docs and third-party lists disagree, both are shown and the conflict is flagged rather than
resolved.

Two independent kinds of evidence are mixed below, and they are kept separate on purpose:

- **Published** — first-party documentation, with the URL and retrieval date.
- **Ours** — measured on this machine against this router (usage DB, discovery, live probes).

---

## 1. TL;DR — what to keep, fix and drop

| Provider | Verdict | One-line reason |
|---|---|---|
| **kilocode** | ✅ **KEEP** | 200 req/hour/IP, no card, no account, no daily cap. Best free tier available. |
| **openrouter** | ✅ **KEEP** | 20 RPM; 50/day free, 1000/day once $10 is purchased. 15 `:free` models. |
| **nvidia** | ⚠️ **KEEP** (unplannable) | Free trial works, but NVIDIA deliberately publishes **no** limits. |
| **gemini-cli** | 🔧 **FIX — highest value** | Quota is **1,000 req/day** (best of the OAuth set) and ours returns **0 successes**. |
| **kiro** | ⚠️ **KEEP, verify first** | Published free tier is **50 credits/month ≈ 0.2–3 tasks/day** → TRASH by quota. But ours still half-works (20/39), so check whether the failures are quota exhaustion before dropping. |
| **gemini** | ⚠️ KEEP, fix one key | Free tier exists; **one of the two configured keys is invalid**. |
| **opencode-zen** | 🔴 **DROP** | Free models now return **HTTP 403** outside the OpenCode client (verified today). |
| **antigravity** | 🔴 **DROP** | **591 requests, 591 failures, 0 successes** — despite being the only free route to Claude Opus 4.6. |
| **groq** | 🔴 **DROP** | Best case 200K tokens/day = 1% of the bar. |
| **llm7** | 🔴 **DROP** | 100,000 tokens/day = 0.5% of the bar. |
| **apmix** | 🔴 **DROP as a route** | 3M tokens **once**, one model, then gone forever. |
| **tokenrouter** | 🔴 **DROP** | Publishes nothing at all; no documented free tier. |
| **cohere** | 🔴 **DROP** | Trial key = 1,000 calls **per month**, evaluation licence only. |
| **mistral** | 🟡 WEAK | Free mode exists, no numbers published; observed 1 request lifetime. |
| **ollama cloud** | 🟡 WEAK | Free starter credits never quantified; 1 concurrent request. |
| **qoder** | 🔴 **DROP** | ~2 agent calls/day; **key currently returns 401**. |
| **codex / OpenAI** | 🔴 **DROP as free** | No free API tier exists — the "Free" row is a spend ceiling, not a grant. |
| **deepseek-web, kimi-web** | 🔴 **DROP** | Consumer endpoints, no quota; DeepSeek's ToS explicitly prohibits automated access. |
| **agentrouter, cline, kimi** | 🔴 **DROP** | No published free tier, no models listing, zero usage. |
| **copilot free** | 🟡 WEAK | Credits magnitude withheld, **auto model selection only** — unpinnable by a proxy. |
| 4 × UUID-named connections | 🔴 **DELETE** | Test leftovers with no provider identity. |

**Net effect on your six:** four hold up (`kilocode`, `openrouter`, `nvidia`, `gemini`), Zen is
**dead**, and Kiro's published free tier does **not** clear the bar — but that verdict comes from
documentation, not from your account, so it deserves a measurement before you act on it.

---

## 2. The four providers that matter

### kilocode — ✅ the best free tier available

| | |
|---|---|
| RPM | **200 requests per hour, per IP** (both anonymous and authenticated) |
| RPD | not documented — no daily cap; ~4,800/day theoretical |
| TPM / TPD | not documented — **no token cap** |
| Card | **No** — verified: an unauthenticated request returned a real completion |
| Reset | rolling hour window |
| Models | 12 `:free` (of 397 discoverable) |
| Source | <https://kilo.ai/docs/gateway/usage-and-billing>, retrieved 2026-10-16 |

**Ours:** 178 successes / 186 total (96%), peak 79 requests on 2026-10-14. Healthy discovery
(397 models).

⚠️ **Caveats.** The 200/hour is **per IP and shared with anonymous traffic** — it cannot be
multiplied with extra keys, and it is hostile to NAT. `kilo-auto/free` may route to providers
that log prompts, per Kilo's own docs. Also note the gateway host is **`ai-gateway.kilo.ai`**;
`api.kilo.ai` returns 404.

### openrouter — ✅ keep, and check whether the $10 is already spent

| | |
|---|---|
| RPM | **20** |
| RPD | **50** with <$10 purchased · **1000** with ≥$10 purchased (all-time, permanent) |
| TPM / TPD | **no token limit** — `:free` is request-count only |
| Card | no for 50/day; **yes** for 1000/day |
| Reset | fixed UTC day (00:00 UTC) |
| Models | 15 `:free` (of 465 discoverable) |
| Source | <https://openrouter.ai/docs/api_reference/limits>, retrieved 2026-10-16 |

**Ours — and this is the actionable part:** usage history shows **134 requests on 2026-10-03**,
which is impossible under the 50/day free cap. Either this account already carries the $10
purchase (→ 1000/day, and the cap is not what has been limiting you), or those requests were
being throttled. 259/273 succeeded (95%).

Limits are **per account**, not per key — extra keys and extra accounts do not raise them.

### nvidia — ⚠️ keep, but you cannot plan against it

NVIDIA **publishes no free-tier limits at all**, confirmed by their own staff:

> "The rate limits vary for each model, and we do not publish those. However, you can see your
> maximum rate limit in the top right of build.nvidia.com" — NVIDIA moderator, 2025-09-10,
> <https://forums.developer.nvidia.com/t/request-more-4-000-credits-option-on-build-nvidia-com/344567>

> "The trial period is not limited by a time period." — same thread, 2025-09-17

The widely-quoted "1,000 credits / 40 RPM" figure is **community-reported only**, contradicted
by staff who say the credit system was abolished. Treat it as unconfirmed.

**Ours:** 37/39 success (95%), 81 discoverable models, all free-trial. Separately, `z-ai/glm-5.3`
models on this tier hang (60s, no response) — that is an availability problem, not a quota one,
and it interacts with the router's 600 s stream timeout.

### opencode-zen — 🔴 the free tier is gone for third-party routing

**Verified today, unauthenticated, by us and independently by research:**

```
POST https://opencode.ai/zen/v1/chat/completions   {"model":"space-bunny-free"}
→ HTTP 403
{"type":"error","error":{"type":"FreeTierError",
  "message":"OpenCode's free tier can only be used from within OpenCode."}}
```

Both `/zen/v1` and the newer `/inference/openai/v1` return the same 403 for all 13 live free
models. The **catalogs stay public (200)**, so a router can still *see* the free models and
cannot *call* them.

**Our own timeline dates the change precisely:** the last successful Zen request in our DB was
**2026-10-05T23:17Z** (29 successes, 0 failures up to that point). So the gate closed between
**2026-10-05 and 2026-10-16** — not earlier.

⚠️ **This makes `AGENTS.md` Trap 19 stale.** That trap records `space-bunny-free` returning 200
unauthenticated on 2026-10-05; that is now expired. (Trap 19 should be updated — it was not
edited here because another agent currently owns `AGENTS.md`.)

The only documented unlimited quotas are behind **OpenCode Go, $10/month**, which does permit
third-party clients. If OpenCode capacity is wanted, that is the thing to buy — it is a paid
product, not a free tier.

---

## 3. Direct API free tiers

| Provider | RPM | RPD | TPM | TPD | Card | Reset | Verdict |
|---|---|---|---|---|---|---|---|
| **gemini** (AI Studio) | not documented | not documented | not documented | not documented | No | midnight **Pacific** | ⚠️ unplannable |
| **groq** | 30 | 1K / 14.4K / 2K by model | 8K / 15K | **200K / 500K** | Free: no | rolling | 🔴 **TRASH** (1% of bar) |
| **cohere** (trial) | 20 / model | **1,000 per MONTH**, all endpoints | — | — | No | calendar month | 🔴 **TRASH** |
| **mistral** | not documented | not documented | not documented | not documented | not documented | not documented | 🟡 WEAK |
| **ollama cloud** | — | — | — | — | No | monthly from signup | 🟡 WEAK |
| **apmix** | not documented ("no daily limits") | not documented | — | **3M total, one-time** | No | one-time, no expiry | 🔴 **TRASH** |
| **tokenrouter** | not documented | not documented | not documented | not documented | not documented | not documented | 🔴 **TRASH** (publishes nothing) |
| **llm7** | 1/sec, 60/min, 250/hr | — | — | **100,000 / 24h** | No | rolling 24h | 🔴 **TRASH** (0.5% of bar) |
| **OpenAI / codex** | none published | none | none | none | n/a | n/a | 🔴 **TRASH as free** |

Sources: Gemini <https://ai.google.dev/gemini-api/docs/rate-limits> (updated 2026-10-15);
Groq <https://console.groq.com/docs/rate-limits>; Cohere
<https://docs.cohere.com/v2/docs/rate-limits.md>; Mistral
<https://docs.mistral.ai/admin/billing-usage/usage-limits>; Ollama <https://ollama.com/pricing>;
apmix <https://apmix.ai/pricing>; LL7 <https://docs.llm7.io/limits>; OpenAI
<https://developers.openai.com/api/docs/guides/rate-limits>. All retrieved 2026-10-16.

**Notes that matter operationally:**

- **Gemini** removed its public per-model free-tier table; limits are now only visible in your
  own AI Studio project, and they are **per project, not per key**. Free-tier content is
  documented as **used to improve Google's products**. One of the two configured keys is
  **invalid** (`400 API key not valid`); the other works (46 models).
- **Groq**'s docs publish only Developer-plan limits, not Free. Several third-party sites report
  the free tier as byte-identical to the Developer table — unverified either way, but 200K TPD
  is 1% of the bar under any reading. Its value is speed, not free quota.
- **Cohere**'s 1,000/month is a single pooled monthly allowance (burn it and wait for the 1st),
  and trial keys are evaluation-only by Cohere's own FAQ.
- **apmix** is honestly worded: *"3M free tokens on the free model… no daily limits, no expiry"*
  — a legitimate one-shot coupon, not a route. Its free model is `claude-sonnet-4-6-free`, not
  the `gpt-6-luna-free` that was wired locally.
- **LL7** states plainly that free quotas *"may be reduced without notice based on demand,
  service capacity, model availability…"*.

---

## 4. OAuth / CLI-account providers

| Provider | What the free tier actually grants | RPM | RPD | TPM | TPD | Card | Reset | Verdict |
|---|---|---|---|---|---|---|---|---|
| **gemini-cli** | **1,000 model requests per user per day**, aggregated across the whole Gemini family | not published | **1,000** | — | — | No | daily | ✅ **USABLE** |
| **antigravity** | "A meaningful quota, refreshed weekly" — **no number published** | not documented | not documented | not documented | not documented | No | **weekly** | 🟡 WEAK |
| **kiro** | **50 credits per month** — a *unit of work*, not requests | not documented | **≈0.2–3 tasks/day** (derived) | not documented | not documented | No | monthly, billing cycle | 🔴 **TRASH** |
| **cline** | "free model promotions … up to a limited usage quota", rotating | not documented | not documented | not documented | not documented | No | not documented | 🔴 **TRASH** |
| **copilot free** | "an allowance of AI credits" (magnitude withheld) + 2,000 completions/month, **auto model selection only** | not documented | monthly, not daily | — | — | No | 00:00 UTC, 1st of month | 🟡 WEAK |
| **deepseek web** | **none** — consumer endpoint, not licensed for API use | — | — | — | — | n/a | n/a | 🔴 **not permitted** |
| **kimi web** | **none** — API platform is pay-as-you-go; a paid consumer membership converts to nothing | — | — | — | — | n/a | n/a | 🔴 **TRASH** |
| **agentrouter** | **no self-serve free tier** — pay-as-you-go per token; trial only "on request" | not documented | not documented | not documented | not documented | No | n/a | 🔴 **TRASH** |

Sources: gemini-cli <https://geminicli.com/docs/resources/quota-and-pricing/>;
antigravity <https://antigravity.google/docs/plans/>; kiro
<https://kiro.dev/docs/billing/> (page updated 2026-08-04) + `/docs/billing/related-questions/`
+ `/docs/models/`; cline <https://docs.cline.bot/getting-started/free-models>; copilot
<https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing>; deepseek ToS
<https://cdn.deepseek.com/policies/en-US/deepseek-terms-of-use.html> (updated 2026-03-27); kimi
<https://www.kimi.com/help/kimi-api/api-troubleshooting>; agentrouter
<https://co.agentrouter.org> pricing. All retrieved 2026-10-16.

### The Kiro reversal — this one matters

Kiro does not publish requests, it publishes **credits**, and the free tier is **50 per month**:

> "A credit is a unit of work in response to user prompts… Different models consume credits at
> different rates." — <https://kiro.dev/docs/billing/related-questions/>

Per-model multipliers: Qwen3 Coder Next 0.05x · MiniMax M2.1 0.15x · DeepSeek 3.2 0.25x ·
GLM-5 0.5x · Auto 1.0x · Sonnet 4.5 1.3x. Kiro's own worked example implies a typical agentic
task costs **~10 credits on Auto**, so 50 credits/month is:

- **≈5 tasks/month** on Auto or Sonnet (~0.2/day), or
- **≈100 tasks/month** on Qwen3 Coder Next (~3.3/day)

Both are below the 20/day bar. The free tier also excludes every Opus, every Sonnet ≥4.6, all
GPT-5.6 variants, Haiku 4.5 and Kimi K3.

⚠️ **Superseded figure:** older sources say "50 Vibe requests/month". That model was replaced by
credits, and one agentic task ≈ 10 credits on Auto — a materially different unit.

**Ours:** 20 successes / 19 failures across 2026-09-28 → 2026-10-14. The 51% failure rate is
consistent with the quota being exhausted mid-session. **Before dropping Kiro, check whether
those failures are quota exhaustion rather than a broken integration** — it is one of your
six most-used providers and this is a documentation verdict, not a measurement of your account.

### The gemini-cli opportunity — best quota, worst implementation

**1,000 requests/user/day** is the only published daily figure in the whole OAuth set that
clears the bar by 50×. But **ours has produced 174 requests and 0 successes**, and discovery
returns 0 models. So this is not a quota problem — it is a broken integration with the best
available quota. **Highest-value fix available**, and it is a fix, not a prune.

Note the accounting rule: one prompt can consume several model requests, and the limit is
**per user per day**, shared with any Gemini CLI or agent mode running on the same Google
account.

### Not-permitted, not merely small

**deepseek web** — DeepSeek's terms, §3.5(3), prohibit "using any robots, spiders, or other
automatic setups", and §8.2 lists account suspension/closure among the remedies. There is no
documented free quota for the consumer endpoint at all.

**kimi web** — Kimi's own help page states the API platform, Kimi Code and Kimi membership are
separate products whose payment, balance and API keys are not interchangeable, and that the
open platform is pay-as-you-go with no subscription. The only free item is a ¥15 new-user
voucher, which **cannot be used on Kimi K3**.

**agentrouter** — two different products share the brand. `agent-router.org` is a free MCP agent
registry (not an inference provider). `co.agentrouter.org`, the LLM gateway this router talks to,
is pay-as-you-go per token with trial credits only "on request". The widely-quoted "$100 free
credits on signup / $200 per referral" appears only in a GitHub Gist and social posts — **no
first-party page states it.**

---

## 5. Our own measurements, 2026-10-16

All-time from the live usage DB (`usageHistory`), success = HTTP 2xx as recorded by the router:

| Provider | Success | Failure | Total | Success rate |
|---|---|---|---|---|
| antigravity | 0 | **591** | 591 | **0%** |
| gemini-cli | 0 | **174** | 174 | **0%** |
| openrouter | 259 | 14 | 273 | 95% |
| kilocode | 178 | 8 | 186 | 96% |
| kiro | 20 | 19 | 39 | 51% |
| nvidia | 37 | 2 | 39 | 95% |
| opencode-zen | 29 | 0 | 29 | 100% (all ≤ 2026-10-05) |
| apmix | 2 | 1 | 3 | 67% |

**765 requests produced zero tokens** via antigravity + gemini-cli. Every request through them
fails and falls back, which both wastes latency and pollutes the routing signal.

Discovery status of all 28 configured connections (`GET /api/providers/{id}/models`):

| Result | Providers |
|---|---|
| Works, large catalog | openrouter (465), kilocode (397), tokenrouter (162), nvidia (81), opencode-zen (88), llm7 (64), gemini (46), mistral (45), ollama (21), groq (11), codex (7) |
| Returns 0 models | kiro (expected — no `profileArn` for Builder ID), gemini-cli |
| **401 invalid key** | qoder |
| **429 rate-limited** | cohere |
| **400 invalid key** | one of the two gemini connections |
| 404 on models fetch | antigravity |
| No listing path | apmix, agentrouter, cline, deepseek-web ×2, kimi, kimi-web, 4 × UUID test rows |

Peak daily request counts: openrouter **134/day** on 2026-10-03 (impossible under the 50/day
free cap → this account likely carries the $10 credit), kilocode 79/day on 2026-10-14 (well
under its 200/hour).

---

## 6. The long tail

The dashboard exposes **109 distinct free-tier providers** (6 no-auth + 10 hand-curated +
93 generated from the OmniRoute snapshot). Only the ones above were researched. The remaining
~93 have **no verified limits** and were not individually checked; treat every catalog `free`
marker on them as **unverified** until someone reads that vendor's own docs.

That is itself the finding: a 109-provider free catalog is mostly noise. If the goal is a
free-tier-only router, the catalogue is more useful when it is a short, verified list than a
long, unverified one.

---

## 7. Conflicts found between docs and third-party lists

| # | Conflict | Resolution |
|---|---|---|
| 1 | NVIDIA staff: "no credit system, unpublished per-model limits" vs community "1,000 credits / 40 RPM" (still being quoted in Oct 2026) | **Unresolved.** NVIDIA publishes nothing; treat all NVIDIA figures as community-reported. |
| 2 | `costgoat.com`: OpenRouter free = 200 req/day | **Wrong.** Official: 50 or 1000. |
| 3 | `opencode.asia` "100/day", Reddit "500/day", HN "200 per 5 hours" for OpenCode Zen | **All unverified and mutually incompatible.** Official docs publish nothing. |
| 4 | OpenCode docs list 15 free models; live API returns 13 | Live wins: 3 removed, 1 undocumented addition. |
| 5 | Groq: docs publish Developer-tier numbers; third parties present them as the free tier | **Unverified.** Immaterial: TRASH under either reading. |
| 6 | LL7: third parties say "10 RPM anonymous / 40 with token"; official says 60 RPM with token | Official wins. |
| 7 | OpenAI: third parties disagree on whether a $5 free credit exists | No current first-party page documents one. Treat as paid. |
| 8 | Kiro: older sources say "50 Vibe requests/month"; current docs say **50 credits/month** | **Superseded.** Credits are the current unit; one agentic task ≈ 10 credits on Auto. |
| 9 | AgentRouter: "$100 free credits on signup / $200 per referral" | **Unsourced** — GitHub Gist and social posts only. First-party says trial is available "on request". |
| 10 | Copilot Free: "allowance of AI credits" with no number | **Withheld by design.** Only paid tiers are quantified (Pro 1,500 credits ≈ $15). |
| 11 | Gemini CLI "60 RPM" | **Unsourced** (a third-party blog). Google publishes no RPM; only the 1,000/day figure is real. |

---

## 8. How to keep this document honest

1. **Zen's 403 was found because it was tested**, not because a notice was published. Re-probe
   the surviving providers monthly; a free tier can be closed without warning (Zen's own docs
   promised nothing, and the change landed with no announcement).
2. Prefer a first-party URL for every number. Record "not documented" rather than inheriting a
   number from a listicle.
3. Record the retrieval date — every row above was retrieved 2026-10-16.
4. Cross-check against `usageHistory` before acting: a provider can be well-documented and
   still fail 100% of requests in practice, which is exactly what happened to antigravity and
   gemini-cli.