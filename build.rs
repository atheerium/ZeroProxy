//! Build script.
//!
//! When the `embed-web` feature is enabled (default for release builds), the
//! Astro static output at `web/dist/` is baked into the binary via
//! `rust-embed`. This script fails the build early with a clear message if
//! `web/dist/index.html` is missing, instead of producing a binary with an
//! empty dashboard.
//!
//! To intentionally build without the embedded UI (smaller binary, requires
//! `--dashboard-sidecar-url` or `--web-dir` at runtime):
//!     cargo build --release --no-default-features

fn main() {
    let embed_enabled = std::env::var("CARGO_FEATURE_EMBED_WEB").is_ok();
    if !embed_enabled {
        return;
    }

    let dist = std::path::Path::new("web/dist/index.html");
    if !dist.exists() {
        // `cargo:warning=` lines are printed without colour but are visible in
        // release builds. We also panic so the build actually fails.
        println!(
            "cargo:warning=web/dist/index.html is missing. \
             Build the dashboard first: (cd web && pnpm install --frozen-lockfile && pnpm run build)"
        );
        panic!(
            "web/dist not built. Run:\n  \
             (cd web && pnpm install --frozen-lockfile && pnpm run build)\n\
             Or build without the embedded UI:\n  \
             cargo build --release --no-default-features"
        );
    }

    // Emit build-time identity env vars so the binary carries its own provenance.
    for (k, v) in build_env() {
        println!("cargo:rustc-env={k}={v}");
    }
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=.git/HEAD");

    // Trigger a rebuild whenever the embedded assets change. Without this,
    // editing `web/dist/...` won't invalidate the existing rust-embed cache
    // and the binary will keep serving stale assets.
    println!("cargo:rerun-if-changed=web/dist");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_EMBED_WEB");

    // Guard: when building the embedded (release) dashboard, refuse to bake a
    // `web/dist/` that is OLDER than the source `web/src/`. Stale embedding is
    // the classic "blank /dashboard/providers, chunk ReferenceError" failure:
    // a developer edits web/src, rebuilds the binary WITHOUT rerunning
    // `pnpm build`, and ships a dashboard predating their fix. Dev (debug)
    // builds are exempt so the normal edit-loop isn't blocked.
    let is_release = std::env::var("PROFILE")
        .map(|p| p == "release")
        .unwrap_or(false);
    if is_release {
        if let Some(msg) = newest_src_newer_than_dist() {
            println!(
                "cargo:warning=web/src is newer than web/dist — embedded dashboard will be STALE. Rebuild it: (cd web && pnpm install && pnpm run build)"
            );
            panic!(
                "web/src is newer than web/dist ({msg}).\n  \
                 The embedded dashboard would be stale and the running server\n  \
                 would serve an old chunk (blank page / ReferenceError).\n  \
                 Fix: (cd web && pnpm install && pnpm run build) then rebuild.\n  \
                 To skip the embed entirely: cargo build --release --no-default-features"
            );
        }
    }
}

/// Returns `Some(reason)` if any file under `web/src` has an mtime strictly
/// newer than `web/dist/index.html` (the build output marker). Walks `web/src`
/// recursively; tolerates missing source dir.
fn build_env() -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Build time (UTC RFC3339)
    let time_str = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    out.push(("ZEROPROXY_BUILD_TIME".into(), time_str));
    // Short git SHA (or empty if unavailable)
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(".")
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8(o.stdout).ok()?;
            let trimmed = s.trim();
            if trimmed.len() >= 4 {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();
    out.push(("ZEROPROXY_GIT_SHA".into(), sha));
    out
}

fn newest_src_newer_than_dist() -> Option<String> {
    let dist_marker = std::path::Path::new("web/dist/index.html");
    let dist_mtime = match std::fs::metadata(dist_marker).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return Some("web/dist/index.html missing".to_string()),
    };
    let src_dir = std::path::Path::new("web/src");
    if !src_dir.exists() {
        return None;
    }
    let mut newest_src: Option<std::time::SystemTime> = None;
    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let md = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                stack.push(path);
            } else if let Ok(mt) = md.modified() {
                newest_src = Some(match newest_src {
                    Some(n) => n.max(mt),
                    None => mt,
                });
            }
        }
    }
    match newest_src {
        Some(src_mt) if src_mt > dist_mtime => Some(format!(
            "src mtime {:?} > dist mtime {:?}",
            src_mt, dist_mtime
        )),
        _ => None,
    }
}
