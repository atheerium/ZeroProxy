/**
 * Helpers for the CipherRoute -> ZeroProxy brand rename.
 *
 * The binary was `cipherroute`, then `openproxy`, then `zeroproxy`, and the old
 * spellings were written into three places the *user* owns and we cannot
 * rewrite for them:
 *
 *   1. the browser's localStorage (saved CLI-tool endpoint presets),
 *   2. a CLI tool's own config file on disk (jcode / opencode provider blocks),
 *   3. a CLI tool's settings (Droid's `custom:CipherRoute-N` model ids).
 *
 * A plain find-and-replace in those three places reads as a rename but is a
 * silent data loss: the dashboard would stop seeing every preset, provider
 * block, and custom model a user had already configured. So the rule is always
 * the same, and these functions exist to make it impossible to get wrong at a
 * call site:
 *
 *   READ   new key first, fall back to the legacy key.
 *   WRITE  the new key only — the legacy value is left in place, unread, so a
 *          user who downgrades keeps their data.
 *
 * The new key winning is the important half. A user who has both, having
 * already re-saved under the new name, must not be shown the stale legacy
 * value.
 */

/** localStorage key for saved CLI-tool endpoint presets (post-rename). */
export const ZEROPROXY_ENDPOINT_PRESETS_KEY = "zeroproxy.cliToolEndpointPresets";

/** localStorage key as written by every pre-rename build. */
export const LEGACY_ENDPOINT_PRESETS_KEY = "cipherroute.cliToolEndpointPresets";

/** Provider-block key written into jcode / opencode configs (post-rename). */
export const ZEROPROXY_CLI_PROVIDER_KEY = "zeroproxy";

/** Provider-block key as written by every pre-rename build. */
export const LEGACY_CLI_PROVIDER_KEY = "cipherroute";

/** Custom-model id prefix written into Droid settings (post-rename). */
export const ZEROPROXY_MODEL_PREFIX = "custom:ZeroProxy";

/** Custom-model id prefix as written by every pre-rename build. */
export const LEGACY_MODEL_PREFIX = "custom:CipherRoute";

/** The slice of the Web Storage API these helpers need. */
type StringStore = Pick<Storage, "getItem" | "setItem">;

type StorageLike = {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
};

function asStorageLike(store: StringStore | StorageLike | undefined): StorageLike | undefined {
  if (store) return store as StorageLike;
  if (typeof window === "undefined") return undefined;
  return window.localStorage;
}

/**
 * Read a JSON value from `newKey`, falling back to `legacyKey`.
 *
 * Returns `null` when neither key holds valid JSON. A corrupt value is treated
 * as absent rather than thrown: a hand-edited or truncated localStorage entry
 * must not take down the whole control that reads it.
 */
export function readMigratedJson<T>(store: StringStore | StorageLike, newKey: string, legacyKey: string): T | null {
  const s = asStorageLike(store);
  if (!s) return null;

  for (const key of [newKey, legacyKey]) {
    const raw = s.getItem(key);
    if (raw === null || raw === undefined) continue;
    try {
      return JSON.parse(raw) as T;
    } catch {
      // Fall through to the next key: a corrupt new value should not stop us
      // reading a perfectly good legacy one.
    }
  }
  return null;
}

/** Write `value` to `newKey` only. The legacy entry is deliberately untouched. */
export function writeMigratedJson<T>(store: StringStore | StorageLike, newKey: string, value: T): void {
  const s = asStorageLike(store);
  if (!s) return;
  s.setItem(newKey, JSON.stringify(value));
}

/**
 * Read a raw (non-JSON) string from `newKey`, falling back to `legacyKey`.
 *
 * Used for per-entity keys like the free-only filter toggle, where each alias
 * gets its own `prefix + alias` entry. Returns `null` when neither is set.
 */
export function readMigratedRaw(store: StringStore | StorageLike, newKey: string, legacyKey: string): string | null {
  const s = asStorageLike(store);
  if (!s) return null;
  const next = s.getItem(newKey);
  if (next !== null && next !== undefined) return next;
  return s.getItem(legacyKey) ?? null;
}

/** Write a raw string to `newKey` only; the legacy entry is left untouched. */
export function writeMigratedRaw(store: StringStore | StorageLike, newKey: string, value: string): void {
  const s = asStorageLike(store);
  if (!s) return;
  s.setItem(newKey, value);
}

/**
 * Pick `obj[newKey]`, falling back to `obj[legacyKey]`.
 *
 * A key that is *present but null/undefined* returns that value rather than
 * falling through. That distinction matters for provider blocks: a user who
 * cleared a provider must not have the stale pre-rename block resurrect itself
 * on the next page load.
 */
export function pickMigratedKey<T>(obj: Record<string, T> | null | undefined, newKey: string, legacyKey: string): T | undefined {
  if (!obj) return undefined;
  if (newKey in obj) return obj[newKey];
  if (legacyKey in obj) return obj[legacyKey];
  return undefined;
}

/**
 * Find the first row whose `id` starts with the new prefix, falling back to the
 * legacy prefix. Tolerates rows with no `id` rather than throwing on
 * `undefined?.startsWith`.
 */
export function findMigratedByPrefix<T extends { id?: string }>(
  rows: readonly T[] | null | undefined,
  newPrefix: string,
  legacyPrefix: string,
): T | undefined {
  if (!rows) return undefined;
  return rows.find((r) => typeof r?.id === "string" && r.id.startsWith(newPrefix)) ?? rows.find((r) => typeof r?.id === "string" && r.id.startsWith(legacyPrefix));
}

/** True when a model id belongs to ZeroProxy under either spelling. */
export function isZeroProxyModelId(id: string | undefined | null): boolean {
  if (typeof id !== "string") return false;
  return id.startsWith(ZEROPROXY_MODEL_PREFIX) || id.startsWith(LEGACY_MODEL_PREFIX);
}
