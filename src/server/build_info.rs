//! Build-time identity exposed at runtime.
//!
//! `build.rs` bakes `ZEROPROXY_BUILD_TIME` and `ZEROPROXY_GIT_SHA` into the
//! binary. We also capture a `STARTED_AT` instant on first access so the API
//! can report uptime.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn build_time_env() -> String {
    option_env!("ZEROPROXY_BUILD_TIME")
        .unwrap_or("unknown")
        .to_string()
}

fn git_sha_env() -> String {
    option_env!("ZEROPROXY_GIT_SHA").unwrap_or("").to_string()
}

/// Baked git SHA, or `None` when the binary was built outside a checkout — in
/// which case it must not be fed to [`commits_since`].
pub fn identity_sha() -> Option<String> {
    let sha = git_sha_env();
    (!sha.is_empty()).then_some(sha)
}

/// Baked build timestamp, or `None` when `build.rs` could not produce one.
pub fn identity_build_time() -> Option<String> {
    let time = build_time_env();
    (time != "unknown").then_some(time)
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

// ────────────────────────────────────────────────────────────────────────────
// Repository freshness
// ────────────────────────────────────────────────────────────────────────────
//
// The baked `gitSha` records what was *compiled*; these report what the checkout
// looks like *now*, which is the only way for the dashboard to answer "am I
// running the latest?". All of it degrades to `None` outside a checkout.

const REPO_TTL: Duration = Duration::from_secs(15);

fn git_in(dir: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn repo_root() -> Option<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let top = git_in(&cwd, &["rev-parse", "--show-toplevel"])?;
    Some(std::path::PathBuf::from(top))
}

fn probe_repo_head() -> Option<serde_json::Value> {
    let dir = repo_root()?;
    let head_sha = git_in(&dir, &["rev-parse", "--short", "HEAD"])?;
    let head_commit_time = git_in(&dir, &["show", "-s", "--format=%cI", "HEAD"]);
    Some(serde_json::json!({
        "headSha": head_sha,
        "headCommitTime": head_commit_time,
    }))
}

/// TTL-cached because the dashboard badge polls `/api/version` on a timer, and
/// an uncached probe spawns two `git` processes per poll.
pub fn repo_head() -> Option<serde_json::Value> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Option<serde_json::Value>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().ok()?;
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < REPO_TTL {
            return value.clone();
        }
    }
    let fresh = probe_repo_head();
    *guard = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Commits present in `HEAD` but absent from `sha`.
///
/// `sha` arrives from the `?since=` query parameter and is passed to `git` as
/// an argument, so it is hex-validated first: a leading `-` would otherwise turn
/// the ref into a flag and let a caller inject `git` arguments.
pub fn commits_since(sha: &str) -> Option<u64> {
    if !is_hex_sha(sha) {
        return None;
    }
    let dir = repo_root()?;
    let range = format!("{sha}..HEAD");
    let count = git_in(&dir, &["rev-list", "--count", &range])?;
    count.parse().ok()
}

/// Hex-only, which is also what excludes `-` and argument injection.
fn is_hex_sha(sha: &str) -> bool {
    (4..=40).contains(&sha.len()) && sha.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_sha_accepts_short_and_full_object_names() {
        assert!(is_hex_sha("1cd7"));
        assert!(is_hex_sha("1cd7344"));
        assert!(is_hex_sha("1cd73449c752066b97cca208b66661ce24b25c17"));
        assert!(is_hex_sha("ABCDEF12"));
    }

    #[test]
    fn hex_sha_rejects_anything_that_could_inject_a_git_argument() {
        // A leading `-` reinterprets the ref as a flag, which is the whole
        // reason this validator exists.
        assert!(!is_hex_sha("--upload-pack=touch /tmp/pwn"));
        assert!(!is_hex_sha("-rf"));
        assert!(!is_hex_sha("HEAD..main"));
        assert!(!is_hex_sha("abc;rm -rf /"));
        assert!(!is_hex_sha("$(id)"));
        assert!(!is_hex_sha(""));
        assert!(!is_hex_sha("abc"));
        assert!(!is_hex_sha(&"a".repeat(41)));
    }

    #[test]
    fn commits_since_ignores_untrusted_input() {
        assert_eq!(commits_since("--upload-pack=touch /tmp/pwn"), None);
        assert_eq!(commits_since("not-a-sha"), None);
    }

    #[test]
    fn commits_since_is_none_for_an_unknown_object() {
        assert_eq!(commits_since("deadbeef"), None);
    }

    #[test]
    fn identity_accessors_never_report_placeholders_as_real_values() {
        if let Some(sha) = identity_sha() {
            assert!(!sha.is_empty());
            assert_ne!(sha, "unknown");
            assert!(is_hex_sha(&sha), "baked sha must be hex: {sha}");
        }
        if let Some(time) = identity_build_time() {
            assert_ne!(time, "unknown");
            assert!(time.contains('-'), "expected RFC3339, got {time}");
        }
    }

    #[test]
    fn repo_head_is_well_formed_when_a_checkout_is_reachable() {
        let Some(head) = repo_head() else {
            return;
        };
        let sha = head["headSha"].as_str().expect("headSha must be a string");
        assert!(!sha.is_empty());
        assert!(is_hex_sha(sha), "headSha must be hex: {sha}");
    }
}
