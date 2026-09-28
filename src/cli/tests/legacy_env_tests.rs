//! Tests for `Cli::apply_legacy_env`, the reconciliation clap cannot do for itself.
//!
//! clap's `env = "..."` accepts one variable name and has no fallback, so the five
//! flags that used to read `CIPHERROUTE_*` are pointed at `ZEROPROXY_*` and the legacy
//! spelling is applied to the parsed struct after `Cli::parse()`.
//!
//! `crate::core::env`'s own tests cover the lookup helper; these cover the *clap* side,
//! where the rule differs in a way the helper's tests cannot express: the legacy value
//! is consulted only when the new variable is entirely absent from the environment, not
//! merely when the parsed field looks empty.

use crate::cli::test_lock::ENV_LOCK;
use crate::cli::Cli;
use clap::Parser;

/// Sets `vars`, runs `body`, then restores every touched variable.
///
/// `vars` is `(new_name, value, legacy_value)`; `None` for a value means "ensure
/// absent", so an ambient variable in the developer's shell cannot satisfy a test.
fn with_legacy_env<T>(vars: &[(&str, Option<&str>, Option<&str>)], body: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut prior = Vec::new();
    for (key, new_value, legacy_value) in vars {
        let new_key: &str = key;
        let legacy_key = crate::core::env::legacy_name(new_key).unwrap();
        for (k, v) in [(new_key, *new_value), (legacy_key.as_str(), *legacy_value)] {
            prior.push((k.to_string(), std::env::var_os(k)));
            match v {
                Some(value) => std::env::set_var(k, value),
                None => std::env::remove_var(k),
            }
        }
    }

    let result = body();

    for (key, value) in prior {
        match value {
            Some(value) => std::env::set_var(&key, value),
            None => std::env::remove_var(&key),
        }
    }
    result
}

fn parse(args: &[&str]) -> Cli {
    let mut cli = Cli::parse_from(args);
    cli.apply_legacy_env();
    cli
}

#[test]
fn new_env_names_are_parsed_by_clap_itself() {
    with_legacy_env(
        &[
            ("ZEROPROXY_PROFILE", Some("new-profile"), None),
            ("ZEROPROXY_URL", Some("http://new"), None),
            ("ZEROPROXY_API_KEY", Some("new-key"), None),
            ("ZEROPROXY_WEB_DIR", Some("/new/web"), None),
        ],
        || {
            let cli = parse(&["zeroproxy"]);
            assert_eq!(cli.profile.as_deref(), Some("new-profile"));
            assert_eq!(cli.url.as_deref(), Some("http://new"));
            assert_eq!(cli.api_key.as_deref(), Some("new-key"));
            assert_eq!(
                cli.web_dir.as_deref(),
                Some(std::path::Path::new("/new/web"))
            );
            assert!(!cli.no_open);
        },
    );
}

#[test]
fn legacy_env_names_are_applied_to_the_parsed_flags() {
    // The regression this reconciliation exists for: a user who still exports the old
    // spelling must not silently lose it to a rename.
    with_legacy_env(
        &[
            ("ZEROPROXY_PROFILE", None, Some("legacy-profile")),
            ("ZEROPROXY_URL", None, Some("http://legacy")),
            ("ZEROPROXY_API_KEY", None, Some("legacy-key")),
            ("ZEROPROXY_WEB_DIR", None, Some("/legacy/web")),
        ],
        || {
            let cli = parse(&["zeroproxy"]);
            assert_eq!(cli.profile.as_deref(), Some("legacy-profile"));
            assert_eq!(cli.url.as_deref(), Some("http://legacy"));
            assert_eq!(cli.api_key.as_deref(), Some("legacy-key"));
            assert_eq!(
                cli.web_dir.as_deref(),
                Some(std::path::Path::new("/legacy/web"))
            );
        },
    );
}

#[test]
fn new_env_names_win_over_legacy_ones() {
    with_legacy_env(
        &[
            (
                "ZEROPROXY_PROFILE",
                Some("new-profile"),
                Some("legacy-profile"),
            ),
            ("ZEROPROXY_URL", Some("http://new"), Some("http://legacy")),
            ("ZEROPROXY_API_KEY", Some("new-key"), Some("legacy-key")),
            ("ZEROPROXY_WEB_DIR", Some("/new/web"), Some("/legacy/web")),
        ],
        || {
            let cli = parse(&["zeroproxy"]);
            assert_eq!(cli.profile.as_deref(), Some("new-profile"));
            assert_eq!(cli.url.as_deref(), Some("http://new"));
            assert_eq!(cli.api_key.as_deref(), Some("new-key"));
            assert_eq!(
                cli.web_dir.as_deref(),
                Some(std::path::Path::new("/new/web"))
            );
        },
    );
}

#[test]
fn an_explicitly_false_new_no_open_is_not_overridden_by_a_legacy_true() {
    // The subtle case that rules out "check whether the parsed field is empty". clap has
    // already set `no_open = false` from the new variable here, so a field-based check
    // would apply the legacy value and invert the documented precedence.
    with_legacy_env(
        &[("ZEROPROXY_NO_OPEN", Some("false"), Some("true"))],
        || {
            let cli = parse(&["zeroproxy"]);
            assert!(
                !cli.no_open,
                "ZEROPROXY_NO_OPEN=false must win over CIPHERROUTE_NO_OPEN=true"
            );
        },
    );
}

#[test]
fn a_legacy_no_open_true_is_applied_when_the_new_name_is_absent() {
    with_legacy_env(&[("ZEROPROXY_NO_OPEN", None, Some("true"))], || {
        let cli = parse(&["zeroproxy"]);
        assert!(cli.no_open);
    });
}

#[test]
fn a_legacy_no_open_false_does_not_enable_the_flag() {
    with_legacy_env(&[("ZEROPROXY_NO_OPEN", None, Some("false"))], || {
        let cli = parse(&["zeroproxy"]);
        assert!(!cli.no_open);
    });
}

#[test]
fn an_empty_legacy_no_open_still_enables_the_flag() {
    // Mirrors clap's own flag-from-env rule, so the fallback is not stricter than the
    // behaviour it stands in for.
    with_legacy_env(&[("ZEROPROXY_NO_OPEN", None, Some(""))], || {
        let cli = parse(&["zeroproxy"]);
        assert!(cli.no_open);
    });
}

#[test]
fn command_line_arguments_win_over_both_env_spellings() {
    with_legacy_env(
        &[
            (
                "ZEROPROXY_PROFILE",
                Some("new-profile"),
                Some("legacy-profile"),
            ),
            ("ZEROPROXY_URL", Some("http://new"), Some("http://legacy")),
            ("ZEROPROXY_API_KEY", Some("new-key"), Some("legacy-key")),
        ],
        || {
            let cli = parse(&[
                "zeroproxy",
                "--profile",
                "flag-profile",
                "--url",
                "http://flag",
                "--api-key",
                "flag-key",
            ]);
            assert_eq!(cli.profile.as_deref(), Some("flag-profile"));
            assert_eq!(cli.url.as_deref(), Some("http://flag"));
            assert_eq!(cli.api_key.as_deref(), Some("flag-key"));
        },
    );
}
