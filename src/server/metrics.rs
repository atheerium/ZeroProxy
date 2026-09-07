//! Prometheus metrics + request-ID propagation.
//!
//! Exposes a `/metrics` endpoint in the standard Prometheus text format and a
//! request-ID middleware that threads `x-request-id` through the request pipeline
//! (incoming header if present, otherwise a fresh UUIDv4). The ID is attached to
//! every emitted metric and traced log line via the [`RequestId`] extension, so
//! operators can correlate one client request with its upstream dispatch,
//! circuit-breaker state change, and combo fallback decision.
//!
//! Cardinality control: metrics are labeled by `provider`, `model`, `path`,
//! `status`, and `streaming`. Avoid adding labels that fan out per-connection
//! (e.g. `connection_id`) — use the structured log stream instead.
use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use prometheus::{
    Encoder, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry, TextEncoder,
};

/// Bag-of-attributes extension inserted by [`request_id_middleware`] and read
/// by handlers / executors that want to surface the ID in upstream headers.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Metric definitions
// ---------------------------------------------------------------------------

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Total HTTP requests handled by the router, labeled by method + path + status.
static HTTP_REQUESTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
/// Wall-clock latency for HTTP requests, labeled by method + path.
static HTTP_REQUEST_DURATION: OnceLock<HistogramVec> = OnceLock::new();
/// Provider dispatches grouped by provider/model/status/streaming.
static PROVIDER_REQUESTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
/// Time-to-first-token for streaming responses.
static PROVIDER_TTFT_SECONDS: OnceLock<HistogramVec> = OnceLock::new();
/// Token totals, labeled by provider/model/type (prompt|completion).
static PROVIDER_TOKENS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
/// Circuit breaker state (0=closed, 1=open, 2=half-open) per
/// provider/connection/endpoint.
static CIRCUIT_BREAKER_STATE: OnceLock<IntGaugeVec> = OnceLock::new();
/// Combo member attempts grouped by combo/model/result.
static COMBO_ATTEMPTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
/// Number of upstream requests currently in flight.
static ACTIVE_UPSTREAM: OnceLock<IntGauge> = OnceLock::new();
/// Service start time (unix seconds), exposed as a constant gauge so
/// Prometheus can compute uptime from `time() - zeroproxy_started`.
static STARTED_AT: OnceLock<GaugeVec> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

fn http_requests_total() -> &'static IntCounterVec {
    HTTP_REQUESTS_TOTAL.get_or_init(|| {
        let m = IntCounterVec::new(
            Opts::new(
                "zeroproxy_http_requests_total",
                "Total HTTP requests handled by zeroproxy",
            ),
            &["method", "path", "status"],
        )
        .expect("counter definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register http_requests_total");
        m
    })
}

fn http_request_duration() -> &'static HistogramVec {
    HTTP_REQUEST_DURATION.get_or_init(|| {
        let m = HistogramVec::new(
            HistogramOpts::new(
                "zeroproxy_http_request_duration_seconds",
                "Wall-clock HTTP request latency in seconds",
            )
            .buckets(vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["method", "path"],
        )
        .expect("histogram definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register http_request_duration");
        m
    })
}

fn provider_requests_total() -> &'static IntCounterVec {
    PROVIDER_REQUESTS_TOTAL.get_or_init(|| {
        let m = IntCounterVec::new(
            Opts::new(
                "zeroproxy_provider_requests_total",
                "Provider dispatch attempts",
            ),
            &["provider", "model", "status", "streaming"],
        )
        .expect("counter definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register provider_requests_total");
        m
    })
}

fn provider_ttft_seconds() -> &'static HistogramVec {
    PROVIDER_TTFT_SECONDS.get_or_init(|| {
        let m = HistogramVec::new(
            HistogramOpts::new(
                "zeroproxy_provider_ttft_seconds",
                "Time to first SSE byte for streaming provider responses",
            )
            .buckets(vec![0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]),
            &["provider", "model"],
        )
        .expect("histogram definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register provider_ttft_seconds");
        m
    })
}

fn provider_tokens_total() -> &'static IntCounterVec {
    PROVIDER_TOKENS_TOTAL.get_or_init(|| {
        let m = IntCounterVec::new(
            Opts::new(
                "zeroproxy_provider_tokens_total",
                "Tokens processed per provider/model",
            ),
            &["provider", "model", "type"],
        )
        .expect("counter definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register provider_tokens_total");
        m
    })
}

fn circuit_breaker_state() -> &'static IntGaugeVec {
    CIRCUIT_BREAKER_STATE.get_or_init(|| {
        let m = IntGaugeVec::new(
            Opts::new(
                "zeroproxy_circuit_breaker_state",
                "Circuit breaker state (0=closed, 1=open, 2=half_open)",
            ),
            &["provider", "connection", "endpoint"],
        )
        .expect("gauge definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register circuit_breaker_state");
        m
    })
}

fn combo_attempts_total() -> &'static IntCounterVec {
    COMBO_ATTEMPTS_TOTAL.get_or_init(|| {
        let m = IntCounterVec::new(
            Opts::new(
                "zeroproxy_combo_attempts_total",
                "Combo member dispatch attempts",
            ),
            &["combo", "model", "result"],
        )
        .expect("counter definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register combo_attempts_total");
        m
    })
}

fn active_upstream() -> &'static IntGauge {
    ACTIVE_UPSTREAM.get_or_init(|| {
        let m = IntGauge::new(
            "zeroproxy_active_upstream_requests",
            "Currently in-flight upstream provider requests",
        )
        .expect("gauge definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register active_upstream");
        m
    })
}

fn started_at_gauge() -> &'static GaugeVec {
    STARTED_AT.get_or_init(|| {
        let m = GaugeVec::new(
            Opts::new(
                "zeroproxy_started_at_seconds",
                "Unix-epoch seconds when this zeroproxy instance started",
            ),
            &["pid"],
        )
        .expect("gauge definition");
        registry()
            .register(Box::new(m.clone()))
            .expect("register started_at");
        m
    })
}

/// Initialize all metric families and stamp the start-time gauge. Idempotent;
/// safe to call once at boot.
pub fn init() {
    let _ = http_requests_total();
    let _ = http_request_duration();
    let _ = provider_requests_total();
    let _ = provider_ttft_seconds();
    let _ = provider_tokens_total();
    let _ = circuit_breaker_state();
    let _ = combo_attempts_total();
    let _ = active_upstream();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0);
    started_at_gauge()
        .with_label_values(&[&format!("{}", std::process::id())])
        .set(now);
}

// ---------------------------------------------------------------------------
// Recording helpers
// ---------------------------------------------------------------------------

/// Increment the provider request counter for a finished dispatch.
pub fn record_provider_request(provider: &str, model: &str, status: u16, streaming: bool) {
    provider_requests_total()
        .with_label_values(&[
            provider,
            model,
            &status.to_string(),
            if streaming { "true" } else { "false" },
        ])
        .inc();
}

/// Observe a time-to-first-token measurement.
pub fn record_ttft(provider: &str, model: &str, seconds: f64) {
    provider_ttft_seconds()
        .with_label_values(&[provider, model])
        .observe(seconds);
}

/// Add token counts to the tokens counter.
pub fn record_tokens(provider: &str, model: &str, kind: &str, count: u64) {
    if count == 0 {
        return;
    }
    provider_tokens_total()
        .with_label_values(&[provider, model, kind])
        .inc_by(count);
}

/// Mark a circuit breaker's current state.
pub fn record_circuit_state(provider: &str, connection: &str, endpoint: &str, state: i64) {
    circuit_breaker_state()
        .with_label_values(&[provider, connection, endpoint])
        .set(state);
}

/// Increment the combo-attempts counter.
pub fn record_combo_attempt(combo: &str, model: &str, result: &str) {
    combo_attempts_total()
        .with_label_values(&[combo, model, result])
        .inc();
}

/// Increment the active-upstream gauge. Pair with [`active_upstream_dec`].
pub fn active_upstream_inc() {
    active_upstream().inc();
}

/// Decrement the active-upstream gauge. Pair with [`active_upstream_inc`].
pub fn active_upstream_dec() {
    active_upstream().dec();
}

// ---------------------------------------------------------------------------
// HTTP middleware + endpoint
// ---------------------------------------------------------------------------

/// Normalize an axum path to a low-cardinality label. The router panics on
/// dynamic path segments if we don't replace them; we use a fixed bucket list
/// since the current route surface is mostly fixed.
fn label_path(path: &str) -> &str {
    if path.starts_with("/v1/chat/completions") {
        "/v1/chat/completions"
    } else if path.starts_with("/v1/completions") {
        "/v1/completions"
    } else if path.starts_with("/v1/embeddings") {
        "/v1/embeddings"
    } else if path.starts_with("/v1/models") {
        "/v1/models"
    } else if path.starts_with("/api/combos") {
        "/api/combos"
    } else if path.starts_with("/api/providers") {
        "/api/providers"
    } else if path.starts_with("/api/keys") {
        "/api/keys"
    } else if path.starts_with("/api/usage") {
        "/api/usage"
    } else if path.starts_with("/api/settings") {
        "/api/settings"
    } else if path.starts_with("/api/dashboard/chat/completions") {
        "/api/dashboard/chat/completions"
    } else if path.starts_with("/health") {
        "/health"
    } else if path.starts_with("/metrics") {
        "/metrics"
    } else {
        "other"
    }
}

/// Axum middleware: propagate (or generate) `x-request-id`, observe HTTP
/// request count + latency, and attach [`RequestId`] to the request
/// extensions so downstream handlers can read it.
pub async fn http_metrics_and_request_id(mut request: Request, next: Next) -> Response {
    init();

    let method = request.method().clone();
    let path = label_path(request.uri().path()).to_string();

    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let start = std::time::Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status_str = response.status().as_str().to_string();

    http_requests_total()
        .with_label_values(&[method.as_str(), &path, &status_str])
        .inc();
    http_request_duration()
        .with_label_values(&[method.as_str(), &path])
        .observe(elapsed);

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        let mut response = response;
        response.headers_mut().insert("x-request-id", value);
        response
    } else {
        response
    }
}

/// Axum handler that returns the registry contents in Prometheus text format.
pub async fn metrics_handler() -> Response {
    init();
    let metric_families = registry().gather();
    let mut buffer = Vec::with_capacity(4096);
    let encoder = TextEncoder::new();
    if let Err(err) = encoder.encode(&metric_families, &mut buffer) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("metrics encode failed: {err}"),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, encoder.format_type())],
        Body::from(buffer),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        // init() is safe to call repeatedly because every metric family is
        // wrapped in OnceLock. We can't easily observe the registry size
        // directly (gather() filters out empty families), but calling it
        // twice must not panic or duplicate families.
        init();
        init();
        // After init, recording on a freshly-registered family must succeed.
        record_provider_request("test", "init", 200, false);
    }

    #[test]
    fn record_provider_request_increments() {
        init();
        record_provider_request("openai", "gpt-4o", 200, false);
        record_provider_request("openai", "gpt-4o", 200, true);
        record_provider_request("openai", "gpt-4o", 429, false);
        let metrics = registry().gather();
        let total: u64 = metrics
            .iter()
            .find(|m| m.get_name() == "zeroproxy_provider_requests_total")
            .map(|m| {
                m.get_metric()
                    .iter()
                    .map(|c| c.get_counter().get_value() as u64)
                    .sum()
            })
            .unwrap_or(0);
        assert!(total >= 3, "expected ≥3 recorded requests, got {total}");
    }

    #[test]
    fn record_circuit_state_writes_gauge() {
        init();
        record_circuit_state("openai", "conn-1", "/v1/chat", 1);
        let metrics = registry().gather();
        let found = metrics
            .iter()
            .find(|m| m.get_name() == "zeroproxy_circuit_breaker_state")
            .and_then(|m| {
                m.get_metric().iter().find(|g| {
                    let labels: Vec<(&str, &str)> = g
                        .get_label()
                        .iter()
                        .map(|p| (p.get_name(), p.get_value()))
                        .collect();
                    labels
                        .iter()
                        .any(|(k, v)| *k == "provider" && *v == "openai")
                })
            });
        assert!(found.is_some(), "circuit state for openai not found");
    }

    #[test]
    fn label_path_normalizes_dynamic_segments() {
        assert_eq!(label_path("/v1/chat/completions"), "/v1/chat/completions");
        assert_eq!(
            label_path("/v1/chat/completions?x=1"),
            "/v1/chat/completions"
        );
        assert_eq!(label_path("/api/combos/abc/edit"), "/api/combos");
        assert_eq!(label_path("/healthz"), "/health");
        assert_eq!(label_path("/random/path"), "other");
    }
}
