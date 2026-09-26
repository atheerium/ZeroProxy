"use client";

import { useState, useEffect } from "react";

interface Freshness {
  backendSha: string | null;
  backendBuildTime: string | null;
  backendCommitsBehind: number | null;
  repoHead: { headSha: string; headCommitTime: string } | null;
  sinceSha: string | null;
  sinceCommitsBehind: number | null;
}

interface BuildInfoData {
  currentVersion: string;
  buildInfo: {
    version: string;
    gitSha: string;
    buildTime: string;
    startedAtUnix?: number;
    uptimeSecs?: number;
    pid?: number;
  };
  freshness?: Freshness;
}

function relativeTime(ts: string | null | undefined): string {
  if (!ts || ts === "unknown") return "unknown";
  const then = new Date(ts).getTime();
  if (isNaN(then)) return "unknown";
  const diff = Date.now() - then;
  if (diff < 0) return "just now";
  if (diff < 60000) return "just now";
  const min = Math.floor(diff / 60000);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.floor(hr / 24)}d ago`;
}

function shortSha(sha: string | null | undefined): string {
  if (!sha || sha === "unknown") return "unknown";
  return sha.slice(0, 7);
}

function formatUptime(startedAtUnix?: number, uptimeSecs?: number): string {
  if (startedAtUnix && startedAtUnix > 0) {
    const diff = Date.now() - startedAtUnix * 1000;
    if (diff < 0) return "just now";
    const min = Math.floor(diff / 60000);
    if (min < 60) return `${min}m ago`;
    return `${Math.floor(min / 60)}h ago`;
  }
  if (uptimeSecs != null) {
    const min = Math.max(0, Math.round(uptimeSecs / 60));
    if (min < 60) return `${min}m ago`;
    return `${Math.floor(min / 60)}h ago`;
  }
  return "unknown";
}

function determineFreshness(freshness: Freshness | undefined): {
  state: "fresh" | "stale" | "unknown";
  sinceBehind: number | null;
  backendBehind: number | null;
  repoHead: Freshness["repoHead"];
} {
  if (
    freshness == null ||
    freshness.sinceCommitsBehind == null ||
    freshness.backendCommitsBehind == null
  ) {
    return { state: "unknown", sinceBehind: null, backendBehind: null, repoHead: freshness?.repoHead ?? null };
  }
  const state = freshness.sinceCommitsBehind === 0 && freshness.backendCommitsBehind === 0 ? "fresh" : "stale";
  return { state, sinceBehind: freshness.sinceCommitsBehind, backendBehind: freshness.backendCommitsBehind, repoHead: freshness.repoHead };
}

export default function BuildInfoBadge() {
  const [data, setData] = useState<BuildInfoData | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    let mounted = true;
    const fetchInfo = () => {
      fetch(`/api/version?since=${__UI_GIT_SHA__}`, { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : null))
        .then((json: any) => {
          if (!mounted) return;
          if (!json) { setVisible(false); return; }
          const buildInfo = json?.buildInfo || {};
          setData({
            currentVersion: json?.currentVersion || buildInfo.version || "?",
            buildInfo: {
              version: buildInfo.version || "?",
              gitSha: buildInfo.gitSha || "unknown",
              buildTime: buildInfo.buildTime || "-",
              startedAtUnix: buildInfo.startedAtUnix,
              uptimeSecs: buildInfo.uptimeSecs,
              pid: buildInfo.pid,
            },
            freshness: json?.freshness,
          });
          setVisible(true);
        })
        .catch(() => {
          if (!mounted) return;
          setVisible(false);
        });
    };
    fetchInfo();
    const timer = setInterval(fetchInfo, 20000);
    return () => { mounted = false; clearInterval(timer); };
  }, []);

  if (!visible || !data) return null;

  const { buildInfo, freshness } = data;
  const uiSha = __UI_GIT_SHA__;
  const uiShaShort = uiSha === "unknown" ? "unknown" : uiSha.slice(0, 7);
  const f = determineFreshness(freshness);

  const dotClass = f.state === "fresh"
    ? "bg-emerald-400 animate-pulse"
    : f.state === "stale"
    ? "bg-amber-400"
    : "bg-gray-400";

  // Name the stale layer, not just the count — a bare "3 behind" cannot tell the
  // reader which rebuild to run.
  const isStale = f.state === "stale";
  const uiBehind = f.sinceBehind ?? 0;
  const binBehind = f.backendBehind ?? 0;
  const bothStale = isStale && uiBehind > 0 && binBehind > 0;

  const behindLabel = !isStale
    ? null
    : bothStale
    ? `UI+bin ${uiBehind}`
    : uiBehind > 0
    ? `UI ${uiBehind}`
    : `bin ${binBehind}`;

  // `--web-only` rebuilds only the dashboard; `--fast detach` rebuilds and
  // restarts only the binary. Two stale layers therefore need both commands.
  const remedy = !isStale
    ? null
    : bothStale
    ? "./scripts/dev.sh --web-only  &&  ./scripts/dev.sh --fast detach"
    : uiBehind > 0
    ? "./scripts/dev.sh --web-only"
    : "./scripts/dev.sh --fast detach";

  const tooltipLines = [
    `Dashboard bundle: built ${relativeTime(__UI_BUILT_AT__)} · ${uiShaShort} · ${f.state === "unknown" ? "unknown" : `${f.sinceBehind} behind`}`,
    `Backend binary: v${buildInfo.version} · built ${relativeTime(buildInfo.buildTime)} · ${shortSha(buildInfo.gitSha)} · ${f.state === "unknown" ? "unknown" : `${f.backendBehind} behind`}`,
    f.repoHead
      ? `Repo HEAD: ${shortSha(f.repoHead.headSha)} · ${relativeTime(f.repoHead.headCommitTime)}`
      : null,
    `Process: ${formatUptime(buildInfo.startedAtUnix, buildInfo.uptimeSecs)} · pid ${buildInfo.pid ?? "-"}`,
    remedy ? `Remedy: ${remedy}` : null,
  ].filter(Boolean).join("\n");

  return (
    <a
      href="#"
      title={tooltipLines}
      onClick={(e) => {
        e.preventDefault();
        fetch(`/api/version?since=${__UI_GIT_SHA__}`, { cache: "no-store" })
          .then((r) => (r.ok ? r.json() : null))
          .then((d: any) => {
            if (!d) return;
            const b = d?.buildInfo || {};
            setData({
              currentVersion: d?.currentVersion || b.version || "?",
              buildInfo: {
                version: b.version || "?",
                gitSha: b.gitSha || "unknown",
                buildTime: b.buildTime || "-",
                startedAtUnix: b.startedAtUnix,
                uptimeSecs: b.uptimeSecs,
                pid: b.pid,
              },
              freshness: d?.freshness,
            });
          })
          .catch(() => {});
      }}
      className="inline-flex items-center gap-1.5 px-2.5 h-7 rounded-full border border-hairline bg-surface/60 text-[11px] font-medium text-text-muted hover:text-ink hover:border-brand-coral/30 transition-colors shrink-0 leading-none"
      aria-label={`Build info: ${f.state} - ${data.currentVersion}, dashboard sha ${uiShaShort}`}
    >
      <span className="material-symbols-outlined text-[14px]">memory</span>
      <span className={`inline-block w-[6px] h-[6px] rounded-full ${dotClass} mr-0.5`} />
      <span className="hidden sm:inline">v{data.currentVersion}</span>
      <span className="font-mono text-[10px]">{uiShaShort}</span>
      {behindLabel && (
        <span className="text-[10px] font-bold text-amber-500 bg-amber-500/10 px-1.5 py-0.5 rounded">
          {behindLabel}
        </span>
      )}
    </a>
  );
}
