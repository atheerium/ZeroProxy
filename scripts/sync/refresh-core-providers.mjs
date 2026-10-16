#!/usr/bin/env node
// Daily catalog refresh for the five providers this router actually depends on.
//
// Why this exists: `zeroproxy sync omniroute` refreshes the *whole* upstream
// snapshot and writes it out-of-process, so the running server keeps serving the
// old catalog until a restart. Free-model rosters churn daily (openrouter and
// kilocode add/remove `:free` entries constantly), and a stale roster means dead
// models keep getting picked by routing.
//
// This script instead goes through the running server's HTTP API, using the
// per-connection live discovery that already exists
// (`GET /api/providers/{id}/models`). That means: no restart, no full-snapshot
// churn, and only the five providers you actually use.
//
// Managed rows are tracked in the state file below, so pruning only ever
// removes rows this script created. User-created custom models are never
// touched, and a provider whose discovery fails is skipped rather than emptied.

import { readFile, writeFile, mkdir } from "node:fs/promises"
import { dirname } from "node:path"

const BASE_URL = (process.env.ZEROPROXY_URL || "http://127.0.0.1:4623").replace(/\/+$/, "")
const STATE_PATH =
  process.env.ZEROPROXY_CORE_CATALOG_STATE ||
  `${process.env.HOME || "/home/atheerium"}/.zeroproxy/core-provider-models.json`
const SYNC_MARKER = "core-provider-refresh"

// `free` decides which discovered models become catalog rows. nvidia is
// free-tier by API key rather than by suffix, so it keeps its whole roster.
const FREE_SUFFIX = /(:free|-free)$/i

const SCOPE = [
  { provider: "opencode-zen", alias: "opencode-zen", freeOnly: true },
  { provider: "openrouter", alias: "openrouter", freeOnly: true },
  { provider: "kilocode", alias: "kc", freeOnly: true },
  { provider: "nvidia", alias: "nvidia", freeOnly: false },
  // Kiro discovery needs a profileArn that Builder ID never returns, so
  // `GET /api/providers/{id}/models` legitimately returns []. Leave it to the
  // built-in catalog instead of writing an empty roster.
  { provider: "kiro", alias: "kr", freeOnly: false, skip: true },
]

// Measured 2026-10-05 against the live zen API: these pass the free suffix test
// but the upstream Console refuses them from outside the OpenCode client with
// `FreeTierError`. That gate is server-side and per-model — a byte-faithful
// replay of OpenCode's own headers still fails — so re-adding them on every
// refresh would only hand the router models that always 502.
const ZEN_FREE_TIER_DENIED = new Set([
  "fledge-alpha-free",
  "jev-1.13-free",
  "ling-3.0-flash-fin-free",
  "ling-3.1-flash-free",
  "longcat-2.5-preview-free",
  "mimo-v2.5-free",
  "mimo-v2.6-flash-free",
  "muse-spark-1.2-contributor-free",
  "muse-spark-1.3-contributor-free",
  "nemotron-3-ultra-free",
  "nemotron-3.5-lightning-free",
])

async function api(path, init = {}) {
  const headers = { "content-type": "application/json", ...(init.headers || {}) }
  if (process.env.ZEROPROXY_API_KEY) headers.authorization = `Bearer ${process.env.ZEROPROXY_API_KEY}`
  const response = await fetch(`${BASE_URL}${path}`, { ...init, headers })
  if (!response.ok) throw new Error(`${init.method || "GET"} ${path} -> ${response.status}`)
  return response.json()
}

async function readState() {
  try {
    return JSON.parse(await readFile(STATE_PATH, "utf8"))
  } catch {
    return {}
  }
}

async function writeState(state) {
  await mkdir(dirname(STATE_PATH), { recursive: true })
  await writeFile(STATE_PATH, JSON.stringify(state, null, 2) + "\n")
}

async function main() {
  const { connections } = await api("/api/providers")
  const state = await readState()
  const summary = []

  for (const entry of SCOPE) {
    if (entry.skip) {
      summary.push({ provider: entry.provider, action: "skipped", reason: "no live discovery" })
      continue
    }

    const owned = connections.filter((c) => c.provider === entry.provider)
    if (owned.length === 0) {
      summary.push({ provider: entry.provider, action: "skipped", reason: "no connection" })
      continue
    }

    const discovered = new Set()
    let failed = null
    for (const connection of owned) {
      try {
        const body = await api(`/api/providers/${connection.id}/models`)
        for (const model of body.models || []) discovered.add(model.id)
      } catch (error) {
        failed = error.message
      }
    }

    // A discovery failure must never be mistaken for "upstream has no models".
    if (discovered.size === 0) {
      summary.push({ provider: entry.provider, action: "skipped", reason: failed || "empty discovery" })
      continue
    }

    const wanted = [...discovered]
      .filter((id) => !entry.freeOnly || FREE_SUFFIX.test(id))
      .filter((id) => entry.provider !== "opencode-zen" || !ZEN_FREE_TIER_DENIED.has(id))
      .sort()

    const previous = new Set(state[entry.provider] || [])
    const wantedSet = new Set(wanted)

    let created = 0
    for (const id of wanted) {
      if (previous.has(`${entry.alias}/${id}`)) continue
      try {
        await api("/api/models/custom", {
          method: "POST",
          body: JSON.stringify({ providerAlias: entry.alias, id, type: "llm", name: id }),
        })
        created += 1
      } catch (error) {
        if (!/already exists/i.test(error.message)) throw error
      }
    }

    let removed = 0
    for (const key of previous) {
      if (wantedSet.has(key.slice(key.indexOf("/") + 1))) continue
      const alias = key.slice(0, key.indexOf("/"))
      try {
        await api(
          `/api/models/custom?providerAlias=${encodeURIComponent(alias)}&id=${encodeURIComponent(
            key.slice(alias.length + 1)
          )}&type=llm`,
          { method: "DELETE" }
        )
        removed += 1
      } catch {
        // Already gone.
      }
    }

    state[entry.provider] = wanted.map((id) => `${entry.alias}/${id}`)

    summary.push({ provider: entry.provider, action: "synced", discovered: discovered.size, wanted: wanted.length, created, removed })
  }

  await writeState(state)
  console.log(JSON.stringify({ at: new Date().toISOString(), source: SYNC_MARKER, summary }, null, 2))
}

main().catch((error) => {
  console.error(`core-provider catalog refresh failed: ${error.message}`)
  process.exitCode = 1
})