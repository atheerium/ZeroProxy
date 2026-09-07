//! Per-key daily budget cap & request-count hard kill-switch.
//!
//! Enforces `ApiKey.daily_budget_usd` and `ApiKey.daily_request_limit` against
//! today's tracked spend and request count. When the cap is reached the request
//! is hard-blocked with HTTP 429; otherwise a successful response carries
//! `X-Daily-Remaining` so clients can surface remaining quota.
//!
//! Mirrors [`super::budget_guard`] but scoped to today (UTC) instead of the
//! current calendar month.

use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde_json::json;

use crate::server::state::AppState;
use crate::types::UsageDb;

/// Response header carrying remaining daily budget (USD).
pub const DAILY_BUDGET_REMAINING_HEADER: &str = "x-daily-budget-remaining";
/// Response header carrying remaining daily request count.
pub const DAILY_REQUESTS_REMAINING_HEADER: &str = "x-daily-requests-remaining";

/// Robot-envelope schema stamped on the 429 body when the daily quota is exceeded.
pub const DAILY_QUOTA_EXCEEDED_SCHEMA: &str = "zeroproxy.v1.daily-quota.exceeded";

/// Enforce the per-key daily budget and request-count limits.
///
/// Returns `Ok((Option<f64>, Option<u64>)>)` where the tuple contains the
/// remaining daily budget (USD) and remaining request count, each `None` when
/// no limit is configured.
/// Returns `Err(response)` with a 429 when any limit is exceeded.
pub fn enforce_daily_quota(
    state: &AppState,
    presented_api_key: Option<&str>,
) -> Result<(Option<f64>, Option<u64>), Response> {
    let Some(raw_key) = presented_api_key else {
        return Ok((None, None));
    };

    let snapshot = state.db.snapshot();
    let Some(api_key) = snapshot.api_keys.iter().find(|k| k.key == raw_key) else {
        return Ok((None, None));
    };

    let daily_budget = api_key.daily_budget();
    let daily_request_limit = api_key.daily_request_limit();

    if daily_budget.is_none() && daily_request_limit.is_none() {
        return Ok((None, None));
    }

    let usage_db = state.usage_tracker().get_usage_db();
    let today = Utc::now().date_naive().to_string();

    let (spent, request_count) = daily_spend_and_count(&usage_db, raw_key, &today);

    // Check daily USD budget
    if let Some(budget) = daily_budget {
        if spent >= budget {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": {
                        "message": format!(
                            "Daily budget of ${:.2} exceeded (spent ${:.2}).",
                            budget, spent
                        ),
                        "type": "daily_budget_exceeded",
                        "code": "daily_budget_exceeded",
                    },
                    "schema": DAILY_QUOTA_EXCEEDED_SCHEMA,
                })),
            )
                .into_response();
            if let Ok(value) = HeaderValue::from_str("0") {
                response
                    .headers_mut()
                    .insert(DAILY_BUDGET_REMAINING_HEADER, value);
            }
            if let Some(limit) = daily_request_limit {
                let remaining = limit.saturating_sub(request_count);
                if let Ok(value) = HeaderValue::from_str(&remaining.to_string()) {
                    response
                        .headers_mut()
                        .insert(DAILY_REQUESTS_REMAINING_HEADER, value);
                }
            }
            return Err(response);
        }
    }

    // Check daily request count
    if let Some(limit) = daily_request_limit {
        if request_count >= limit {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": {
                        "message": format!(
                            "Daily request limit of {} exceeded (used {}).",
                            limit, request_count
                        ),
                        "type": "daily_request_limit_exceeded",
                        "code": "daily_request_limit_exceeded",
                    },
                    "schema": DAILY_QUOTA_EXCEEDED_SCHEMA,
                })),
            )
                .into_response();
            if let Some(budget) = daily_budget {
                let remaining = (budget - spent).max(0.0);
                if let Ok(value) = HeaderValue::from_str(&format!("{remaining:.2}")) {
                    response
                        .headers_mut()
                        .insert(DAILY_BUDGET_REMAINING_HEADER, value);
                }
            }
            if let Ok(value) = HeaderValue::from_str("0") {
                response
                    .headers_mut()
                    .insert(DAILY_REQUESTS_REMAINING_HEADER, value);
            }
            return Err(response);
        }
    }

    let remaining_budget = daily_budget.map(|b| (b - spent).max(0.0));
    let remaining_requests = daily_request_limit.map(|l| l.saturating_sub(request_count));
    Ok((remaining_budget, remaining_requests))
}

/// Stamp daily quota headers onto a successful response.
pub fn with_daily_quota_headers(
    mut response: Response,
    budget_remaining: Option<f64>,
    requests_remaining: Option<u64>,
) -> Response {
    if let Some(remaining) = budget_remaining {
        if let Ok(value) = HeaderValue::from_str(&format!("{remaining:.2}")) {
            response
                .headers_mut()
                .insert(DAILY_BUDGET_REMAINING_HEADER, value);
        }
    }
    if let Some(remaining) = requests_remaining {
        if let Ok(value) = HeaderValue::from_str(&remaining.to_string()) {
            response
                .headers_mut()
                .insert(DAILY_REQUESTS_REMAINING_HEADER, value);
        }
    }
    response
}

/// Sum the USD cost and request count for `api_key` on a given UTC date.
fn daily_spend_and_count(usage_db: &UsageDb, api_key: &str, date: &str) -> (f64, u64) {
    let mut spent = 0.0;
    let mut count = 0u64;
    for entry in &usage_db.history {
        if entry.api_key.as_deref() != Some(api_key) {
            continue;
        }
        let matches_date = entry
            .timestamp
            .as_deref()
            .map(|ts| ts.starts_with(date))
            .unwrap_or(false);
        if !matches_date {
            continue;
        }
        spent += entry.cost.unwrap_or(0.0);
        count += 1;
    }
    (spent, count)
}

/// Build a per-provider daily quota summary for the API response.
pub fn daily_quota_summary(state: &AppState) -> serde_json::Value {
    let usage_db = state.usage_tracker().get_usage_db();
    let today = Utc::now().date_naive().to_string();
    let snapshot = state.db.snapshot();

    let mut by_provider: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();

    // Aggregate today's stats per provider
    let mut provider_requests: std::collections::BTreeMap<String, (u64, u64, f64)> =
        std::collections::BTreeMap::new(); // provider -> (requests, tokens, cost)

    for entry in &usage_db.history {
        let matches_date = entry
            .timestamp
            .as_deref()
            .map(|ts| ts.starts_with(&today))
            .unwrap_or(false);
        if !matches_date {
            continue;
        }
        let provider = entry.provider.as_deref().unwrap_or("unknown").to_string();
        let tokens = entry
            .tokens
            .as_ref()
            .and_then(|t| {
                t.prompt_tokens
                    .or(t.input_tokens)
                    .map(|p| p + t.completion_tokens.or(t.output_tokens).unwrap_or(0))
            })
            .unwrap_or(0);
        let entry_cost = entry.cost.unwrap_or(0.0);
        let slot = provider_requests.entry(provider).or_insert((0, 0, 0.0));
        slot.0 += 1;
        slot.1 += tokens;
        slot.2 += entry_cost;
    }

    for (provider, (requests, tokens, cost)) in &provider_requests {
        // Find budget limits for this provider's API keys
        let daily_budget_limit: Option<f64> = snapshot
            .api_keys
            .iter()
            .filter_map(|k| k.daily_budget())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let daily_request_limit: Option<u64> = snapshot
            .api_keys
            .iter()
            .filter_map(|k| k.daily_request_limit())
            .max_by(|a, b| a.cmp(b));

        by_provider.insert(
            provider.clone(),
            json!({
                "requests": requests,
                "tokens": tokens,
                "cost": format!("{:.4}", cost),
                "dailyBudgetLimit": daily_budget_limit,
                "dailyRequestLimit": daily_request_limit,
                "budgetRemaining": daily_budget_limit.map(|b| format!("{:.4}", (b - cost).max(0.0))),
                "requestsRemaining": daily_request_limit.map(|l| l.saturating_sub(*requests)),
            }),
        );
    }

    json!({
        "date": today,
        "byProvider": by_provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TokenUsage, UsageEntry};

    fn entry(api_key: &str, date: &str, cost: f64) -> UsageEntry {
        UsageEntry {
            timestamp: Some(format!("{date}T12:00:00Z")),
            provider: Some("openai".into()),
            model: "gpt-4o".into(),
            connection_id: Some("c1".into()),
            api_key: Some(api_key.into()),
            tokens: Some(TokenUsage {
                prompt_tokens: Some(10),
                input_tokens: None,
                completion_tokens: Some(20),
                output_tokens: None,
                total_tokens: None,
                reasoning_tokens: None,
                cached_tokens: None,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                extra: Default::default(),
            }),
            cost: Some(cost),
            status: None,
            endpoint: None,
            bytes_before: 0,
            bytes_after: 0,
            bytes_saved: 0,
            image_prompts: 0,
            extra: Default::default(),
        }
    }

    #[test]
    fn daily_spend_sums_only_matching_date() {
        let mut usage = UsageDb::default();
        let today = Utc::now().date_naive().to_string();
        let yesterday = (Utc::now() - chrono::Duration::days(1))
            .date_naive()
            .to_string();
        usage.history.push(entry("k1", &today, 2.5));
        usage.history.push(entry("k1", &today, 1.5));
        usage.history.push(entry("k1", &yesterday, 10.0)); // wrong day
        usage.history.push(entry("k2", &today, 5.0)); // wrong key

        let (spent, count) = daily_spend_and_count(&usage, "k1", &today);
        assert_eq!(spent, 4.0);
        assert_eq!(count, 2);
    }
}
