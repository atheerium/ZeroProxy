use std::time::{Duration, Instant};

use axum::{
    body::to_bytes,
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reqwest::header as reqwest_header;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::timeout;

use crate::core::executor::provider_config_for;
use crate::core::model::catalog::provider_catalog;
use crate::server::state::AppState;

use super::{chat, provider_models};

const OPENAI_COMPATIBLE_PREFIX: &str = "openai-compatible-";
const ANTHROPIC_COMPATIBLE_PREFIX: &str = "anthropic-compatible-";
const MODEL_TEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
struct TestModelTarget {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderModelTestResult {
    model_id: String,
    name: String,
    ok: bool,
    latency_ms: u64,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderModelTestResponse {
    provider: String,
    connection_id: String,
    results: Vec<ProviderModelTestResult>,
}

pub(super) async fn test_provider_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(response) = super::require_dashboard_or_management_api_key(&headers, &state) {
        return response;
    }

    let Some(connection) = state
        .db
        .snapshot()
        .provider_connections
        .iter()
        .find(|connection| connection.id == id)
        .cloned()
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Connection not found" })),
        )
            .into_response();
    };

    let provider = connection.provider.clone();
    let alias = provider_alias(&provider).to_string();
    let mut models = static_models_for_provider(&provider);

    if models.is_empty() && is_compatible_provider(&provider) {
        models = provider_models::fetch_models_for_connection(&state, &connection)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|model| TestModelTarget {
                name: model.name.clone(),
                id: model.id,
            })
            .collect();
    }

    if models.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "No models configured for this provider" })),
        )
            .into_response();
    }

    let api_key = connection.api_key.as_deref();

    let mut results = Vec::with_capacity(models.len());
    for model in models {
        results.push(ping_model(&alias, &provider, model, api_key).await);
    }

    Json(ProviderModelTestResponse {
        provider,
        connection_id: id,
        results,
    })
    .into_response()
}

fn static_models_for_provider(provider: &str) -> Vec<TestModelTarget> {
    let catalog = provider_catalog();
    let alias = provider_alias(provider);
    catalog
        .models_for_alias(alias)
        .unwrap_or(&[])
        .iter()
        .map(|model| TestModelTarget {
            id: model.id.clone(),
            name: model.name.clone().unwrap_or_else(|| model.id.clone()),
        })
        .collect()
}

fn provider_alias(provider: &str) -> &str {
    provider_catalog()
        .static_alias_for_provider(provider)
        .unwrap_or(provider)
}

fn internal_api_key(state: &AppState) -> Option<String> {
    state
        .db
        .snapshot()
        .api_keys
        .iter()
        .find(|key| key.is_active.unwrap_or(true))
        .map(|key| key.key.clone())
}

fn is_compatible_provider(provider: &str) -> bool {
    provider.starts_with(OPENAI_COMPATIBLE_PREFIX)
        || provider.starts_with(ANTHROPIC_COMPATIBLE_PREFIX)
}

async fn ping_model(
    _alias: &str,
    provider: &str,
    model: TestModelTarget,
    api_key: Option<&str>,
) -> ProviderModelTestResult {
    let start = Instant::now();
    let client = reqwest::Client::builder()
        .timeout(MODEL_TEST_TIMEOUT)
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default();

    let config = provider_config_for(provider);
    let is_anthropic = config
        .map(|c| {
            c.default_headers
                .iter()
                .any(|(k, _)| k == "anthropic-version")
        })
        .unwrap_or(false);
    let base_url = config
        .map(|c| c.base_url.as_str())
        .unwrap_or("https://api.openai.com/v1/chat/completions");

    let mut headers = reqwest_header::HeaderMap::new();
    headers.insert(
        reqwest_header::CONTENT_TYPE,
        reqwest_header::HeaderValue::from_static("application/json"),
    );

    if is_anthropic {
        if let Some(key) = api_key {
            if let Ok(v) = reqwest_header::HeaderValue::from_str(key) {
                headers.insert("x-api-key", v);
            }
        }
        headers.insert(
            "anthropic-version",
            reqwest_header::HeaderValue::from_static("2023-06-01"),
        );
        if provider == "agentrouter" {
            if let Ok(v) =
                reqwest_header::HeaderValue::from_str("claude-cli/2.0.14 (external, cli)")
            {
                headers.insert(reqwest_header::USER_AGENT, v);
            }
        }
    } else {
        if let Some(key) = api_key {
            if let Ok(v) = reqwest_header::HeaderValue::from_str(&format!("Bearer {key}")) {
                headers.insert(reqwest_header::AUTHORIZATION, v);
            }
        }
    }

    for (k, v) in config
        .map(|c| c.default_headers.iter())
        .into_iter()
        .flatten()
    {
        if let Ok(val) = reqwest_header::HeaderValue::from_str(v) {
            headers.entry(k.as_str()).or_insert(val);
        }
    }

    let body = if is_anthropic {
        json!({
            "model": model.id,
            "max_tokens": 1,
            "messages": [{ "role": "user", "content": "hi" }]
        })
    } else {
        json!({
            "model": model.id,
            "max_tokens": 1,
            "stream": false,
            "messages": [{ "role": "user", "content": "hi" }]
        })
    };

    let response = match timeout(
        MODEL_TEST_TIMEOUT,
        client
            .post(base_url)
            .headers(headers)
            .body(body.to_string())
            .send(),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            return ProviderModelTestResult {
                model_id: model.id,
                name: model.name,
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Request failed: {e}")),
            };
        }
        Err(_) => {
            return ProviderModelTestResult {
                model_id: model.id,
                name: model.name,
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some("Request timed out".to_string()),
            };
        }
    };

    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status();
    let ok = status.is_success();
    let error = if ok {
        None
    } else {
        let text = response.text().await.unwrap_or_default();
        let truncated: String = text.chars().take(120).collect();
        Some(format!("HTTP {}: {truncated}", status.as_u16()))
    };

    ProviderModelTestResult {
        model_id: model.id,
        name: model.name,
        ok,
        latency_ms,
        error,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ComboTestModelRequest {
    /// `<provider-prefix>/<model-id>` exactly as it would appear in a
    /// combo's `models` list. Tested via a real
    /// `chat::chat_completions` call with `max_tokens=1`, mirroring the
    /// per-connection `test_provider_models` behaviour so the same
    /// status/latency semantics apply.
    pub(super) model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ComboTestModelResponse {
    pub(super) model: String,
    pub(super) ok: bool,
    pub(super) latency_ms: u64,
    pub(super) error: Option<String>,
}

/// `POST /api/combos/test-model` — quick health check for a single
/// `<prefix>/<model-id>` combo member. Used by the combo edit modal to
/// give the operator a per-row test icon without having to know which
/// connection backs each combo entry.
///
/// The actual request shape (`max_tokens=1`, single `"hi"` message,
/// non-streaming, 15s timeout, treat both `200 OK` and `400 Bad Request`
/// as "model responded") deliberately matches [`ping_model`] so the two
/// surfaces produce comparable results.
pub(super) async fn test_combo_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ComboTestModelRequest>,
) -> Response {
    if let Err(response) = super::require_dashboard_or_management_api_key(&headers, &state) {
        return response;
    }

    let model = req.model.trim();
    if model.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "`model` is required" })),
        )
            .into_response();
    }

    let api_key = internal_api_key(&state);
    let start = Instant::now();
    let mut ping_headers = HeaderMap::new();
    if let Some(api_key) = api_key.as_deref() {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
            ping_headers.insert(AUTHORIZATION, value);
        }
    }

    let body = json!({
        "model": model,
        "max_tokens": 1,
        "stream": false,
        "messages": [{ "role": "user", "content": "hi" }]
    });

    let response = match timeout(
        MODEL_TEST_TIMEOUT,
        chat::chat_completions(State(state.clone()), ping_headers, Ok(Json(body))),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => {
            return Json(ComboTestModelResponse {
                model: model.to_string(),
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some("Request timed out".to_string()),
            })
            .into_response();
        }
    };

    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status();
    let ok = status == StatusCode::OK || status == StatusCode::BAD_REQUEST;
    let error = if ok {
        None
    } else {
        Some(read_error_text(response, status).await)
    };

    Json(ComboTestModelResponse {
        model: model.to_string(),
        ok,
        latency_ms,
        error,
    })
    .into_response()
}

async fn read_error_text(response: Response, status: StatusCode) -> String {
    let text = match to_bytes(response.into_body(), usize::MAX).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => return error.to_string(),
    };

    if text.is_empty() {
        format!("HTTP {}", status.as_u16())
    } else {
        let truncated: String = text.chars().take(120).collect();
        format!("HTTP {}: {truncated}", status.as_u16())
    }
}
