//! Environment-variable access with a legacy-name fallback.
//!
//! The crate was renamed from `cipherroute` to `zeroproxy` in `bf807c8a`
//! (2026-08-29), but the environment variables were not renamed at the time — they
//! stayed `CIPHERROUTE_*` while the binary, the data dir and the schema namespace all
//! became `zeroproxy`. That half-finished rename is confusing on its own, and it is a
//! silent one: a user who reads `CIPHERROUTE_API_KEY` out of a doc comment and sets it
//! gets a proxy that appears to start and then rejects every request.
//!
//! So the variables are now spelled `ZEROPROXY_*`, and this module keeps the old
//! spelling working. Every lookup goes through [`var`], which reads the new name first
//! and only then falls back to the legacy one:
//!
//! ```ignore
//! // Reads ZEROPROXY_URL, else CIPHERROUTE_URL, else NotPresent.
//! let url = env::var("ZEROPROXY_URL").unwrap_or_default();
//! ```
//!
//! Two rules that this module exists to enforce:
//!
//! 1. **The new name wins.** If both are set, `ZEROPROXY_*` is used. That makes the
//!    rename a real change rather than a no-op, and it gives a user a way to override
//!    an inherited legacy value.
//! 2. **Only the brand token is swapped.** [`legacy_name`] rewrites the `ZEROPROXY_`
//!    substring wherever it appears, not just at the start, so a prefixed name such as
//!    `JCODE_ZEROPROXY_API_KEY` correctly falls back to `JCODE_CIPHERROUTE_API_KEY`.
//!
//! Do not call [`std::env::var`] directly for a ZeroProxy variable — that is what
//! produced two spellings of the same setting in the first place. Note the contrast
//! with [`std::env::set_var`] and [`std::env::remove_var`], which are *not* wrapped:
//! those are only ever used to isolate a test, and a test must name one spelling
//! explicitly so that it is obvious which path it exercises.

use std::env::VarError;

/// The brand token used by every ZeroProxy environment variable.
pub const BRAND: &str = "ZEROPROXY_";

/// The pre-rename brand token, still honoured for backward compatibility.
pub const LEGACY_BRAND: &str = "CIPHERROUTE_";

/// The legacy spelling of `key`, or `None` if it has no brand token to rewrite.
///
/// The rewrite is a plain substring substitution rather than a prefix check, so
/// `JCODE_ZEROPROXY_API_KEY` maps to `JCODE_CIPHERROUTE_API_KEY` just as
/// `ZEROPROXY_URL` maps to `CIPHERROUTE_URL`.
///
/// ```
/// assert_eq!(crate::core::env::legacy_name("ZEROPROXY_URL").as_deref(), Some("CIPHERROUTE_URL"));
/// assert_eq!(
///     crate::core::env::legacy_name("JCODE_ZEROPROXY_API_KEY").as_deref(),
///     Some("JCODE_CIPHERROUTE_API_KEY")
/// );
/// assert_eq!(crate::core::env::legacy_name("OPENAI_API_KEY"), None);
/// ```
pub fn legacy_name(key: &str) -> Option<String> {
    key.contains(BRAND)
        .then(|| key.replace(BRAND, LEGACY_BRAND))
}

/// Read `key` from the environment, falling back to its legacy spelling.
///
/// The signature matches [`std::env::var`] deliberately: it returns
/// [`std::env::VarError`], so every existing `.ok()`, `.unwrap_or_default()` and
/// `.and_then(|v| v.parse().ok())` call site keeps working unchanged and this is a
/// one-token edit at each of them. A wrapper returning `Option<String>` instead would
/// have forced a rewrite of all 35 sites for no behavioural gain.
///
/// The new name is consulted first, so it takes precedence when both are set. A `key`
/// with no brand token is read plainly, with no fallback — which makes this safe to
/// route third-party lookups through as well.
///
/// ```
/// // Reads ZEROPROXY_URL, then CIPHERROUTE_URL.
/// assert_eq!(
///     crate::core::env::var("ZEROPROXY_URL").is_ok(),
///     std::env::var("ZEROPROXY_URL").is_ok() || std::env::var("CIPHERROUTE_URL").is_ok()
/// );
/// ```
pub fn var(key: &str) -> Result<String, VarError> {
    match std::env::var(key) {
        Ok(value) => Ok(value),
        // `VarError` has a single variant, so the original error carries no
        // information that the fallback attempt does not also produce.
        Err(_) => match legacy_name(key) {
            Some(legacy) => std::env::var(legacy),
            None => Err(VarError::NotPresent),
        },
    }
}

/// Read `key` from the environment as an [`OsString`](std::ffi::OsString), falling back
/// to its legacy spelling.
///
/// For `ZEROPROXY_DATA_DIR` only. Do NOT route it through [`var`]: a non-UTF-8 path returns
/// `VarError::NotUnicode`, which [`var`] treats as unset and answers from the legacy name —
/// so the new value would be silently ignored on the strength of an encoding assumption.
pub fn var_os(key: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(key).or_else(|| legacy_name(key).and_then(|legacy| std::env::var_os(&legacy)))
}

#[cfg(test)]
mod tests {
    use super::{legacy_name, var, var_os, BRAND, LEGACY_BRAND};

    /// Serialises the tests that mutate the process environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_env<T>(
        new_name: Option<&str>,
        legacy_name: Option<&str>,
        body: impl FnOnce() -> T,
    ) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let new_key = format!("{BRAND}ENV_RENAME_TEST");
        let legacy_key = format!("{LEGACY_BRAND}ENV_RENAME_TEST");

        // Record and clear any pre-existing values so the assertions below cannot be
        // satisfied by an ambient variable from the developer's shell.
        let prior_new = std::env::var_os(&new_key);
        let prior_legacy = std::env::var_os(&legacy_key);
        std::env::remove_var(&new_key);
        std::env::remove_var(&legacy_key);

        match new_name {
            Some(value) => std::env::set_var(&new_key, value),
            None => std::env::remove_var(&new_key),
        }
        match legacy_name {
            Some(value) => std::env::set_var(&legacy_key, value),
            None => std::env::remove_var(&legacy_key),
        }

        let result = body();

        match prior_new {
            Some(value) => std::env::set_var(&new_key, value),
            None => std::env::remove_var(&new_key),
        }
        match prior_legacy {
            Some(value) => std::env::set_var(&legacy_key, value),
            None => std::env::remove_var(&legacy_key),
        }
        result
    }

    #[test]
    fn legacy_name_rewrites_the_brand_token() {
        assert_eq!(
            legacy_name("ZEROPROXY_URL").as_deref(),
            Some("CIPHERROUTE_URL")
        );
    }

    #[test]
    fn legacy_name_rewrites_a_mid_string_brand_token() {
        // The whole reason this is a substring swap and not a prefix strip: a prefixed
        // variable must still fall back correctly.
        assert_eq!(
            legacy_name("JCODE_ZEROPROXY_API_KEY").as_deref(),
            Some("JCODE_CIPHERROUTE_API_KEY")
        );
    }

    #[test]
    fn legacy_name_is_none_without_a_brand_token() {
        assert_eq!(legacy_name("OPENAI_API_KEY"), None);
        assert_eq!(legacy_name("PATH"), None);
    }

    #[test]
    fn var_reads_the_new_name() {
        with_env(Some("new-value"), None, || {
            assert_eq!(var("ZEROPROXY_ENV_RENAME_TEST").as_deref(), Ok("new-value"));
        });
    }

    #[test]
    fn var_falls_back_to_the_legacy_name() {
        // The regression this module exists for: a user who still has the old spelling
        // in their shell must not silently lose it.
        with_env(None, Some("legacy-value"), || {
            assert_eq!(
                var("ZEROPROXY_ENV_RENAME_TEST").as_deref(),
                Ok("legacy-value")
            );
        });
    }

    #[test]
    fn new_name_takes_precedence_over_the_legacy_name() {
        with_env(Some("new-value"), Some("legacy-value"), || {
            assert_eq!(var("ZEROPROXY_ENV_RENAME_TEST").as_deref(), Ok("new-value"));
        });
    }

    #[test]
    fn var_reports_not_present_when_neither_is_set() {
        with_env(None, None, || {
            assert_eq!(
                var("ZEROPROXY_ENV_RENAME_TEST"),
                Err(std::env::VarError::NotPresent)
            );
        });
    }

    #[test]
    fn var_reads_a_brandedess_name_without_a_fallback() {
        // A key with no brand token must behave exactly like std::env::var, including
        // for names that happen to look similar.
        with_env(None, None, || {
            std::env::set_var("ENV_RENAME_TEST_UNBRANDED", "plain");
            let result = var("ENV_RENAME_TEST_UNBRANDED");
            std::env::remove_var("ENV_RENAME_TEST_UNBRANDED");
            assert_eq!(result.as_deref(), Ok("plain"));
        });
    }

    #[test]
    fn var_os_falls_back_to_the_legacy_name() {
        with_env(None, Some("/legacy/dir"), || {
            assert_eq!(
                var_os("ZEROPROXY_ENV_RENAME_TEST"),
                Some(std::ffi::OsString::from("/legacy/dir"))
            );
        });
    }

    #[test]
    fn var_os_prefers_the_new_name() {
        with_env(Some("/new/dir"), Some("/legacy/dir"), || {
            assert_eq!(
                var_os("ZEROPROXY_ENV_RENAME_TEST"),
                Some(std::ffi::OsString::from("/new/dir"))
            );
        });
    }

    #[test]
    fn var_os_is_none_without_a_brand_token_or_value() {
        with_env(None, None, || {
            assert_eq!(var_os("ENV_RENAME_TEST_UNBRANDED_OS"), None);
        });
    }
}
