//! Weighted scoring for smart-auto candidates.
//!
//! All pricing lookups mirror `src/core/combo/ordering.rs` (same rows, same
//! alias/`all` fallback, same per-token math); any divergence there should be
//! ported here — see the `mirrors ordering.rs` notes. Live account health
//! comes from `account_fallback`; no `UsageDb` scans (per-request cost stays
//! O(candidates)).
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::core::account_fallback::{calculate_account_health, filter_available_accounts};
use crate::core::model::resolve_provider_alias;
use crate::core::usage::{parse_model_pricing, CostModel};
use crate::types::{AppDb, PricingTable};

use super::AutoWeights;

pub(crate) struct StaticHints {
    reliability: f64,
    latency: Option<f64>,
    intelligence: Option<f64>,
    cost: f64,
}

/// Pricing row lookup (mirrors `pricing_entry` in
/// `src/core/combo/ordering.rs`: alias or canonical provider key, exact model
/// or provider-wide `all` row).
pub(crate) fn pricing_entry<'a>(
    model: &'a str,
    pricing: &'a PricingTable,
) -> Option<(&'a Value, &'a str)> {
    let (prefix, name) = model.split_once('/')?;
    let resolved = resolve_provider_alias(prefix);
    for provider in [prefix, resolved.as_str()] {
        if let Some(models) = pricing.get(provider) {
            if let Some(entry) = models.get(name).or_else(|| models.get("all")) {
                return Some((entry, name));
            }
        }
    }
    None
}

/// Representative per-request cost in USD per 1M in + out (mirrors
/// `model_cost` in `src/core/combo/ordering.rs`): missing rows and
/// non-per-token cost models are free.
pub(crate) fn model_cost(model: &str, pricing: &PricingTable) -> f64 {
    let Some((entry, name)) = pricing_entry(model, pricing) else {
        return 0.0;
    };
    let provider = model.split('/').next().unwrap_or_default();
    let parsed = parse_model_pricing(provider, name, entry);
    match parsed.cost_model {
        CostModel::PerToken => parsed.input_price_per_million + parsed.output_price_per_million,
        CostModel::Free | CostModel::FlatMonthly | CostModel::Credits => 0.0,
    }
}

/// Free only with evidence: `CostModel::Free`, zero computed price, or an
/// explicitly free-looking model id. A **missing pricing row is unknown, not
/// free** — BestFree must not admit metered models just because this
/// deployment has an empty `AppDb.pricing` table.
pub(crate) fn is_free_model(model: &str, pricing: &PricingTable) -> bool {
    let Some((entry, name)) = pricing_entry(model, pricing) else {
        return looks_free_by_name(model);
    };
    let provider = model.split('/').next().unwrap_or_default();
    let parsed = parse_model_pricing(provider, name, entry);
    parsed.cost_model == CostModel::Free || model_cost(model, pricing) <= 0.0
}

/// Heuristic for ids without a pricing row: OpenRouter-style `:free` suffix,
/// a model segment that is/ends with `free`, or a `-free` marker.
fn looks_free_by_name(model: &str) -> bool {
    let name = model
        .rsplit_once('/')
        .map(|(_, n)| n)
        .unwrap_or(model)
        .to_ascii_lowercase();
    name.ends_with(":free")
        || name.ends_with("-free")
        || name == "free"
        || name.starts_with("free-")
        || name.ends_with("-free-1")
        || name.split(':').any(|p| p == "free")
}

/// Static pricing hints (mirrors `sort_models_by_balanced` in
/// `src/core/combo/ordering.rs`).
pub(crate) fn static_hints(model: &str, pricing: &PricingTable) -> StaticHints {
    let (reliability, latency, intelligence) = match pricing_entry(model, pricing) {
        Some((entry, _)) => {
            let obj = entry.as_object();
            let reliability = obj
                .and_then(|o| {
                    o.get("reliabilityScore")
                        .or_else(|| o.get("reliability_score"))
                        .or_else(|| o.get("reliability"))
                })
                .and_then(Value::as_f64)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);
            let latency = obj.and_then(|o| {
                ["latency", "latencyMs", "latency_ms"]
                    .iter()
                    .find_map(|f| o.get(*f).and_then(Value::as_f64))
            });
            // mirrors ordering.rs intelligence derivation: score override,
            // else sizeLabel-tier composite minus rank penalty.
            let intelligence = obj.and_then(|o| {
                if let Some(v) = o
                    .get("intelligenceScore")
                    .or_else(|| o.get("intelligence_score"))
                    .and_then(Value::as_f64)
                {
                    return Some(v);
                }
                let tier = match o
                    .get("sizeLabel")
                    .or_else(|| o.get("size_label"))
                    .or_else(|| o.get("size"))
                    .and_then(Value::as_str)
                    .unwrap_or("Medium")
                    .trim()
                    .to_lowercase()
                    .as_str()
                {
                    "frontier" => 4.0,
                    "large" => 3.0,
                    "medium" => 2.0,
                    "small" => 1.0,
                    _ => 0.0,
                };
                let rank = o
                    .get("intelligenceRank")
                    .or_else(|| o.get("intelligence_rank"))
                    .or_else(|| o.get("rank"))
                    .and_then(Value::as_u64)
                    .unwrap_or(500) as u32;
                Some(tier * 1000.0 - (rank.max(1) as f64).sqrt() * 31.0)
            });
            (reliability, latency, intelligence)
        }
        None => (0.5, None, None),
    };
    StaticHints {
        reliability,
        latency,
        intelligence,
        cost: model_cost(model, pricing),
    }
}

fn min_max(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    (
        values.iter().cloned().fold(f64::INFINITY, f64::min),
        values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Stable best-first ordering of pre-filtered survivors. Reliability blends
/// the static hint with live account health; speed/intelligence/cost are
/// min-max normalized within the set (missing → 0.5, all-equal cost → 1.0).
pub(crate) fn order_by_score(
    survivors: Vec<String>,
    snapshot: &AppDb,
    weights: AutoWeights,
    now: DateTime<Utc>,
) -> Vec<String> {
    if survivors.is_empty() {
        return Vec::new();
    }
    let statics: Vec<StaticHints> = survivors
        .iter()
        .map(|m| static_hints(m, &snapshot.pricing))
        .collect();
    let max_latency = statics
        .iter()
        .filter_map(|h| h.latency)
        .fold(0.0_f64, f64::max);
    let present_intel: Vec<f64> = statics.iter().filter_map(|h| h.intelligence).collect();
    let (min_intel, max_intel) = min_max(&present_intel);
    let costs: Vec<f64> = statics.iter().map(|h| h.cost).collect();
    let (min_cost, max_cost) = min_max(&costs);

    let mut scored: Vec<(usize, f64)> = survivors
        .iter()
        .enumerate()
        .map(|(idx, cand)| {
            let (provider, model) = cand.split_once('/').unwrap_or((cand.as_str(), ""));
            let available = filter_available_accounts(
                &snapshot.provider_connections,
                provider,
                model,
                None,
                now,
            );
            let health_norm = if available.is_empty() {
                0.0
            } else {
                available
                    .iter()
                    .map(|c| calculate_account_health(c, now))
                    .sum::<f64>()
                    / (available.len() as f64 * 100.0)
            }
            .clamp(0.0, 1.0);
            let reliability = 0.6 * statics[idx].reliability + 0.4 * health_norm;
            let speed = match statics[idx].latency {
                Some(lat) if max_latency > 0.0 => (1.0 - lat / max_latency).clamp(0.0, 1.0),
                _ => 0.5,
            };
            let intelligence = match statics[idx].intelligence {
                Some(v) if max_intel > min_intel => {
                    ((v - min_intel) / (max_intel - min_intel)).clamp(0.0, 1.0)
                }
                Some(_) => 1.0,
                None => 0.5,
            };
            let cost = if max_cost > min_cost {
                1.0 - ((statics[idx].cost - min_cost) / (max_cost - min_cost)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let overall = weights.reliability * reliability
                + weights.speed * speed
                + weights.intelligence * intelligence
                + weights.cost * cost;
            (idx, overall)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    scored
        .into_iter()
        .map(|(idx, _)| survivors[idx].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_pricing_row_is_not_free_unless_name_says_so() {
        let empty = PricingTable::new();
        // Metered-looking ids with no row must be excluded from BestFree.
        assert!(!is_free_model("google/gemini-3.1-pro-preview", &empty));
        assert!(!is_free_model("openai/gpt-5.2", &empty));
        // Explicit free naming still qualifies without a row.
        assert!(is_free_model("meta-llama/llama-3.3-70b:free", &empty));
        assert!(is_free_model("mistralai/mistral-small-free", &empty));
        assert!(is_free_model("provider/free", &empty));
    }

    #[test]
    fn explicit_free_pricing_row_is_free() {
        let mut pricing = PricingTable::new();
        pricing.insert(
            "google".into(),
            [("gemini-flash".to_string(), json!({"price": 0}))]
                .into_iter()
                .collect(),
        );
        assert!(is_free_model("google/gemini-flash", &pricing));
        assert!(!is_free_model("google/gemini-pro", &pricing));
    }
}
