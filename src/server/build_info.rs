//! Build-time identity exposed at runtime.
//!
//! `build.rs` bakes `ZEROPROXY_BUILD_TIME` and `ZEROPROXY_GIT_SHA` into the
//! binary. We also capture a `STARTED_AT` instant on first access so the API
//! can report uptime.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn build_time_env() -> String {
    option_env!("ZEROPROXY_BUILD_TIME")
        .unwrap_or("unknown")
        .to_string()
}

fn git_sha_env() -> String {
    option_env!("ZEROPROXY_GIT_SHA").unwrap_or("").to_string()
}

static STARTED: OnceLock<(SystemTime, Instant)> = OnceLock::new();

/// Initialize the start-time registry. Idempotent; safe to call multiple times.
pub fn init() {
    let _ = STARTED.get_or_init(|| (SystemTime::now(), Instant::now()));
}

/// Full identity payload (used by `/api/version`).
pub fn identity() -> serde_json::Value {
    let sha = git_sha_env();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "gitSha": if sha.is_empty() { "unknown".to_string() } else { sha },
        "buildTime": build_time_env(),
    })
}

/// Runtime status payload (used by `/health` and the dashboard badge).
pub fn status() -> serde_json::Value {
    let (sys, inst) = STARTED.get_or_init(|| (SystemTime::now(), Instant::now()));
    let uptime = inst.elapsed();
    let start_unix = sys.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "gitSha": git_sha_env(),
        "buildTime": build_time_env(),
        "startedAtUnix": start_unix,
        "uptimeSecs": uptime.as_secs(),
        "pid": std::process::id() as u32,
    })
}
