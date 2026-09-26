//! Port of `open-sse/config/errorConfig.js` — error type/message tables,
//! backoff configuration, and the unified rule list used to decide cooldown
//! durations from upstream errors.

use std::time::Duration;

/// Cooldown duration constants (ms-equivalent) used by [`ERROR_RULES`] and
/// the `COOLDOWN_MS` backwards-compat record.
mod cooldown_consts {
    pub const LONG_MS: u64 = 2 * 60 * 1000;
    pub const SHORT_MS: u64 = 5 * 1000;
    pub const TRANSIENT_MS: u64 = 30 * 1000;
}

/// Long cooldown for credential / auth-related errors that need user action.
pub const COOLDOWN_LONG_MS: u64 = cooldown_consts::LONG_MS;
/// Short cooldown for "request not allowed" style soft-rejection errors.
pub const COOLDOWN_SHORT_MS: u64 = cooldown_consts::SHORT_MS;
/// Default cooldown for transient/unknown errors.
pub const TRANSIENT_COOLDOWN_MS: u64 = cooldown_consts::TRANSIENT_MS;

/// Hard cap for provider-reported rate-limit cooldowns (some providers
/// like Codex announce `resets_at` 5–6 hours out, which we never honour
/// directly — clamp to 30 minutes).
pub const MAX_RATE_LIMIT_COOLDOWN_MS: u64 = 30 * 60 * 1000;

/// Exponential backoff parameters for rate-limit retries.
#[derive(Debug, Clone, Copy)]
pub struct BackoffConfig {
    pub base_ms: u64,
    pub max_ms: u64,
    pub max_level: u32,
}

pub const BACKOFF_CONFIG: BackoffConfig = BackoffConfig {
    base_ms: 2000,
    max_ms: 5 * 60 * 1000,
    max_level: 15,
};

/// OpenAI-compatible error type/code descriptor surfaced to clients.
#[derive(Debug, Clone, Copy)]
pub struct ErrorTypeInfo {
    pub r#type: &'static str,
    pub code: &'static str,
}

/// Translate an HTTP status code to an OpenAI-compatible error type/code.
pub const fn error_type_for(status: u16) -> Option<ErrorTypeInfo> {
    Some(match status {
        400 => ErrorTypeInfo {
            r#type: "invalid_request_error",
            code: "bad_request",
        },
        401 => ErrorTypeInfo {
            r#type: "authentication_error",
            code: "invalid_api_key",
        },
        402 => ErrorTypeInfo {
            r#type: "billing_error",
            code: "payment_required",
        },
        403 => ErrorTypeInfo {
            r#type: "permission_error",
            code: "insufficient_quota",
        },
        404 => ErrorTypeInfo {
            r#type: "invalid_request_error",
            code: "model_not_found",
        },
        406 => ErrorTypeInfo {
            r#type: "invalid_request_error",
            code: "model_not_supported",
        },
        429 => ErrorTypeInfo {
            r#type: "rate_limit_error",
            code: "rate_limit_exceeded",
        },
        500 => ErrorTypeInfo {
            r#type: "server_error",
            code: "internal_server_error",
        },
        502 => ErrorTypeInfo {
            r#type: "server_error",
            code: "bad_gateway",
        },
        503 => ErrorTypeInfo {
            r#type: "server_error",
            code: "service_unavailable",
        },
        504 => ErrorTypeInfo {
            r#type: "server_error",
            code: "gateway_timeout",
        },
        529 => ErrorTypeInfo {
            r#type: "server_error",
            code: "service_overloaded",
        },
        _ => return None,
    })
}

/// Default client-facing error message for each known status code.
pub const fn default_error_message(status: u16) -> Option<&'static str> {
    Some(match status {
        400 => "Bad request",
        401 => "Invalid API key provided",
        402 => "Payment required",
        403 => "You exceeded your current quota",
        404 => "Model not found",
        406 => "Model not supported",
        429 => "Rate limit exceeded",
        500 => "Internal server error",
        502 => "Bad gateway - upstream provider error",
        503 => "Service temporarily unavailable",
        504 => "Gateway timeout",
        529 => "Service overloaded",
        _ => return None,
    })
}

/// One classification rule. Either matches by substring on the error
/// message (`text`) or by HTTP status (`status`). When matched, `cooldown`
/// gives the suggested cooldown duration; if `backoff` is true the rate-
/// limit exponential backoff is used instead.
#[derive(Debug, Clone, Copy)]
pub struct ErrorRule {
    pub text: Option<&'static str>,
    pub status: Option<u16>,
    pub cooldown: Option<Duration>,
    pub backoff: bool,
}

/// Unified error classification table, checked top-to-bottom. Text rules
/// fire before status rules.
pub const ERROR_RULES: &[ErrorRule] = &[
    // Text-based rules (case-insensitive substring match).
    ErrorRule {
        text: Some("no credentials"),
        status: None,
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: Some("request not allowed"),
        status: None,
        cooldown: Some(Duration::from_millis(cooldown_consts::SHORT_MS)),
        backoff: false,
    },
    ErrorRule {
        text: Some("improperly formed request"),
        status: None,
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: Some("rate limit"),
        status: None,
        cooldown: None,
        backoff: true,
    },
    ErrorRule {
        text: Some("too many requests"),
        status: None,
        cooldown: None,
        backoff: true,
    },
    ErrorRule {
        text: Some("quota exceeded"),
        status: None,
        cooldown: None,
        backoff: true,
    },
    ErrorRule {
        text: Some("capacity"),
        status: None,
        cooldown: None,
        backoff: true,
    },
    ErrorRule {
        text: Some("overloaded"),
        status: None,
        cooldown: None,
        backoff: true,
    },
    // 401 with model-unavailable message → model lock (not OAuth refresh).
    // Matches phrases like "not supported", "not entitled" that signal
    // the model is permanently unavailable for this account.
    ErrorRule {
        text: Some("not supported"),
        status: Some(401),
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: Some("not entitled"),
        status: Some(401),
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    // Status-based fallbacks.
    ErrorRule {
        text: None,
        status: Some(401),
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: None,
        status: Some(403),
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: None,
        status: Some(404),
        cooldown: Some(Duration::from_millis(cooldown_consts::LONG_MS)),
        backoff: false,
    },
    ErrorRule {
        text: None,
        status: Some(529),
        cooldown: Some(Duration::from_millis(cooldown_consts::SHORT_MS)),
        backoff: true,
    },
    ErrorRule {
        text: None,
        status: Some(429),
        cooldown: None,
        backoff: true,
    },
];

/// Outcome of classifying a single upstream error against [`ERROR_RULES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClassification {
    /// Use exponential backoff (rate-limit path).
    Backoff,
    /// Apply the given fixed cooldown duration.
    Cooldown(Duration),
    /// No rule matched; caller should apply their own default.
    NoMatch,
    /// Permanent error (400, 401, 403) — do not fall back to next combo member.
    Permanent,
}

/// Run an upstream error through [`ERROR_RULES`] and return the matching
/// classification. Text rules fire first; status rules are the fallback.
///
/// After all rules are checked, permanent HTTP status codes (400, 401, 403)
/// that did *not* match any earlier rule are classified as [`Permanent`] so
/// the caller does not burn through combo members on client errors.
pub fn classify_error(message: Option<&str>, status: Option<u16>) -> ErrorClassification {
    let lowered = message.map(|m| m.to_lowercase());
    // Provider-originated rejections ("upstream returned 400 …") are not
    // client errors on *our* surface: the next combo/auto member may accept
    // a differently translated body (e.g. Gemini rejecting a stream shape
    // OpenAI-compatible providers accept). Never classify them Permanent, and
    // never let a bare status rule cool the account over one — a mistranslated
    // body would otherwise take a healthy provider out of rotation. Text rules
    // still win, so a genuine "upstream returned 429 … rate limit" backs off.
    let upstream_rejection = lowered
        .as_deref()
        .map(|m| m.contains("upstream returned"))
        .unwrap_or(false);
    for rule in ERROR_RULES {
        if upstream_rejection && rule.text.is_none() {
            continue;
        }
        let matched = match (rule.text, rule.status) {
            (Some(needle), _) => lowered
                .as_deref()
                .map(|m| m.contains(needle))
                .unwrap_or(false),
            (None, Some(want)) => status == Some(want),
            (None, None) => false,
        };
        if !matched {
            continue;
        }
        if rule.backoff {
            return ErrorClassification::Backoff;
        }
        if let Some(d) = rule.cooldown {
            return ErrorClassification::Cooldown(d);
        }
    }
    if upstream_rejection {
        return ErrorClassification::NoMatch;
    }
    // Permanent errors (400, 401, 403) that matched no rule at all
    // should not trigger fallback — the error is client-side, not
    // a transient provider issue.
    if matches!(status, Some(400) | Some(401) | Some(402) | Some(403)) {
        return ErrorClassification::Permanent;
    }
    ErrorClassification::NoMatch
}

/// Retry policy applied at combo level. Combo retries here mean
/// "fall back to the next member and try again", not "retry the same
/// upstream call". Per-provider retries inside executors are governed
/// separately by each executor's retry loop.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of additional members to try before giving up.
    pub max_attempts: u32,
    /// Initial wait before the first retry.
    pub base_backoff_ms: u64,
    /// Cap on exponential growth of the backoff.
    pub max_backoff_ms: u64,
    /// Random jitter factor [0.0, 1.0] applied multiplicatively to the
    /// computed backoff. 0.0 = no jitter (deterministic); 1.0 = up to
    /// 100% extra wait.
    pub jitter_factor: f64,
    /// Which classifications should trigger a retry. Default =
    /// `[Backoff, Cooldown, NoMatch]`. `Permanent` is never retried.
    pub retry_on: Vec<ErrorClassification>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_backoff_ms: 1000,
            max_backoff_ms: 30_000,
            jitter_factor: 0.25,
            retry_on: vec![
                ErrorClassification::Backoff,
                ErrorClassification::Cooldown(Duration::from_secs(0)),
                ErrorClassification::NoMatch,
            ],
        }
    }
}

impl RetryPolicy {
    /// Decide whether to retry the next combo member given the current
    /// classification and attempt count (0-indexed). Returns `None` when
    /// the caller should stop retrying.
    pub fn should_retry(
        &self,
        classification: ErrorClassification,
        attempt: u32,
    ) -> Option<Duration> {
        // Permanent errors never retry — they're caller bugs, not transient.
        if matches!(classification, ErrorClassification::Permanent) {
            return None;
        }
        if !self.retry_on.contains(&classification) {
            return None;
        }
        if attempt >= self.max_attempts {
            return None;
        }
        let backoff = self
            .base_backoff_ms
            .saturating_mul(2u64.saturating_pow(attempt))
            .min(self.max_backoff_ms);
        let jitter_ms = if self.jitter_factor > 0.0 {
            (backoff as f64 * self.jitter_factor * rand_jitter()) as u64
        } else {
            0
        };
        Some(Duration::from_millis(backoff + jitter_ms))
    }
}

/// Deterministic-ish jitter in [0.0, 1.0). Uses a thread-local PRNG so
/// test runs stay reproducible (no external dependency on `rand`).
fn rand_jitter() -> f64 {
    use std::cell::Cell;
    use std::time::SystemTime;
    thread_local! {
        static STATE: Cell<u64> = Cell::new({
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0xdead_beef)
                ^ 0x9e37_79b9_7f4a_7c15
        });
    }
    STATE.with(|s| {
        // SplitMix64 step
        let mut x = s.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let y = x ^ (x >> 31);
        s.set(y);
        // Top 24 bits → [0.0, 1.0)
        (y >> 40) as f64 / (1u64 << 24) as f64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_type_round_trip() {
        let info = error_type_for(429).unwrap();
        assert_eq!(info.r#type, "rate_limit_error");
        assert_eq!(info.code, "rate_limit_exceeded");
        assert!(error_type_for(999).is_none());
    }

    #[test]
    fn classify_picks_text_rule_first() {
        // Status 500 wouldn't match any rule, but the text "rate limit" wins.
        assert_eq!(
            classify_error(Some("Rate limit exceeded"), Some(500)),
            ErrorClassification::Backoff
        );
    }

    #[test]
    fn classify_falls_back_to_status() {
        assert_eq!(
            classify_error(Some("auth failed"), Some(401)),
            ErrorClassification::Cooldown(Duration::from_millis(COOLDOWN_LONG_MS))
        );
    }

    #[test]
    fn classify_no_match_returns_unmatched() {
        assert_eq!(
            classify_error(Some("teapot"), Some(418)),
            ErrorClassification::NoMatch
        );
    }

    #[test]
    fn classify_400_is_permanent() {
        assert_eq!(
            classify_error(Some("weird payload"), Some(400)),
            ErrorClassification::Permanent
        );
    }

    #[test]
    fn classify_upstream_400_is_fallback_eligible() {
        // Provider rejected the translated body — next combo member must run.
        let msg =
            "upstream returned 400 for URL https://example/v1/stream: Unknown name \"stream\"";
        assert_eq!(
            classify_error(Some(msg), Some(400)),
            ErrorClassification::NoMatch
        );
        assert_eq!(
            classify_error(Some("upstream returned 401 for URL https://x"), Some(401)),
            ErrorClassification::NoMatch
        );
    }

    #[test]
    fn classify_429_is_backoff() {
        assert_eq!(
            classify_error(Some("irrelevant text"), Some(429)),
            ErrorClassification::Backoff
        );
    }

    #[test]
    fn classify_no_message_uses_status_only() {
        assert_eq!(
            classify_error(None, Some(404)),
            ErrorClassification::Cooldown(Duration::from_millis(COOLDOWN_LONG_MS))
        );
    }

    #[test]
    fn classify_case_insensitive_text() {
        assert_eq!(
            classify_error(Some("RATE LIMIT"), Some(500)),
            ErrorClassification::Backoff
        );
    }

    // ---- RetryPolicy ----

    #[test]
    fn retry_policy_default_retries_three_classes() {
        let p = RetryPolicy::default();
        assert!(p.should_retry(ErrorClassification::Backoff, 0).is_some());
        assert!(p.should_retry(ErrorClassification::NoMatch, 0).is_some());
        assert!(p.should_retry(ErrorClassification::Permanent, 0).is_none());
    }

    #[test]
    fn retry_policy_respects_max_attempts() {
        let p = RetryPolicy {
            max_attempts: 2,
            ..RetryPolicy::default()
        };
        assert!(p.should_retry(ErrorClassification::Backoff, 0).is_some());
        assert!(p.should_retry(ErrorClassification::Backoff, 1).is_some());
        assert!(p.should_retry(ErrorClassification::Backoff, 2).is_none());
    }

    #[test]
    fn retry_policy_exponential_backoff_caps_at_max() {
        let p = RetryPolicy {
            max_attempts: 10,
            base_backoff_ms: 1000,
            max_backoff_ms: 5000,
            jitter_factor: 0.0,
            ..RetryPolicy::default()
        };
        let d0 = p.should_retry(ErrorClassification::Backoff, 0).unwrap();
        let d1 = p.should_retry(ErrorClassification::Backoff, 1).unwrap();
        let d5 = p.should_retry(ErrorClassification::Backoff, 5).unwrap();
        assert_eq!(d0, Duration::from_millis(1000));
        assert_eq!(d1, Duration::from_millis(2000));
        assert_eq!(d5, Duration::from_millis(5000));
    }

    #[test]
    fn retry_policy_jitter_bounded_by_factor() {
        let p = RetryPolicy {
            base_backoff_ms: 1000,
            max_backoff_ms: 1000,
            jitter_factor: 0.5,
            max_attempts: 1,
            ..RetryPolicy::default()
        };
        for _ in 0..20 {
            let d = p.should_retry(ErrorClassification::Backoff, 0).unwrap();
            // base=1000, max=1000, jitter up to 50% → [1000, 1500]
            assert!(
                d >= Duration::from_millis(1000) && d <= Duration::from_millis(1500),
                "out of range: {:?}",
                d
            );
        }
    }

    #[test]
    fn retry_policy_classification_gate() {
        let p = RetryPolicy {
            retry_on: vec![ErrorClassification::Backoff],
            ..RetryPolicy::default()
        };
        assert!(p.should_retry(ErrorClassification::Backoff, 0).is_some());
        assert!(p.should_retry(ErrorClassification::NoMatch, 0).is_none());
    }
}
