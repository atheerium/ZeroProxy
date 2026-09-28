"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Card, Input } from "@/shared/components";
import Modal, { ConfirmModal } from "@/shared/components/Modal";

// ──────────────────────────────────────────────────────────────────────
// Types — mirror src/db/backups.rs and src/server/api/db_backups.rs.
// ──────────────────────────────────────────────────────────────────────

interface BackupInfo {
  id: string;
  filename: string;
  createdAt: string;
  size: number;
  reason: string;
  providerCount: number;
  comboCount: number;
  apiKeyCount: number;
}

interface ListResponse {
  backups: BackupInfo[];
  maxFiles: number;
  retentionDays: number;
  autoDisabled: boolean;
}

type StatusMessage = { type: "success" | "error" | "info"; text: string };

const REASON_LABEL: Record<string, string> = {
  auto: "Auto",
  manual: "Manual",
  "pre-restore": "Pre-restore",
  "pre-import": "Pre-import",
};

function StatusAlert({ status }: { status: StatusMessage | null }) {
  if (!status) return null;
  const cls =
    status.type === "success"
      ? "border-green-300 bg-green-50 text-green-800 dark:border-green-700 dark:bg-green-900/30 dark:text-green-200"
      : status.type === "error"
      ? "border-red-300 bg-red-50 text-red-800 dark:border-red-700 dark:bg-red-900/30 dark:text-red-200"
      : "border-blue-300 bg-blue-50 text-blue-800 dark:border-blue-700 dark:bg-blue-900/30 dark:text-blue-200";
  return (
    <div className={`mt-3 rounded-md border px-3 py-2 text-sm ${cls}`}>{status.text}</div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export default function DbBackupsPageClient() {
  const [data, setData] = useState<ListResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<StatusMessage | null>(null);
  const [pendingRestore, setPendingRestore] = useState<BackupInfo | null>(null);
  const [pendingDelete, setPendingDelete] = useState<BackupInfo | null>(null);
  const [pendingCleanup, setPendingCleanup] = useState<boolean>(false);
  const [pendingImport, setPendingImport] = useState<File | null>(null);
  const [requireLogin, setRequireLogin] = useState(false);
  const [hasPassword, setHasPassword] = useState(false);
  const [dbAuth, setDbAuth] = useState<{
    open: boolean;
    mode: "" | "export" | "import";
    password: string;
  }>({ open: false, mode: "", password: "" });
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Scoped data-management (fresh-start export/import/reset per domain)
  const SCOPED_OPTIONS = [
    { value: "apiKeys", label: "API Keys" },
    { value: "providerCredentials", label: "Provider Credentials" },
    { value: "combos", label: "Combos" },
    { value: "usage", label: "Usage" },
  ] as const;
  const [scopedScopes, setScopedScopes] = useState<string[]>(["apiKeys", "providerCredentials", "combos", "usage"]);
  const [scopedPassword, setScopedPassword] = useState("");
  const [resetConfirm, setResetConfirm] = useState("");
  const [scopedStatus, setScopedStatus] = useState<StatusMessage | null>(null);
  const [scopedImportFile, setScopedImportFile] = useState<File | null>(null);
  const [showResetConfirm, setShowResetConfirm] = useState(false);
  const scopedImportInputRef = useRef<HTMLInputElement>(null);

  const fetchList = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch("/api/db-backups");
      if (!res.ok) throw new Error(`Server returned ${res.status}`);
      const json = (await res.json()) as ListResponse;
      setData(json);
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Failed to load backups: ${err.message}` : "Failed to load backups",
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchList();
    // Load settings so we know whether password re-auth is required.
    void (async () => {
      try {
        const res = await fetch("/api/settings");
        if (!res.ok) return;
        const json = (await res.json()) as {
          requireLogin?: boolean;
          hasPassword?: boolean;
        };
        setRequireLogin(json.requireLogin === true);
        setHasPassword(json.hasPassword === true);
      } catch {
        // Best-effort; export/import still works without re-auth when
        // requireLogin is off (server skips the check).
      }
    })();
  }, [fetchList]);

  const needsDbPasswordReauth = requireLogin && hasPassword;

  const handleCreate = useCallback(async () => {
    setBusy(true);
    setStatus(null);
    try {
      const res = await fetch("/api/db-backups", { method: "PUT" });
      if (!res.ok) throw new Error(`Server returned ${res.status}`);
      const json = await res.json();
      if (json?.created === false) {
        setStatus({ type: "info", text: json?.message || "Backup skipped." });
      } else {
        setStatus({ type: "success", text: `Snapshot created: ${json?.backup?.id ?? "(no id)"}` });
      }
      await fetchList();
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Failed to create backup: ${err.message}` : "Failed to create backup",
      });
    } finally {
      setBusy(false);
    }
  }, [fetchList]);

  const handleDelete = useCallback((backup: BackupInfo) => {
    setPendingDelete(backup);
  }, []);

  const confirmDelete = useCallback(async () => {
    const target = pendingDelete;
    if (!target) return;
    setPendingDelete(null);
    setBusy(true);
    setStatus(null);
    try {
      const res = await fetch(`/api/db-backups/${encodeURIComponent(target.id)}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`Server returned ${res.status}`);
      setStatus({ type: "success", text: "Backup deleted." });
      await fetchList();
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Failed to delete: ${err.message}` : "Failed to delete",
      });
    } finally {
      setBusy(false);
    }
  }, [pendingDelete, fetchList]);

  const handleCleanup = useCallback(() => {
    setPendingCleanup(true);
  }, []);

  const confirmCleanup = useCallback(async () => {
    setPendingCleanup(false);
    setBusy(true);
    setStatus(null);
    try {
      const res = await fetch("/api/db-backups", {
        method: "DELETE",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({}),
      });
      if (!res.ok) throw new Error(`Server returned ${res.status}`);
      const json = await res.json();
      const r = json?.result;
      setStatus({
        type: "success",
        text: `Cleanup ran (deleted ${r?.deletedFiles ?? 0}, kept ${r?.keptFiles ?? 0}).`,
      });
      await fetchList();
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Cleanup failed: ${err.message}` : "Cleanup failed",
      });
    } finally {
      setBusy(false);
    }
  }, [fetchList]);

  const confirmRestore = useCallback(async () => {
    const target = pendingRestore;
    if (!target) return;
    setPendingRestore(null);
    setBusy(true);
    setStatus(null);
    try {
      const res = await fetch("/api/db-backups/restore", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ backupId: target.id }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `Server returned ${res.status}`);
      }
      const json = await res.json();
      setStatus({
        type: "success",
        text: `Restored ${target.id} — ${json?.providerCount ?? 0} providers, ${
          json?.comboCount ?? 0
        } combos, ${json?.apiKeyCount ?? 0} API keys.`,
      });
      await fetchList();
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Restore failed: ${err.message}` : "Restore failed",
      });
    } finally {
      setBusy(false);
    }
  }, [pendingRestore, fetchList]);

  const runExport = useCallback(async (password?: string) => {
    setBusy(true);
    setStatus(null);
    try {
      const headers: Record<string, string> = {};
      if (password) headers["x-op-password"] = password;
      const res = await fetch("/api/db-backups/export", { headers });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error(
          (data as { error?: string }).error || `Server returned ${res.status}`,
        );
      }
      const blob = await res.blob();
      const disposition = res.headers.get("content-disposition") || "";
      const match = disposition.match(/filename="?([^"]+)"?/i);
      const filename = match?.[1] || `ZeroProxy-db-${Date.now()}.json`;
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename;
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);
      URL.revokeObjectURL(url);
      setStatus({ type: "success", text: "Database exported" });
    } catch (err) {
      setStatus({
        type: "error",
        text: err instanceof Error ? `Export failed: ${err.message}` : "Export failed",
      });
    } finally {
      setBusy(false);
    }
  }, []);

  const handleExport = useCallback(() => {
    if (needsDbPasswordReauth) {
      setDbAuth({ open: true, mode: "export", password: "" });
    } else {
      void runExport();
    }
  }, [needsDbPasswordReauth, runExport]);

  const handleImportClick = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  const handleImportFile = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (!file) return;
      setPendingImport(file);
    },
    [],
  );

  const runImport = useCallback(
    async (file: File, password?: string) => {
      setBusy(true);
      setStatus(null);
      try {
        const form = new FormData();
        form.append("file", file);
        const headers: Record<string, string> = {};
        if (password) headers["x-op-password"] = password;
        const res = await fetch("/api/db-backups/import", {
          method: "POST",
          headers,
          body: form,
        });
        if (!res.ok) {
          const data = await res.json().catch(async () => {
            const text = await res.text().catch(() => "");
            return { error: text };
          });
          throw new Error(
            (data as { error?: string }).error || `Server returned ${res.status}`,
          );
        }
        const json = await res.json();
        setStatus({
          type: "success",
          text: `Imported ${file.name} — ${json?.providerCount ?? 0} providers, ${
            json?.comboCount ?? 0
          } combos, ${json?.apiKeyCount ?? 0} API keys.`,
        });
        await fetchList();
      } catch (err) {
        setStatus({
          type: "error",
          text: err instanceof Error ? `Import failed: ${err.message}` : "Import failed",
        });
      } finally {
        setBusy(false);
      }
    },
    [fetchList],
  );

  const confirmImport = useCallback(async () => {
    const file = pendingImport;
    if (!file) return;
    if (needsDbPasswordReauth) {
      // Keep pendingImport; password modal will consume it on confirm.
      setDbAuth({ open: true, mode: "import", password: "" });
      return;
    }
    setPendingImport(null);
    await runImport(file);
  }, [pendingImport, needsDbPasswordReauth, runImport]);

  const handleDbAuthConfirm = useCallback(async () => {
    const { mode, password } = dbAuth;
    setDbAuth({ open: false, mode: "", password: "" });
    if (mode === "export") {
      await runExport(password);
    } else if (mode === "import") {
      const file = pendingImport;
      setPendingImport(null);
      if (file) await runImport(file, password);
    }
  }, [dbAuth, runExport, runImport, pendingImport]);

  const toggleScopedScope = useCallback((value: string) => {
    setScopedScopes((prev) => (prev.includes(value) ? prev.filter((v) => v !== value) : [...prev, value]));
  }, []);

  const handleScopedExport = useCallback(async () => {
    if (scopedScopes.length === 0) {
      setScopedStatus({ type: "error", text: "Pick at least one scope to export." });
      return;
    }
    if (needsDbPasswordReauth && !scopedPassword) {
      setScopedStatus({ type: "error", text: "Password required for export (requireLogin is enabled)." });
      return;
    }
    setBusy(true);
    setScopedStatus(null);
    try {
      const qs = encodeURIComponent(scopedScopes.join(","));
      const headers: Record<string, string> = {};
      if (scopedPassword) headers["x-op-password"] = scopedPassword;
      const res = await fetch(`/api/data/export?scopes=${qs}`, { headers });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error((data as { error?: string }).error || `Server returned ${res.status}`);
      }
      const blob = await res.blob();
      const disposition = res.headers.get("content-disposition") || "";
      const m = disposition.match(/filename="?([^"]+)"?/i);
      const filename = m?.[1] || `ZeroProxy-data-${scopedScopes.join("-")}-${Date.now()}.json`;
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      setScopedStatus({ type: "success", text: `Exported ${scopedScopes.join(", ")} → ${filename}` });
    } catch (err) {
      setScopedStatus({ type: "error", text: err instanceof Error ? `Export failed: ${err.message}` : "Export failed" });
    } finally {
      setBusy(false);
    }
  }, [scopedScopes, scopedPassword, needsDbPasswordReauth]);

  const handleScopedImportFileChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0] || null;
    e.target.value = "";
    if (f) setScopedImportFile(f);
  }, []);

  const handleScopedImport = useCallback(async () => {
    const file = scopedImportFile;
    if (!file) {
      setScopedStatus({ type: "error", text: "Pick a JSON file to import." });
      return;
    }
    if (needsDbPasswordReauth && !scopedPassword) {
      setScopedStatus({ type: "error", text: "Password required for import." });
      return;
    }
    setBusy(true);
    setScopedStatus(null);
    try {
      const text = await file.text();
      const payload = JSON.parse(text);
      if (scopedPassword) (payload as Record<string, unknown>).password = scopedPassword;
      const res = await fetch("/api/data/import", {
        method: "POST",
        headers: { "content-type": "application/json", ...(scopedPassword ? { "x-op-password": scopedPassword } : {}) },
        body: JSON.stringify(payload),
      });
      if (!res.ok) {
        const data = await res.json().catch(async () => ({ error: await res.text().catch(() => "") }));
        throw new Error((data as { error?: string }).error || `Server returned ${res.status}`);
      }
      const json = await res.json();
      setScopedStatus({ type: "success", text: `Imported ${file.name} — ${json?.imported?.providerConnections ?? 0} providers, ${json?.imported?.combos ?? 0} combos, ${json?.imported?.apiKeys ?? 0} keys, ${json?.imported?.usageEntries ?? 0} usage.` });
      setScopedImportFile(null);
      await fetchList();
    } catch (err) {
      setScopedStatus({ type: "error", text: err instanceof Error ? `Import failed: ${err.message}` : "Import failed" });
    } finally {
      setBusy(false);
    }
  }, [scopedImportFile, scopedPassword, needsDbPasswordReauth, fetchList]);

  const handleScopedReset = useCallback(async () => {
    if (scopedScopes.length === 0) {
      setScopedStatus({ type: "error", text: "Pick at least one scope to reset." });
      return;
    }
    if (resetConfirm.trim() !== "RESET") {
      setScopedStatus({ type: "error", text: 'Type RESET to confirm.' });
      return;
    }
    if (needsDbPasswordReauth && !scopedPassword) {
      setScopedStatus({ type: "error", text: "Password required to reset." });
      return;
    }
    setShowResetConfirm(false);
    setBusy(true);
    setScopedStatus(null);
    try {
      const res = await fetch("/api/data/reset", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...(scopedPassword ? { "x-op-password": scopedPassword } : {}),
        },
        body: JSON.stringify({ scopes: scopedScopes, confirm: "RESET", password: scopedPassword || undefined }),
      });
      if (!res.ok) {
        const data = await res.json().catch(async () => ({ error: await res.text().catch(() => "") }));
        throw new Error((data as { error?: string }).error || `Server returned ${res.status}`);
      }
      const json = await res.json();
      setScopedStatus({
        type: "success",
        text: `Reset done — cleared ${json?.reset?.cleared?.join(", ") || scopedScopes.join(", ")} (now ${json?.reset?.provider_count ?? 0} providers, ${json?.reset?.combo_count ?? 0} combos, ${json?.reset?.api_key_count ?? 0} keys, ${json?.reset?.usage_entries ?? 0} usage). A pre-reset backup was saved.`,
      });
      setResetConfirm("");
      await fetchList();
    } catch (err) {
      setScopedStatus({ type: "error", text: err instanceof Error ? `Reset failed: ${err.message}` : "Reset failed" });
    } finally {
      setBusy(false);
    }
  }, [scopedScopes, resetConfirm, scopedPassword, needsDbPasswordReauth, fetchList]);

  const backups = data?.backups ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold text-ink">Database Backups</h1>
        <p className="text-sm text-body mt-1">
          Hourly snapshots of <code>db.json</code> with retention. Restore, export,
          and import full databases here.
        </p>
      </div>

      <Card>
        <div className="p-4 flex flex-wrap items-center gap-2 justify-between">
          <div className="text-sm text-body">
            {data ? (
              <>
                <span className="font-medium text-ink">{backups.length}</span> snapshot
                {backups.length === 1 ? "" : "s"} · retention: max{" "}
                <span className="font-medium text-ink">{data.maxFiles}</span> files
                {data.retentionDays > 0 ? `, ${data.retentionDays} days` : ", no day cutoff"}
                {data.autoDisabled ? " · auto-backup DISABLED" : ""}
              </>
            ) : (
              "Loading…"
            )}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={() => void fetchList()} disabled={loading || busy}>
              Refresh
            </Button>
            <Button onClick={() => void handleCreate()} disabled={busy}>
              {busy ? "Working…" : "Create snapshot"}
            </Button>
            <Button variant="secondary" onClick={() => void handleCleanup()} disabled={busy}>
              Prune
            </Button>
            <Button variant="secondary" onClick={handleExport} disabled={busy}>
              Export db.json
            </Button>
            <Button variant="secondary" onClick={handleImportClick} disabled={busy}>
              Import db.json
            </Button>
            <input
              ref={fileInputRef}
              type="file"
              accept=".json,application/json"
              className="hidden"
              onChange={handleImportFile}
            />
          </div>
        </div>
        <StatusAlert status={status} />
      </Card>

      <Card>
        <div className="p-4 space-y-4">
          <div>
            <h2 className="text-base font-semibold text-ink">Fresh start · scoped data management</h2>
            <p className="text-sm text-body mt-1">
              Clear all cache for a fresh start, or export / import only the domains you need: <code>API keys + provider credentials</code>, <code>combos</code>, <code>usage</code>.
              Exports are filtered server-side via <code>/api/data/export?scopes=…</code>; imports merge only the domains present; reset wipes only the chosen scopes (requires <code>RESET</code> + password when login is required). A pre-reset / pre-import snapshot is saved automatically.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            {SCOPED_OPTIONS.map((o) => (
              <label key={o.value} className="inline-flex items-center gap-1.5 rounded border px-2.5 py-1.5 text-sm cursor-pointer select-none border-line hover:bg-surface-soft">
                <input
                  type="checkbox"
                  checked={scopedScopes.includes(o.value)}
                  onChange={() => toggleScopedScope(o.value)}
                  className="rounded"
                />
                {o.label}
                <span className="font-mono text-[11px] text-body">({o.value})</span>
              </label>
            ))}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setScopedScopes(SCOPED_OPTIONS.map((o) => o.value))}
              disabled={busy}
            >
              All
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setScopedScopes([])} disabled={busy}>
              None
            </Button>
          </div>

          {needsDbPasswordReauth && (
            <div className="flex flex-wrap items-center gap-2">
              <Input
                type="password"
                placeholder="Current password (required for export/import/reset)"
                value={scopedPassword}
                onChange={(e) => setScopedPassword(e.target.value)}
                className="max-w-sm"
              />
              <span className="text-xs text-body">Used as <code>x-op-password</code> + <code>password</code> field for re-auth.</span>
            </div>
          )}

          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={() => void handleScopedExport()} disabled={busy || scopedScopes.length === 0}>
              Export scoped JSON
            </Button>
            <Button variant="secondary" onClick={() => scopedImportInputRef.current?.click()} disabled={busy}>
              Pick import file
            </Button>
            <input ref={scopedImportInputRef} type="file" accept=".json,application/json" className="hidden" onChange={handleScopedImportFileChange} />
            {scopedImportFile && (
              <span className="self-center text-sm text-body">
                <span className="font-mono text-xs">{scopedImportFile.name}</span> · {(scopedImportFile.size / 1024).toFixed(1)} KB
              </span>
            )}
            <Button variant="secondary" onClick={() => void handleScopedImport()} disabled={busy || !scopedImportFile}>
              Import scoped file
            </Button>
            <Button variant="danger" onClick={() => setShowResetConfirm(true)} disabled={busy || scopedScopes.length === 0}>
              Reset selected scopes
            </Button>
          </div>

          {needsDbPasswordReauth && (
            <div className="flex items-center gap-2">
              <Input placeholder='Type RESET to confirm' value={resetConfirm} onChange={(e) => setResetConfirm(e.target.value)} className="max-w-[200px]" />
              <span className="text-xs text-body">Reset needs <code>RESET</code> + password. A pre-reset backup is saved.</span>
            </div>
          )}
          {!needsDbPasswordReauth && (
            <div className="flex items-center gap-2">
              <Input placeholder='Type RESET to confirm' value={resetConfirm} onChange={(e) => setResetConfirm(e.target.value)} className="max-w-[200px]" />
            </div>
          )}

          <StatusAlert status={scopedStatus} />
        </div>
      </Card>

      <Card>
        <div className="overflow-x-auto">
          <table className="min-w-full text-sm">
            <thead className="text-left text-body uppercase text-xs tracking-wide">
              <tr>
                <th className="px-4 py-2">Snapshot</th>
                <th className="px-4 py-2">Reason</th>
                <th className="px-4 py-2">Created</th>
                <th className="px-4 py-2">Size</th>
                <th className="px-4 py-2">Providers</th>
                <th className="px-4 py-2">Combos</th>
                <th className="px-4 py-2">API keys</th>
                <th className="px-4 py-2 text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {loading && (
                <tr>
                  <td colSpan={8} className="px-4 py-6 text-center text-body">
                    Loading backups…
                  </td>
                </tr>
              )}
              {!loading && backups.length === 0 && (
                <tr>
                  <td colSpan={8} className="px-4 py-6 text-center text-body">
                    No backups yet. Click <span className="font-medium">Create snapshot</span> to
                    save one now, or wait for the hourly auto-backup.
                  </td>
                </tr>
              )}
              {!loading &&
                backups.map((b) => (
                  <tr key={b.id} className="hover:bg-surface-soft">
                    <td className="px-4 py-2 font-mono text-xs text-ink">{b.id}</td>
                    <td className="px-4 py-2 text-body">
                      {REASON_LABEL[b.reason] || b.reason}
                    </td>
                    <td className="px-4 py-2 text-body">{formatTime(b.createdAt)}</td>
                    <td className="px-4 py-2 text-body">{formatSize(b.size)}</td>
                    <td className="px-4 py-2 text-body">{b.providerCount}</td>
                    <td className="px-4 py-2 text-body">{b.comboCount}</td>
                    <td className="px-4 py-2 text-body">{b.apiKeyCount}</td>
                    <td className="px-4 py-2 text-right">
                      <div className="inline-flex gap-2">
                        <Button
                          variant="secondary"
                          onClick={() => setPendingRestore(b)}
                          disabled={busy}
                        >
                          Restore
                        </Button>
                        <Button
                          variant="secondary"
                          onClick={() => handleDelete(b)}
                          disabled={busy}
                        >
                          Delete
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </Card>

      <ConfirmModal
        isOpen={!!pendingRestore}
        onClose={() => setPendingRestore(null)}
        onConfirm={confirmRestore}
        title="Restore snapshot?"
        message={pendingRestore ? (
          <>
            <p>
              This will replace the current database with the contents of{" "}
              <span className="font-mono">{pendingRestore.id}</span>. A pre-restore safety
              snapshot will be created first.
            </p>
            <p className="mt-2">
              {pendingRestore.providerCount} providers · {pendingRestore.comboCount} combos ·{" "}
              {pendingRestore.apiKeyCount} API keys.
            </p>
          </>
        ) : null}
        confirmText="Restore"
        variant="danger"
        loading={busy}
      />

      <ConfirmModal
        isOpen={!!pendingDelete}
        onClose={() => setPendingDelete(null)}
        onConfirm={confirmDelete}
        title="Delete backup?"
        message={pendingDelete ? <>Delete snapshot <span className="font-mono">{pendingDelete.id}</span>? This cannot be undone.</> : null}
        confirmText="Delete"
        variant="danger"
        loading={busy}
      />

      <ConfirmModal
        isOpen={pendingCleanup}
        onClose={() => setPendingCleanup(false)}
        onConfirm={confirmCleanup}
        title="Prune backups?"
        message="Prune backups using the current retention settings? Snapshots beyond the retention window will be removed."
        confirmText="Prune"
        variant="danger"
        loading={busy}
      />

      <ConfirmModal
        isOpen={showResetConfirm}
        onClose={() => setShowResetConfirm(false)}
        onConfirm={() => void handleScopedReset()}
        title="Reset selected scopes?"
        message={
          <>
            <p>
              This will wipe <span className="font-mono font-medium">{scopedScopes.join(", ") || "(nothing selected)"}</span>. A pre-reset snapshot will be saved first.
            </p>
            <p className="mt-2 text-sm text-body">
              Type <code>RESET</code> in the field above and confirm. {needsDbPasswordReauth ? "Password re-auth is required." : ""}
            </p>
            {resetConfirm.trim() !== "RESET" && <p className="mt-2 text-sm text-red-600">You must type RESET exactly.</p>}
          </>
        }
        confirmText="Reset"
        variant="danger"
        loading={busy}
      />

      <ConfirmModal
        isOpen={!!pendingImport && !dbAuth.open}
        onClose={() => setPendingImport(null)}
        onConfirm={confirmImport}
        title="Import database?"
        message={pendingImport ? <>Import <span className="font-mono">{pendingImport.name}</span>? This replaces the current database. A pre-import snapshot will be created automatically.</> : null}
        confirmText="Import"
        variant="danger"
        loading={busy}
      />

      <Modal
        isOpen={dbAuth.open}
        onClose={() => {
          setDbAuth({ open: false, mode: "", password: "" });
          if (dbAuth.mode === "import") setPendingImport(null);
        }}
        title="Confirm Password"
        size="sm"
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => {
                setDbAuth({ open: false, mode: "", password: "" });
                if (dbAuth.mode === "import") setPendingImport(null);
              }}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleDbAuthConfirm()}
              loading={busy}
              disabled={!dbAuth.password}
            >
              Confirm
            </Button>
          </>
        }
      >
        <p className="text-text-muted mb-3 text-sm">
          Enter your current password to{" "}
          {dbAuth.mode === "export" ? "export" : "import"} the database.
        </p>
        <Input
          type="password"
          value={dbAuth.password}
          onChange={(e) => setDbAuth((s) => ({ ...s, password: e.target.value }))}
          onKeyDown={(e) => {
            if (e.key === "Enter" && dbAuth.password) void handleDbAuthConfirm();
          }}
          placeholder="Current password"
          autoFocus
        />
      </Modal>
    </div>
  );
}
