"use client";
import { useState, useEffect } from "react";
import { Card, Button } from "@/shared/components";

export default function ComboDetailClient({ comboId }: { comboId: string }) {
  const [combo, setCombo] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [probeResults, setProbeResults] = useState<any[]>([]);
  const [probing, setProbing] = useState(false);

  // 1) Resolve id from runtime URL (the Astro shell passes "_dynamic" at build time).
  // 2) Fetch the single combo directly via the live id-keyed endpoint.
  useEffect(() => {
    const pathId = window.location.pathname.split("/").pop() || comboId || "";
    if (!pathId || pathId === "_dynamic") {
      setCombo(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    setFetchError(null);
    fetch(`/api/combos/${encodeURIComponent(pathId)}`, { cache: "no-store" })
      .then(async (r) => {
        if (r.status === 404) {
          setCombo(null);
          setLoading(false);
          return;
        }
        if (!r.ok) {
          setFetchError(`HTTP ${r.status}`);
          setLoading(false);
          // Still attempt to parse in case backend returns a body.
          return null;
        }
        return r.json();
      })
      .then((data) => {
        if (data !== null && data !== undefined) {
          setCombo(data);
          setFetchError(null);
        }
        setLoading(false);
      })
      .catch((e) => {
        setFetchError(String(e));
        setLoading(false);
      });
  }, [comboId]);

  const handleProbe = async () => {
    if (!combo || !Array.isArray(combo.models)) return;
    setProbing(true);
    setProbeResults([]);
    try {
      const results: any[] = [];
      for (const model of combo.models) {
        try {
          const res = await fetch("/api/combos/test-model", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            cache: "no-store",
            body: JSON.stringify({ model }),
          });
          const data = await res.json();
          results.push(data);
        } catch (e: any) {
          results.push({ model, ok: false, error: String(e) });
        }
      }
      setProbeResults(results);
    } finally {
      setProbing(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold">Combo Control Center</h1>
      {loading ? (
        <p className="text-sm text-text-muted">Loading combo...</p>
      ) : fetchError ? (
        <Card>
          <p className="text-sm text-red-600">Fetch error: {fetchError}</p>
        </Card>
      ) : !combo ? (
        <p className="text-sm text-text-muted">Combo not found.</p>
      ) : (
        <Card>
          <div className="flex items-center gap-3 mb-3">
            <h2 className="font-mono text-lg font-medium">{combo.name}</h2>
            <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-600">
              <span className="material-symbols-outlined text-[12px]">monitoring</span>
              Active
            </span>
          </div>
          <div className="text-sm text-text-muted mb-3">
            Members: {combo.models?.join(", ") || "—"}
          </div>
          <div className="flex gap-2">
            <Button
              icon="monitoring"
              size="sm"
              disabled={probing || !Array.isArray(combo.models) || combo.models.length === 0}
              loading={probing}
              onClick={handleProbe}
            >
              Probe All
            </Button>
          </div>
          {probeResults.length > 0 && (
            <div className="mt-4 border-t pt-3">
              <h3 className="text-xs font-semibold uppercase tracking-wide text-text-muted mb-2">Probe results</h3>
              <div className="flex flex-wrap gap-2">
                {probeResults.map((r, i) => (
                  <span
                    key={i}
                    className={`inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium ${
                      r.ok ? "bg-emerald-50 text-emerald-700" : "bg-red-50 text-red-700"
                    }`}
                    title={r.error || (r.ok ? `latency ${r.latencyMs}ms` : "")}
                  >
                    <span className="material-symbols-outlined text-[10px]">{r.ok ? "check_circle" : "error"}</span>
                    {r.model || r.model} {r.ok ? `${r.latencyMs ?? "?"}ms` : "fail"}
                  </span>
                ))}
              </div>
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
