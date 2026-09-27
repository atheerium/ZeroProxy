/**
 * Detect a free-tier model from its id alone.
 *
 * Providers mark free models by suffix, not by metadata: OpenRouter appends
 * `:free`, most direct vendors append `-free`. Measured across the 1787 model
 * ids in the OmniRoute snapshot plus the built-in catalog: 46 match, split
 * `-free` 24, `:free` 19, `/free` 3, and 3 of the 24 use a capital `Free` --
 * so the match must be case-insensitive or those are silently missed.
 *
 * Deliberately suffix-only. `goldeneye-free-auto` and
 * `gpt-5.6-luna-free-thinking` also contain "free", but in both cases it is an
 * infix segment where reading intent either way is a guess, so they are left
 * unclassified rather than guessed at.
 */
const FREE_SUFFIXES = [":free", "-free", "_free", "/free"] as const;

export function isFreeModelId(modelId: string | null | undefined): boolean {
  if (!modelId) return false;
  const id = modelId.trim().toLowerCase();
  return FREE_SUFFIXES.some((suffix) => id.endsWith(suffix));
}

export function filterFreeModelIds<T extends { id: string }>(
  models: readonly T[]
): T[] {
  return models.filter((model) => isFreeModelId(model.id));
}
