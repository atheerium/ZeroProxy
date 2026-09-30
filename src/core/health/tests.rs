use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::json;

use super::daemon::{clear_sticky_error, has_sticky_error, needs_persist};
use super::probe::probe_target;
use super::{
    HealthRegistry, HealthStatus, DEGRADED_UNTIL_KEY, DEGRADE_RATE_LIMITED, DEGRADE_SERVER_ERROR,
    DEGRADE_UNAVAILABLE, HEALTH_STATUS_KEY,
};
use crate::types::ProviderConnection;

fn fixed_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
        .single()
        .expect("valid fixed timestamp")
}

fn connection(id: &str, provider: &str) -> ProviderConnection {
    ProviderConnection {
        id: id.to_string(),
        provider: provider.to_string(),
        auth_type: "apikey".to_string(),
        api_key: Some("sk-test".to_string()),
        ..Default::default()
    }
}

#[test]
fn from_http_classifies_documented_statuses() {
    assert_eq!(HealthStatus::from_http(200), HealthStatus::Healthy);
    assert_eq!(HealthStatus::from_http(204), HealthStatus::Healthy);
    assert_eq!(HealthStatus::from_http(401), HealthStatus::AuthFailed);
    assert_eq!(HealthStatus::from_http(403), HealthStatus::AuthFailed);
    assert_eq!(HealthStatus::from_http(429), HealthStatus::RateLimited);
    assert_eq!(HealthStatus::from_http(503), HealthStatus::Unavailable);
    assert_eq!(HealthStatus::from_http(500), HealthStatus::ServerError);
    assert_eq!(HealthStatus::from_http(502), HealthStatus::ServerError);
    assert_eq!(HealthStatus::from_http(504), HealthStatus::ServerError);
    assert_eq!(HealthStatus::from_http(404), HealthStatus::Unknown);
    assert_eq!(HealthStatus::from_http(400), HealthStatus::Unknown);
    assert_eq!(
        HealthStatus::from_transport_failure(),
        HealthStatus::ServerError
    );
}

#[test]
fn degrade_windows_match_spec_table() {
    assert_eq!(HealthStatus::Healthy.degrade_duration(), None);
    assert_eq!(HealthStatus::AuthFailed.degrade_duration(), None);
    assert_eq!(HealthStatus::Unknown.degrade_duration(), None);
    assert_eq!(
        HealthStatus::RateLimited.degrade_duration(),
        Some(DEGRADE_RATE_LIMITED)
    );
    assert_eq!(
        HealthStatus::Unavailable.degrade_duration(),
        Some(DEGRADE_UNAVAILABLE)
    );
    assert_eq!(
        HealthStatus::ServerError.degrade_duration(),
        Some(DEGRADE_SERVER_ERROR)
    );
    assert_eq!(DEGRADE_RATE_LIMITED.as_secs(), 120);
    assert_eq!(DEGRADE_UNAVAILABLE.as_secs(), 600);
    assert_eq!(DEGRADE_SERVER_ERROR.as_secs(), 300);
}

#[test]
fn record_probe_sets_exact_degrade_deadline_per_status() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    for (status, expected_secs) in [
        (429u16, 120i64),
        (503, 600),
        (500, 300),
        (502, 300),
        (504, 300),
    ] {
        let record = registry.record_probe_at("conn", "openai", Some(status), None, now);
        assert_eq!(
            record.degraded_until,
            Some(now + ChronoDuration::seconds(expected_secs)),
            "status {status} must degrade for {expected_secs}s"
        );
    }
}

#[test]
fn healthy_probe_clears_degrade_and_failure_count() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    registry.record_probe_at("conn", "openai", Some(503), None, now);
    assert!(registry.is_connection_degraded_at("conn", now));

    let record = registry.record_probe_at("conn", "openai", Some(200), None, now);
    assert_eq!(record.status, HealthStatus::Healthy);
    assert_eq!(record.degraded_until, None);
    assert_eq!(record.consecutive_failures, 0);
    assert!(!registry.is_connection_degraded_at("conn", now));
}

#[test]
fn auth_failure_is_marked_but_never_degraded() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    let record = registry.record_probe_at("conn", "openai", Some(401), None, now);
    assert_eq!(record.status, HealthStatus::AuthFailed);
    assert_eq!(record.degraded_until, None);
    assert_eq!(record.consecutive_failures, 1);
    assert!(!registry.is_connection_degraded_at("conn", now));
}

#[test]
fn transport_failure_degrades_like_server_error() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    let record = registry.record_probe_at(
        "conn",
        "openai",
        None,
        Some("probe transport error: timeout".into()),
        now,
    );
    assert_eq!(record.status, HealthStatus::ServerError);
    assert_eq!(
        record.degraded_until,
        Some(now + ChronoDuration::seconds(300))
    );
}

#[test]
fn degrade_window_expires_after_its_deadline() {
    let registry = HealthRegistry::new();
    let now = fixed_now();
    registry.record_probe_at("conn", "openai", Some(429), None, now);

    assert!(registry.is_connection_degraded_at("conn", now + ChronoDuration::seconds(119)));
    assert!(!registry.is_connection_degraded_at("conn", now + ChronoDuration::seconds(121)));
}

#[test]
fn consecutive_failures_accumulate_until_healthy() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    registry.record_probe_at("conn", "openai", Some(503), None, now);
    let second = registry.record_probe_at("conn", "openai", Some(429), None, now);
    assert_eq!(second.consecutive_failures, 2);

    let healthy = registry.record_probe_at("conn", "openai", Some(200), None, now);
    assert_eq!(healthy.consecutive_failures, 0);
}

#[test]
fn provider_is_degraded_only_when_every_connection_is() {
    let registry = HealthRegistry::new();
    let now = fixed_now();

    registry.record_probe_at("a", "openrouter", Some(503), None, now);
    registry.record_probe_at("b", "openrouter", Some(200), None, now);
    assert!(!registry.is_provider_degraded_at("openrouter", now));

    registry.record_probe_at("b", "openrouter", Some(429), None, now);
    assert!(registry.is_provider_degraded_at("openrouter", now));
}

#[test]
fn unknown_provider_is_never_degraded() {
    let registry = HealthRegistry::new();
    let now = fixed_now();
    registry.record_probe_at("a", "openai", Some(503), None, now);

    assert!(!registry.is_provider_degraded_at("groq", now));
    assert!(!registry.is_model_degraded("groq/llama-3"));
}

#[test]
fn model_alias_prefix_resolves_to_provider() {
    let registry = HealthRegistry::new();
    // Wall-clock base: `is_model_degraded` compares against `Utc::now()`, so the
    // degrade window has to extend into the real future.
    let now = Utc::now();
    registry.record_probe_at("a", "claude", Some(503), None, now);

    assert!(registry.is_model_degraded("cc/claude-opus-4-8"));
    assert!(registry.is_model_degraded("claude/claude-opus-4-8"));
    assert!(!registry.is_model_degraded(""));
}

#[test]
fn summary_reports_status_and_provider_breakdown() {
    let registry = HealthRegistry::new();
    // `summary()` evaluates degrade windows against `Utc::now()`.
    let now = Utc::now();
    registry.record_probe_at("a", "openai", Some(200), None, now);
    registry.record_probe_at("b", "openai", Some(429), None, now);
    registry.record_probe_at("c", "groq", Some(401), None, now);

    let summary = registry.summary();
    assert_eq!(summary.connections, 3);
    assert_eq!(summary.healthy, 1);
    assert_eq!(summary.degraded, 1);
    assert_eq!(summary.by_status.get("rate_limited"), Some(&1));
    assert_eq!(summary.by_status.get("auth_failed"), Some(&1));

    let openai = summary
        .providers
        .iter()
        .find(|entry| entry.provider == "openai")
        .expect("openai summary present");
    assert_eq!(openai.connections, 2);
    assert_eq!(openai.healthy, 1);
    assert_eq!(openai.degraded, 1);
    assert!(openai.degraded_until.is_some());
}

#[test]
fn probe_target_requires_api_key_and_known_base_url() {
    let mut conn = connection("conn", "openai");
    conn.api_key = None;
    assert!(probe_target(&conn).is_none());

    let unknown = connection("conn", "definitely-not-a-provider");
    assert!(probe_target(&unknown).is_none());
}

#[test]
fn probe_target_rewrites_chat_endpoint_to_models_with_bearer() {
    let target = probe_target(&connection("conn", "openai")).expect("openai target");
    assert_eq!(target.url, "https://api.openai.com/v1/models");
    assert!(target
        .headers
        .iter()
        .any(|(name, value)| name == "authorization" && value == "Bearer sk-test"));
}

#[test]
fn probe_target_uses_anthropic_headers_for_messages_endpoint() {
    let target = probe_target(&connection("conn", "anthropic")).expect("anthropic target");
    assert_eq!(target.url, "https://api.anthropic.com/v1/models");
    assert!(target
        .headers
        .iter()
        .any(|(name, value)| name == "x-api-key" && value == "sk-test"));
    assert!(target
        .headers
        .iter()
        .any(|(name, _)| name == "anthropic-version"));
}

#[test]
fn probe_target_uses_google_header_for_gemini() {
    let target = probe_target(&connection("conn", "gemini")).expect("gemini target");
    assert_eq!(
        target.url,
        "https://generativelanguage.googleapis.com/v1beta/models"
    );
    assert!(target
        .headers
        .iter()
        .any(|(name, value)| name == "x-goog-api-key" && value == "sk-test"));
}

#[test]
fn probe_target_prefers_operator_base_url() {
    let mut conn = connection("conn", "openai");
    conn.provider_specific_data.insert(
        "baseUrl".into(),
        json!("https://gateway.internal/v1/chat/completions"),
    );

    let target = probe_target(&conn).expect("custom target");
    assert_eq!(target.url, "https://gateway.internal/v1/models");
}

#[test]
fn needs_persist_detects_transitions_only() {
    let registry = HealthRegistry::new();
    let now = fixed_now();
    let mut conn = connection("conn", "openai");

    let degraded = registry.record_probe_at("conn", "openai", Some(503), None, now);
    assert!(needs_persist(&conn, &degraded));

    conn.extra
        .insert(HEALTH_STATUS_KEY.into(), json!("unavailable"));
    conn.extra.insert(
        DEGRADED_UNTIL_KEY.into(),
        json!(degraded
            .degraded_until
            .expect("degraded deadline")
            .to_rfc3339()),
    );
    assert!(!needs_persist(&conn, &degraded));

    let extended = registry.record_probe_at(
        "conn",
        "openai",
        Some(503),
        None,
        now + ChronoDuration::seconds(180),
    );
    assert!(needs_persist(&conn, &extended));

    let healthy = registry.record_probe_at("conn", "openai", Some(200), None, now);
    assert!(needs_persist(&conn, &healthy));
}

fn with_sticky_error(conn: &mut ProviderConnection) {
    conn.last_error = Some("probe HTTP 503".to_string());
    conn.last_error_at = Some("2026-09-22T18:21:57Z".to_string());
    conn.error_code = Some("503".to_string());
    conn.backoff_level = Some(1);
    conn.consecutive_errors = Some(1);
}

#[test]
fn a_successful_probe_clears_the_sticky_error_fields() {
    let mut conn = connection("conn", "agentrouter");
    conn.test_status = Some("unavailable".to_string());
    with_sticky_error(&mut conn);

    clear_sticky_error(&mut conn);

    assert_eq!(conn.last_error, None);
    assert_eq!(conn.last_error_at, None);
    assert_eq!(conn.error_code, None);
    assert_eq!(conn.backoff_level, Some(0));
    assert_eq!(conn.consecutive_errors, Some(0));
    assert!(!has_sticky_error(&conn));
    // A background probe must not overwrite a user-initiated connection test.
    assert_eq!(conn.test_status.as_deref(), Some("unavailable"));
}

#[test]
fn a_recovered_connection_is_written_even_when_its_status_did_not_change() {
    let registry = HealthRegistry::new();
    let now = fixed_now();
    let mut conn = connection("conn", "agentrouter");
    // The shape observed on the live db: probe says healthy, a request-path
    // error from days earlier is still on the row.
    conn.extra
        .insert(HEALTH_STATUS_KEY.into(), json!("healthy"));
    with_sticky_error(&mut conn);

    let healthy = registry.record_probe_at("conn", "agentrouter", Some(200), None, now);
    assert!(needs_persist(&conn, &healthy));
}

#[test]
fn a_healthy_connection_with_no_error_is_not_rewritten() {
    let registry = HealthRegistry::new();
    let mut conn = connection("conn", "openai");
    conn.extra
        .insert(HEALTH_STATUS_KEY.into(), json!("healthy"));

    let healthy = registry.record_probe_at("conn", "openai", Some(200), None, fixed_now());
    assert!(!needs_persist(&conn, &healthy));
}

#[test]
fn every_sticky_error_field_is_detected() {
    // Guards the disjunction in `has_sticky_error` against being "simplified":
    // a connection left with only `backoff_level` set would otherwise never be
    // rewritten, so the error would survive every future successful probe.
    for set in [
        (Some(0), Some(0), Some("503"), true),
        (Some(1), Some(0), None, true),
        (Some(0), Some(2), None, true),
    ] {
        let mut conn = connection("conn", "openai");
        conn.backoff_level = set.0;
        conn.consecutive_errors = set.1;
        conn.last_error = set.2.map(str::to_string);
        assert_eq!(has_sticky_error(&conn), set.3, "{set:?}");
    }

    let mut clean = connection("conn", "openai");
    clear_sticky_error(&mut clean);
    clean.backoff_level = Some(0);
    clean.consecutive_errors = Some(0);
    assert!(!has_sticky_error(&clean));
}
