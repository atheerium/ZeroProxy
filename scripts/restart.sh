#!/usr/bin/env bash
# restart-only: kill → verify free → start with --web-dir (no cargo build).
# For DB/config/model-lock-only changes; backend/catalog changes still need cargo rebuild.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-4623}"
BIN_DEBUG="${REPO_ROOT}/target/debug/zeroproxy"
PIDFILE="${HOME}/.zeroproxy/zeroproxy.pid"

kill_robust() {
  echo "== stopping server on port ${PORT} =="
  # Try graceful stop first (clears pidfile + frees socket cleanly).
  "${BIN_DEBUG}" server stop 2>/dev/null || true
  # Kill by cmdline pattern used by dev.sh detach (handles both "server start" and bare forms).
  pkill -f "zeroproxy.*${PORT}" 2>/dev/null || true
  # Kill leftover cipherroute references (legacy binary name in some environments).
  pkill -f "cipherroute.*${PORT}" 2>/dev/null || true
  # Last resort: fuser kills anything holding the TCP port.
  if command -v fuser >/dev/null 2>&1; then
    fuser -k "${PORT}/tcp" 2>/dev/null || true
  fi
  sleep 0.5
}

wait_for_port_free() {
  local tries=0 max=20  # 10s max
  while (( tries < max )); do
    if ! (ss -tlnp 2>/dev/null | grep -q ":${PORT} ") && ! (lsof -iTCP:${PORT} 2>/dev/null | grep -q LISTEN); then
      echo "== port ${PORT} is free =="
      return 0
    fi
    sleep 0.5
    (( tries++ )) || true
  done
  echo "ERROR: port ${PORT} still held after ${max} tries (stale server?)" >&2
  echo "Check: pgrep -a zeroproxy; kill -TERM \$(cat ${PIDFILE} 2>/dev/null)" >&2
  return 1
}

kill_robust
wait_for_port_free

echo "== starting ${BIN_DEBUG} --web-dir ${REPO_ROOT}/web/dist server start --port ${PORT} --detach --no-open =="
"${BIN_DEBUG}" --web-dir "${REPO_ROOT}/web/dist" server start --detach --no-open --port "${PORT}"

# Post-start verification (same contract dev.sh uses).
sleep 2
HEALTH=$(curl -sf --max-time 5 "http://127.0.0.1:${PORT}/health" 2>/dev/null || echo "NO_ANSWER")
echo "== health check =="
if [[ "$HEALTH" == NO_ANSWER ]]; then
  echo "FAIL: /health did not respond (port=${PORT})"
  echo "Binary: $(ls -l ${BIN_DEBUG} | awk '{print $5,$6,$NF}')"
  exit 1
else
  echo "PASS: health is ok (binary mtime=$(stat -c '%Y' ${BIN_DEBUG} 2>/dev/null || echo 'n/a'))"
fi
echo "Server restarted (port ${PORT}). Dashboard: --web-dir live (no binary rebuild needed for web/src changes)."
