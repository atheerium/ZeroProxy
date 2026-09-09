#!/usr/bin/env bash
# dev build + run loop for CipherRoute
# Runnable from repo root OR any cwd:
#   ./scripts/dev.sh --fast              # fast incremental build (default)
#   ./scripts/dev.sh --full              # full rebuild + checks
#   ./scripts/dev.sh --fast detach       # fast build + restart server
#   ./scripts/dev.sh build               # legacy: only build (fast)
#   PORT=4624 ./scripts/dev.sh detach    # custom port
#   BUILD_MODE=release ./scripts/dev.sh --full  # optimized build
set -euo pipefail

# Resolve repo root regardless of cwd or symlink
SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
REPO_ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"
cd "$REPO_ROOT"

PORT="${PORT:-4623}"
BUILD_MODE="${BUILD_MODE:-debug}"  # debug (incremental, fast) or release

# ── preset / layer flags ──────────────────────────────────────────────
PRESET="fast"           # fast | full
DO_WEB="auto"           # auto | true | false
DO_BACKEND="auto"       # auto | true | false
DO_CHECKS="auto"        # auto (full=>true, fast=>false) | true | false
FORCE_WEB=false
NO_RESTART=false
MODE=""                 # run | build | detach | check | check-stale
SHOW_HELP=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast) PRESET="fast"; shift ;;
    --full) PRESET="full"; FORCE_WEB=true; shift ;;
    --web-only) DO_WEB=true; DO_BACKEND=false; shift ;;
    --backend-only) DO_WEB=false; DO_BACKEND=true; shift ;;
    --release) BUILD_MODE="release"; shift ;;
    --no-restart) NO_RESTART=true; shift ;;
    --port) PORT="$2"; shift 2 ;;
    --port=*) PORT="${1#*=}"; shift ;;
    --check) MODE="check"; shift ;;
    --help|-h) SHOW_HELP=true; shift ;;
    --) shift; break ;;
    -*) echo "Unknown option: $1" >&2; exit 1 ;;
    *) # positional mode: run | build | detach | check | check-stale
      if [[ -z "$MODE" ]]; then
        MODE="$1"
      else
        echo "Unknown arg: $1" >&2; exit 1
      fi
      shift
      ;;
  esac
done

# Resolve auto values
if [[ "$DO_CHECKS" == "auto" ]]; then
  if [[ "$PRESET" == "full" ]]; then DO_CHECKS=true; else DO_CHECKS=false; fi
fi
# Explicit --web-only / --backend-only override FORCE_WEB
if [[ "$DO_WEB" == "true" ]]; then FORCE_WEB=true; fi
if [[ "$DO_WEB" == "false" ]]; then FORCE_WEB=false; fi

# Default mode: if no mode given, run fast->build+run, full->build, layer-only->build
if [[ -z "$MODE" ]]; then
  if [[ "$NO_RESTART" == true ]]; then
    MODE="build"
  elif [[ "$DO_WEB" != "auto" || "$DO_BACKEND" != "auto" ]]; then
    # --web-only / --backend-only without explicit run/detach means build only
    MODE="build"
  elif [[ "$PRESET" == "full" && "$DO_CHECKS" == true && "$DO_WEB" == "auto" && "$DO_BACKEND" == "auto" ]]; then
    # bare --full without detach/run means build + checks
    MODE="build"
  else
    MODE="run"
  fi
fi

# Convert legacy MODE=release env to flag
if [[ "$MODE" == "release" ]]; then
  BUILD_MODE="release"
  MODE="run"
fi

BIN_DEBUG="target/debug/zeroproxy"
BIN_RELEASE="target/release/zeroproxy"
BIN="$BIN_DEBUG"
CARGO_ARGS=(build --bin zeroproxy)
if [[ "$BUILD_MODE" == "release" ]]; then
  BIN="$BIN_RELEASE"
  CARGO_ARGS=(build --release --bin zeroproxy)
fi

print_help() {
  cat <<'HELP'
Usage: ./scripts/dev.sh [OPTIONS] [MODE]

OPTIONS (run from repo root — no cd scripts needed):
  --fast          Fast incremental build (default). Cargo debug + web only if stale. ~10-20s.
  --full          Full rebuild + checks. Always rebuilds web, runs fmt/clippy/tests. ~2-5m.
                  (dev mode uses --web-dir so web changes are instant — no binary rebuild needed)
  --web-only      Only rebuild web/dist (pnpm build).
  --backend-only  Only rebuild Rust binary (cargo build).
  --release       Use release profile (implies slower optimized build).
  --port PORT     Server port (default 4623, also $PORT).
  --no-restart    Build only, don't start server (alias for MODE=build).
  -h, --help      Show this help.

MODE (legacy positional, still supported):
  run        Build + start foreground (default, Ctrl+C to stop)
  detach     Build + start detached on 127.0.0.1:$PORT
  build      Only build, don't run
  check      Run fmt, clippy, astro check, tests
  check-stale  Warn if web/src newer than web/dist

Examples:
  ./scripts/dev.sh --fast              # edit web/src → quick rebuild, skip checks
  ./scripts/dev.sh --fast detach       # fast rebuild + restart server (daily loop)
  ./scripts/dev.sh --full              # before push: always web + full checks
  ./scripts/dev.sh --full detach       # full rebuild + restart
  ./scripts/dev.sh --web-only          # touched only dashboard
  ./scripts/dev.sh --backend-only      # touched only src/ Rust
  ./scripts/dev.sh --check             # lint without building
  BUILD_MODE=release ./scripts/dev.sh --full  # optimized release build

When to use which:
  fast  → iterating on one layer, want <20s feedback (provider UI tweaks, single executor)
  full  → before git push / PR, after provider_catalog.json or src/db/ changes, or when "can't find providers" (stale web/dist)
  web-only / backend-only → you know only one layer changed (saves the other rebuild)
HELP
}

kill_port() {
  systemctl --user stop openproxy.service 2>/dev/null || true
  if command -v fuser >/dev/null 2>&1; then
    fuser -k "${PORT}/tcp" 2>/dev/null || true
  fi
  pkill -f "zeroproxy server start"; pkill -f "openproxy" 2>/dev/null || true
  pkill -f "target/.*/zeroproxy.*${PORT}" 2>/dev/null || true
  sleep 0.5
}

check_stale_dashboard() {
  if [[ -d "web/src" && -d "web/dist" ]]; then
    local src_newest dist_newest
    src_newest=$(find web/src -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f1)
    dist_newest=$(find web/dist -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f1)
    if [[ -n "$src_newest" && -n "$dist_newest" ]] && awk "BEGIN{exit !($src_newest > $dist_newest)}"; then
      echo "!! WARNING: web/src newer than web/dist — dashboard may be stale."
      echo "   Run: ./scripts/dev.sh --web-only  or  ./scripts/dev.sh --full"
      if git diff --name-only HEAD 2>/dev/null | grep -q "web/src/shared/constants/providers"; then
        echo "   Detected provider catalog change without rebuild — /dashboard/providers may show stale list."
      fi
      return 0  # stale
    fi
    return 1  # not stale
  elif [[ -d "web/src" && ! -d "web/dist" ]]; then
    echo "!! web/dist missing — run: ./scripts/dev.sh --web-only"
    return 0
  fi
  return 1
}

is_web_stale() {
  check_stale_dashboard >/dev/null 2>&1
}

check_dirty_tree() {
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "!! Dirty working tree detected:"
    git status --porcelain | head -n 20
    echo "   Commit or stash before switching worktrees/branches to avoid cross-agent conflicts."
  fi
}

run_checks() {
  echo "== checks: fmt, clippy, provider_models tests =="
  cargo fmt --all -- --check
  echo "fmt ok"
  # Match CI: warnings tolerated (clippy exits 0 with 99 pre-existing warnings on main).
  # Without -D warnings this fails only on real errors, so --full is a usable pre-push gate.
  cargo clippy --all-targets --all-features 2>&1 | tail -n 20
  echo "clippy ok"
  if [[ -d "web" ]]; then
    pnpm --dir web exec astro check 2>&1 | tail -n 30 || echo "astro check advisory (fix new errors)"
  fi
  cargo test -p zeroproxy --lib provider_models -- --nocapture 2>&1 | tail -n 30
  echo "checks passed"
}

build_web() {
  if [[ ! -d "web" ]]; then
    echo "== web: no web/ dir, skipping =="
    return 0
  fi
  echo "== pnpm --prefix web build =="
  pnpm --prefix web build
  echo "== web/dist built =="
  ls -lh web/dist/_astro/ProvidersPageClient.*.js 2>/dev/null | head -1 || ls -lh web/dist | head -5
}

build_backend() {
  echo "== cargo ${CARGO_ARGS[*]} =="
  cargo "${CARGO_ARGS[@]}"
  echo "== built $BIN =="
  ls -lh "$BIN" | awk '{print $9, $5, $6, $7, $8}'
}

build() {
  local did_any=false

  # Decide web
  local do_web_build=false
  if [[ "$DO_WEB" == "true" ]]; then
    do_web_build=true
  elif [[ "$DO_WEB" == "false" ]]; then
    do_web_build=false
  elif [[ "$FORCE_WEB" == true ]]; then
    do_web_build=true
  elif is_web_stale; then
    echo "== web/src stale → rebuilding web =="
    do_web_build=true
  else
    echo "== web/dist up-to-date → skipping web build (use --web-only or --full to force) =="
  fi

  if [[ "$do_web_build" == true ]]; then
    build_web
    did_any=true
  fi

  # Decide backend
  local do_backend_build=false
  if [[ "$DO_BACKEND" == "true" ]]; then
    do_backend_build=true
  elif [[ "$DO_BACKEND" == "false" ]]; then
    do_backend_build=false
  else
    do_backend_build=true
  fi

  if [[ "$do_backend_build" == true ]]; then
    # Still warn about stale even if we skipped web build
    if [[ "$do_web_build" == false ]]; then
      check_stale_dashboard || true
    fi
    build_backend
    did_any=true
  fi

  if [[ "$did_any" == false ]]; then
    echo "Nothing to build."
  fi

  # Full preset runs checks after builds
  if [[ "$PRESET" == "full" && "$DO_CHECKS" == true && "$MODE" == "build" ]]; then
    echo ""
    run_checks
  fi
}

if [[ "$SHOW_HELP" == true ]]; then
  print_help
  exit 0
fi

case "$MODE" in
  build)
    build
    if [[ "$PRESET" == "full" && "$DO_CHECKS" == true ]]; then
      echo "Full build done."
    else
      echo "Build done. Run ./scripts/dev.sh --fast detach to start."
    fi
    ;;
  check)
    run_checks
    ;;
  check-stale)
    check_stale_dashboard || echo "web/dist up-to-date."
    ;;
  detach)
    check_dirty_tree || true
    kill_port
    build
    echo "== starting $BIN --web-dir web/dist server start --port $PORT --detach --no-open =="
    "$BIN" --web-dir "$REPO_ROOT/web/dist" server start --detach --no-open --port "$PORT"
    echo "== status =="
    "$BIN" --robot server status 2>&1 | head -n 20 || curl -sf "http://127.0.0.1:${PORT}/health" && echo "health ok"
    echo "Logs: tail -f ~/.zeroproxy/log.txt  (or journalctl --user -u zeroproxy -f if using service)"
    echo "Stop: $BIN server stop  or  pkill -f zeroproxy  or  fuser -k ${PORT}/tcp"
    ;;
  run|restart|"")
    check_dirty_tree || true
    kill_port
    build
    echo "== starting $BIN --web-dir web/dist server start --port $PORT (foreground, Ctrl+C to stop) =="
    echo "   Dashboard: http://127.0.0.1:${PORT}"
    echo "   API:       http://127.0.0.1:${PORT}/v1  (Bearer \$CIPHERROUTE_API_KEY)"
    exec "$BIN" --web-dir "$REPO_ROOT/web/dist" server start --port "$PORT" --no-open
    ;;
  *)
    echo "Unknown mode: $MODE" >&2
    print_help
    exit 1
    ;;
esac
