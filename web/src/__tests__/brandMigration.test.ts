import { describe, it, expect } from "vitest";
import {
  readMigratedJson,
  writeMigratedJson,
  pickMigratedKey,
  readMigratedRaw,
  writeMigratedRaw,
  findMigratedByPrefix,
  ZEROPROXY_ENDPOINT_PRESETS_KEY,
  ZEROPROXY_CLI_PROVIDER_KEY,
  ZEROPROXY_MODEL_PREFIX,
  LEGACY_CLI_PROVIDER_KEY,
  LEGACY_MODEL_PREFIX,
} from "@/lib/brandMigration";

/** Minimal Web Storage fake. The production contract is getItem/setItem. */
function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => (m.has(k) ? m.get(k)! : null),
    setItem: (k: string, v: string) => void m.set(k, v),
    _raw: m,
  };
}

/**
 * These lock the brand-rename migration contract. Every legacy key below is one
 * that was written into a place the user still owns — a browser's localStorage,
 * a tool's own config file on disk, or a tool's settings — so a plain rename
 * would silently orphan the user's data. The rule is always
 * read-new-then-fall-back-to-legacy, write-new-only.
 */

describe("readMigratedJson", () => {
  it("prefers the new key when both are present", () => {
    const store = fakeStorage({
      "zeroproxy.cliToolEndpointPresets": JSON.stringify(["new"]),
      "cipherroute.cliToolEndpointPresets": JSON.stringify(["old"]),
    });
    expect(readMigratedJson(store, "zeroproxy.cliToolEndpointPresets", "cipherroute.cliToolEndpointPresets")).toEqual([
      "new",
    ]);
  });

  it("falls back to the legacy key so pre-rename presets are not lost", () => {
    const store = fakeStorage({ "cipherroute.cliToolEndpointPresets": JSON.stringify(["old"]) });
    expect(readMigratedJson(store, "zeroproxy.cliToolEndpointPresets", "cipherroute.cliToolEndpointPresets")).toEqual([
      "old",
    ]);
  });

  it("returns null when neither key is set", () => {
    expect(readMigratedJson(fakeStorage(), "a", "b")).toBeNull();
  });

  it("returns null rather than throwing on corrupt JSON", () => {
    const store = fakeStorage({ "zeroproxy.cliToolEndpointPresets": "{not json" });
    expect(readMigratedJson(store, "zeroproxy.cliToolEndpointPresets", "cipherroute.cliToolEndpointPresets")).toBeNull();
  });
});

describe("writeMigratedJson", () => {
  it("writes only the new key and leaves the legacy key untouched", () => {
    const store = fakeStorage({ "cipherroute.cliToolEndpointPresets": JSON.stringify(["old"]) });
    writeMigratedJson(store, "zeroproxy.cliToolEndpointPresets", ["new"]);
    expect(JSON.parse(store.getItem("zeroproxy.cliToolEndpointPresets")!)).toEqual(["new"]);
    expect(JSON.parse(store.getItem("cipherroute.cliToolEndpointPresets")!)).toEqual(["old"]);
  });
});

describe("readMigratedRaw / writeMigratedRaw", () => {
  it("prefers the new key", () => {
    const store = fakeStorage({ "zeroproxy:freeOnly:a": "1", "cipherroute:freeOnly:a": "0" });
    expect(readMigratedRaw(store, "zeroproxy:freeOnly:a", "cipherroute:freeOnly:a")).toBe("1");
  });
  it("falls back to the legacy key so an existing free-only toggle survives", () => {
    const store = fakeStorage({ "cipherroute:freeOnly:a": "0" });
    expect(readMigratedRaw(store, "zeroproxy:freeOnly:a", "cipherroute:freeOnly:a")).toBe("0");
  });
  it("returns null when neither key is set", () => {
    expect(readMigratedRaw(fakeStorage(), "n", "l")).toBeNull();
  });
  it("writes only the new key", () => {
    const store = fakeStorage({ "cipherroute:freeOnly:a": "0" });
    writeMigratedRaw(store, "zeroproxy:freeOnly:a", "1");
    expect(store.getItem("zeroproxy:freeOnly:a")).toBe("1");
    expect(store.getItem("cipherroute:freeOnly:a")).toBe("0");
  });
});

describe("pickMigratedKey", () => {
  it("prefers the new key", () => {
    expect(pickMigratedKey({ zeroproxy: "new", cipherroute: "old" }, "zeroproxy", "cipherroute")).toBe("new");
  });

  it("falls back to the legacy key", () => {
    expect(pickMigratedKey({ cipherroute: "old" }, "zeroproxy", "cipherroute")).toBe("old");
  });

  it("returns undefined when the value is explicitly null in the new key", () => {
    // A provider block that exists but is null must not silently fall through to
    // a stale legacy block that the user already removed.
    expect(pickMigratedKey({ zeroproxy: null, cipherroute: "old" }, "zeroproxy", "cipherroute")).toBeNull();
  });
});

describe("findMigratedByPrefix", () => {
  const rows = [
    { id: "custom:CipherRoute-0", baseUrl: "http://old" },
    { id: "custom:ZeroProxy-0", baseUrl: "http://new" },
  ];

  it("prefers a new-prefix row over a legacy-prefix row", () => {
    expect(findMigratedByPrefix(rows, ZEROPROXY_MODEL_PREFIX, LEGACY_MODEL_PREFIX)?.baseUrl).toBe("http://new");
  });

  it("falls back to the legacy prefix so existing Droid models still resolve", () => {
    expect(findMigratedByPrefix([rows[0]], ZEROPROXY_MODEL_PREFIX, LEGACY_MODEL_PREFIX)?.baseUrl).toBe("http://old");
  });

  it("returns undefined when nothing matches", () => {
    expect(findMigratedByPrefix([{ id: "other" }], ZEROPROXY_MODEL_PREFIX, LEGACY_MODEL_PREFIX)).toBeUndefined();
  });

  it("tolerates rows with no id", () => {
    expect(findMigratedByPrefix([{}], ZEROPROXY_MODEL_PREFIX, LEGACY_MODEL_PREFIX)).toBeUndefined();
  });
});

describe("exported key constants", () => {
  it("pins the exact strings so the old and new keys cannot drift", () => {
    expect(ZEROPROXY_ENDPOINT_PRESETS_KEY).toBe("zeroproxy.cliToolEndpointPresets");
    expect(ZEROPROXY_CLI_PROVIDER_KEY).toBe("zeroproxy");
    expect(LEGACY_CLI_PROVIDER_KEY).toBe("cipherroute");
    expect(ZEROPROXY_MODEL_PREFIX).toBe("custom:ZeroProxy");
    expect(LEGACY_MODEL_PREFIX).toBe("custom:CipherRoute");
  });
});
