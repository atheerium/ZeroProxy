"use client";

import { useState, useEffect } from "react";

interface BuildInfoData {
  currentVersion?: string;
  gitSha?: string;
  buildTime?: string;
  startedAtUnix?: number;
  uptimeSecs?: number;
  pid?: number;
}

export default function BuildInfoBadge() {
  const [info, setInfo] = useState<BuildInfoData | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    let mounted = true;
    const fetchInfo = () => {
      fetch("/api/version", { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : null))
        .then((data: any) => {
          if (!mounted) return;
          const b = data?.buildInfo || data?.runtimeInfo || {};
          const version = b.version || data?.currentVersion || "?";
          const sha = b.gitSha || "unknown";
          const buildTime = b.buildTime || data?.buildInfo?.buildTime || "-";
          const uptimeSec = b.uptimeSecs != null ? b.uptimeSecs : 0;
          setInfo({
            currentVersion: version,
            gitSha: sha,
            buildTime,
            startedAtUnix: b.startedAtUnix,
            uptimeSecs: uptimeSec,
            pid: data?.pid || b.pid,
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
    return () => {
      mounted = false;
      clearInterval(timer);
    };
  }, []);

  if (!visible || !info) return null;

  const shaShort = info.gitSha ? info.gitSha.slice(0, 7) : "-";
  const uptimeMin = info.uptimeSecs != null ? Math.round(info.uptimeSecs / 60) : 0;
  const uptimeStr = uptimeMin > 60 ? `${Math.round(uptimeMin / 60)}h` : `${uptimeMin}m`;

  const tooltip = [
    `v${info.currentVersion}`,
    `sha ${shaShort}`,
    `build ${info.buildTime}`,
    `up ${uptimeStr}`,
    `pid ${info.pid ?? "-"}`,
  ].join(" · ");

  return (
    <a
      href="#"
      title={tooltip}
      onClick={(e) => {
        e.preventDefault();
        // Refresh immediately on click for quick verification.
        fetch("/api/version", { cache: "no-store" })
          .then((r) => r.ok ? r.json() : null)
          .then((d: any) => {
            const b = d?.buildInfo || d?.runtimeInfo || {};
            setInfo({
              currentVersion: b.version || d?.currentVersion || "?",
              gitSha: b.gitSha || "-",
              buildTime: b.buildTime || "-",
              uptimeSecs: b.uptimeSecs ?? 0,
              pid: b.pid,
            });
          })
          .catch(() => {});
      }}
      className="inline-flex items-center gap-1.5 px-2.5 h-7 rounded-full border border-hairline bg-surface/60 text-[11px] font-medium text-text-muted hover:text-ink hover:border-brand-coral/30 transition-colors shrink-0 leading-none"
      aria-label={`Build info: version ${info.currentVersion}, sha ${shaShort}, uptime ${uptimeStr}`}
    >
      <span className="material-symbols-outlined text-[14px]">memory</span>
      <span className="inline-block w-[6px] h-[6px] rounded-full bg-emerald-400 animate-pulse mr-0.5" />
      <span className="hidden sm:inline">LIVE v{info.currentVersion}</span>
      <span className="text-text-muted/60">|</span>
      <span className="font-mono text-[10px]">{shaShort}</span>
      <span className="text-text-muted/40 hidden md:inline">· {uptimeStr}</span>
    </a>
  );
}
