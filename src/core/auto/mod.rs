//! Smart-auto model selection (`auto`, `auto/best-coding`, `auto/best-free`).
//!
//! Picks an ordered `provider/model` candidate list for requests that name a
//! virtual `auto*` model. The sibling chat dispatcher tries the returned list
//! best-first with its normal per-account fallback.
//!
//! # Candidate universe
//!
//! (a) [`crate::core::performance::get_auto_pool_models`] entries whose
//! provider prefix resolves to a *configured* provider, plus (b) per-provider
//! *enabled available* models. Configured means the provider owns at least one
//! usable [`crate::types::ProviderConnection`] (active + credentials present,
//! mirroring `connection_has_credentials` in `src/server/api/chat.rs`).
//!
//! Source of truth for (b): the static provider catalog
//! ([`crate::core::model::catalog::provider_catalog`], `models_for_alias` —
//! the same listing the dashboard `/dashboard/providers/<provider>` page and
//! `ModelSelectModal` render) minus the dashboard disabled map
//! (`AppDb.extra["disabledModels"]`, provider → ids, as read by
//! `models_disabled::disabled_models_from_db`), intersected with each
//! connection's `enabledModels` allow-list when present, plus
//! `snapshot.custom_models` rows for configured providers (the dashboard
//! custom-model source of truth, same rows `/v1/models` merges).
//!
//! # Cost discipline
//!
//! No full `UsageDb` scans here — per-request selection reads only static
//! pricing hints plus live health / breaker / account state, so selection
//! stays O(candidates), never O(history).
use std::collections::{BTreeMap, HashSet};

use chrono::Utc;
use serde_json::Value;

use crate::core::account_fallback::filter_available_accounts;
use crate::core::circuit_breaker::{CircuitBreakerRegistry, CircuitState};
use crate::core::combo::quarantined_members;
use crate::core::model::{catalog::provider_catalog, resolve_provider_alias};
use crate::core::performance::get_auto_pool_models;
use crate::types::{AppDb, ProviderConnection, Settings};

mod scoring;
use scoring::{is_free_model, order_by_score};

/// Quarantine namespace consulted by [`select_candidates`].
pub const AUTO_QUARANTINE_KEY: &str = "auto";

/// Hard capabilities — a candidate missing any *required* one is dropped
/// outright (mirrors `HARD_CAPS` in `src/core/combo/mod.rs`).
const HARD_CAPS: &[&str] = &["vision", "pdf", "audioInput", "videoInput"];

/// Chat endpoints whose breaker keys gate auto candidates. Keys are built
/// with the exact construction in `src/server/api/chat.rs:1523` —
/// `key("{provider}:{connection_id}", endpoint)` — for every endpoint chat
/// traffic actually uses, since [`select_candidates`] is endpoint-agnostic.
const CIRCUIT_ENDPOINTS: &[&str] = &[
    "/v1/chat/completions",
    "/api/dashboard/chat/completions",
    "default",
];

/// Which `auto*` flavour was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoPreset {
    Balanced,
    BestCoding,
    BestFree,
}

/// Parse a virtual model name into an [`AutoPreset`].
///
/// Trimmed, case-insensitive exact match only (`"auto"` → [`AutoPreset::Balanced`],
/// `"auto/best-coding"` → [`AutoPreset::BestCoding`],
/// `"auto/best-free"` → [`AutoPreset::BestFree`]); anything else is `None`.
/// Callers must run this *before* any provider/model split (`"auto/…"`
/// contains a `/`).
pub fn parse_preset(model: &str) -> Option<AutoPreset> {
    match model.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(AutoPreset::Balanced),
        "auto/best-coding" => Some(AutoPreset::BestCoding),
        "auto/best-free" => Some(AutoPreset::BestFree),
        _ => None,
    }
}

/// Weighted scoring mix. Weights must sum to ~1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoWeights {
    pub reliability: f64,
    pub speed: f64,
    pub intelligence: f64,
    pub cost: f64,
}

/// Scoring weights for `preset`, honouring `settings.extra["autoWeights"]`
/// overrides (`{balanced,bestCoding,bestFree: {reliability,speed,intelligence,cost}}`).
/// An override applies only when all four keys are present and sum within
/// 0.001 of 1.0 (mirrors `ScoringWeights::validate`); otherwise the default
/// applies (Balanced 0.40/0.25/0.20/0.15, BestCoding 0.30/0.15/0.45/0.10,
/// BestFree 0.50/0.30/0.15/0.05).
impl AutoWeights {
    fn default_for(preset: AutoPreset) -> Self {
        match preset {
            AutoPreset::Balanced => Self {
                reliability: 0.40,
                speed: 0.25,
                intelligence: 0.20,
                cost: 0.15,
            },
            AutoPreset::BestCoding => Self {
                reliability: 0.30,
                speed: 0.15,
                intelligence: 0.45,
                cost: 0.10,
            },
            AutoPreset::BestFree => Self {
                reliability: 0.50,
                speed: 0.30,
                intelligence: 0.15,
                cost: 0.05,
            },
        }
    }
}

/// Scoring weights for `preset`, honouring `settings.extra["autoWeights"]`
/// overrides (`{balanced,bestCoding,bestFree: {reliability,speed,intelligence,cost}}`).
/// An override applies only when all four keys are present and sum within
/// 0.001 of 1.0 (mirrors `ScoringWeights::validate`); otherwise the default
/// applies (Balanced 0.40/0.25/0.20/0.15, BestCoding 0.30/0.15/0.45/0.10,
/// BestFree 0.50/0.30/0.15/0.05).
pub fn weights_for(preset: AutoPreset, settings: &Settings) -> AutoWeights {
    let fallback = AutoWeights::default_for(preset);
    let key = match preset {
        AutoPreset::Balanced => "balanced",
        AutoPreset::BestCoding => "bestCoding",
        AutoPreset::BestFree => "bestFree",
    };
    let Some(entry) = settings
        .extra
        .get("autoWeights")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_object)
    else {
        return fallback;
    };
    let get = |k: &str| entry.get(k).and_then(Value::as_f64);
    let (Some(r), Some(s), Some(i), Some(c)) = (
        get("reliability"),
        get("speed"),
        get("intelligence"),
        get("cost"),
    ) else {
        return fallback;
    };
    if ((r + s + i + c) - 1.0).abs() >= 0.001 {
        return fallback;
    }
    AutoWeights {
        reliability: r,
        speed: s,
        intelligence: i,
        cost: c,
    }
}

/// Ordered `provider/model` candidates, best first (may be empty).
///
/// Hard-drops anything missing a required hard capability, health-degraded,
/// circuit-open on every usable connection, without an available account,
/// quarantined under [`AUTO_QUARANTINE_KEY`], or (for [`AutoPreset::BestFree`])
/// metered. Survivors are soft-scored and stable-sorted descending.
pub fn select_candidates(
    preset: AutoPreset,
    snapshot: &AppDb,
    required_caps: &HashSet<String>,
    circuit: &CircuitBreakerRegistry,
) -> Vec<String> {
    let now = Utc::now();
    let mut configured: Vec<String> = Vec::new();
    for conn in &snapshot.provider_connections {
        if is_usable_connection(conn) && !configured.iter().any(|p| p == &conn.provider) {
            configured.push(conn.provider.clone());
        }
    }
    if configured.is_empty() {
        return Vec::new();
    }

    let mut universe: Vec<String> = Vec::new();
    let mut push_unique = |entry: String| {
        if !universe.contains(&entry) {
            universe.push(entry);
        }
    };
    for entry in get_auto_pool_models(snapshot) {
        let Some((prefix, name)) = entry.split_once('/') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(owner) = configured.iter().find(|c| same_provider(prefix, c)) {
            let cand = format!("{owner}/{name}");
            if supports_any(snapshot, owner, name) {
                push_unique(cand);
            }
        }
    }
    let disabled = disabled_models_map(&snapshot.extra);
    for provider in &configured {
        for id in enabled_models_for(snapshot, provider, &disabled) {
            if supports_any(snapshot, provider, &id) {
                push_unique(format!("{provider}/{id}"));
            }
        }
    }
    if universe.is_empty() {
        return Vec::new();
    }

    let quarantined = quarantined_members(AUTO_QUARANTINE_KEY);
    let mut survivors: Vec<String> = Vec::new();
    for cand in &universe {
        let Some((provider, model)) = cand.split_once('/') else {
            continue;
        };
        if missing_hard_cap(cand, required_caps) {
            continue;
        }
        if crate::core::health::is_model_degraded(cand) {
            continue;
        }
        let usable: Vec<&ProviderConnection> = snapshot
            .provider_connections
            .iter()
            .filter(|c| c.provider == provider && is_usable_connection(c))
            .collect();
        if usable.is_empty()
            || usable
                .iter()
                .all(|c| circuit_blocked(circuit, provider, &c.id))
        {
            continue;
        }
        let available =
            filter_available_accounts(&snapshot.provider_connections, provider, model, None, now);
        if available.is_empty() {
            continue;
        }
        if quarantined.contains(cand) {
            continue;
        }
        if preset == AutoPreset::BestFree && !is_free_model(cand, &snapshot.pricing) {
            continue;
        }
        survivors.push(cand.clone());
    }
    if survivors.is_empty() {
        return Vec::new();
    }

    let weights = weights_for(preset, &snapshot.settings);
    order_by_score(survivors, snapshot, weights, now)
}

fn is_usable_connection(conn: &ProviderConnection) -> bool {
    conn.is_active()
        && (conn
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_some()
            || conn
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_some())
}

fn same_provider(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
        || resolve_provider_alias(a).eq_ignore_ascii_case(&resolve_provider_alias(b))
}

fn disabled_models_map(extra: &BTreeMap<String, Value>) -> BTreeMap<String, Vec<String>> {
    extra
        .get("disabledModels")
        .cloned()
        .map(|v| serde_json::from_value(v).unwrap_or_default())
        .unwrap_or_default()
}

fn disabled_ids_for(provider: &str, map: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut ids = Vec::new();
    for (key, values) in map {
        if same_provider(key, provider) {
            ids.extend(values.iter().cloned());
        }
    }
    ids
}

fn bare_id(entry: &str) -> &str {
    entry.rsplit('/').next().unwrap_or(entry).trim()
}

fn model_ids_match(advertised: &str, requested: &str) -> bool {
    let advertised = advertised.trim();
    let requested = requested.trim();
    advertised == requested || advertised.ends_with(&format!("/{requested}"))
}

fn supports_any(snapshot: &AppDb, provider: &str, model_id: &str) -> bool {
    snapshot
        .provider_connections
        .iter()
        .filter(|c| c.provider == provider && is_usable_connection(c))
        .any(|conn| {
            let enabled: Vec<&str> = conn
                .provider_specific_data
                .get("enabledModels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            if !enabled.is_empty() {
                return enabled.iter().any(|v| model_ids_match(v, model_id));
            }
            conn.default_model
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none_or(|v| model_ids_match(v, model_id))
        })
}

fn enabled_models_for(
    snapshot: &AppDb,
    provider: &str,
    disabled: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let disabled_ids = disabled_ids_for(provider, disabled);
    let is_disabled = |id: &str| disabled_ids.iter().any(|d| d == id);
    let mut ids: Vec<String> = Vec::new();
    let mut push = |id: &str| {
        let id = id.trim();
        if !id.is_empty() && !is_disabled(id) && !ids.iter().any(|v| v == id) {
            ids.push(id.to_string());
        }
    };

    let mut allow_listed: Vec<String> = Vec::new();
    for conn in snapshot
        .provider_connections
        .iter()
        .filter(|c| c.provider == provider && is_usable_connection(c))
    {
        if let Some(values) = conn
            .provider_specific_data
            .get("enabledModels")
            .and_then(Value::as_array)
        {
            for v in values.iter().filter_map(Value::as_str) {
                let id = bare_id(v);
                if !id.is_empty() && !allow_listed.iter().any(|e| e == id) {
                    allow_listed.push(id.to_string());
                }
            }
        }
    }
    if !allow_listed.is_empty() {
        for id in &allow_listed {
            push(id);
        }
        return ids;
    }

    let catalog = provider_catalog();
    let primary = catalog
        .static_alias_for_provider(provider)
        .unwrap_or(provider);
    if let Some(models) = catalog.models_for_alias(primary) {
        for m in models {
            if m.kind == "llm" {
                push(&m.id);
            }
        }
    }
    if primary != provider {
        if let Some(models) = catalog.models_for_alias(provider) {
            for m in models {
                if m.kind == "llm" {
                    push(&m.id);
                }
            }
        }
    }
    for cm in &snapshot.custom_models {
        if same_provider(&cm.provider_alias, provider)
            && (cm.r#type.is_empty() || cm.r#type == "llm" || cm.r#type == "chat")
        {
            push(cm.id.trim());
        }
    }
    ids
}

fn missing_hard_cap(candidate: &str, required: &HashSet<String>) -> bool {
    HARD_CAPS
        .iter()
        .any(|cap| required.contains(*cap) && !model_has_capability(candidate, cap))
}

fn model_has_capability(entry: &str, capability: &str) -> bool {
    let entry_lower = entry.to_lowercase();
    match capability {
        "vision" => {
            if entry_lower.starts_with("openai/o1")
                || entry_lower.starts_with("openai/o3")
                || entry_lower.starts_with("anthropic/claude")
                || entry_lower.starts_with("google/gemini")
                || entry_lower.starts_with("vertex/claude")
                || entry_lower.starts_with("vertex/gemini")
                || entry_lower.starts_with("aws/claude")
                || entry_lower.starts_with("gcp/gemini")
                || entry_lower.starts_with("custom/node-openai")
            {
                return true;
            }
            if entry_lower.contains("vision")
                || entry_lower.contains("-4o")
                || entry_lower.contains("gemini")
                || entry_lower.starts_with("oc/mimo")
            {
                return true;
            }
            false
        }
        "pdf" => {
            if entry_lower.starts_with("anthropic/claude")
                || entry_lower.starts_with("vertex/claude")
                || entry_lower.starts_with("aws/claude")
                || entry_lower.starts_with("google/gemini")
                || entry_lower.starts_with("vertex/gemini")
                || entry_lower.starts_with("gcp/gemini")
            {
                return true;
            }
            false
        }
        "audioInput" => entry_lower.starts_with("oc/mimo"),
        "videoInput" => entry_lower.starts_with("oc/mimo"),
        _ => false,
    }
}

fn circuit_blocked(circuit: &CircuitBreakerRegistry, provider: &str, connection_id: &str) -> bool {
    CIRCUIT_ENDPOINTS.iter().any(|endpoint| {
        circuit.state(&CircuitBreakerRegistry::key(
            &format!("{provider}:{connection_id}"),
            endpoint,
        )) == CircuitState::Open
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::types::PricingTable;

    fn conn(id: &str, provider: &str) -> ProviderConnection {
        ProviderConnection {
            id: id.to_string(),
            provider: provider.to_string(),
            auth_type: "api_key".to_string(),
            is_active: Some(true),
            api_key: Some("sk-test".to_string()),
            ..Default::default()
        }
    }

    fn custom(provider: &str, id: &str) -> crate::types::CustomModel {
        crate::types::CustomModel {
            provider_alias: provider.to_string(),
            id: id.to_string(),
            ..Default::default()
        }
    }

    fn pricing_table(rows: &[(&str, &str, Value)]) -> PricingTable {
        let mut pricing = PricingTable::new();
        for (provider, model, value) in rows {
            pricing
                .entry((*provider).to_string())
                .or_default()
                .insert((*model).to_string(), value.clone());
        }
        pricing
    }

    #[test]
    fn parse_preset_matches_known_forms() {
        assert_eq!(parse_preset("auto"), Some(AutoPreset::Balanced));
        assert_eq!(parse_preset("AUTO"), Some(AutoPreset::Balanced));
        assert_eq!(parse_preset("  auto  "), Some(AutoPreset::Balanced));
        assert_eq!(
            parse_preset("auto/best-coding"),
            Some(AutoPreset::BestCoding)
        );
        assert_eq!(parse_preset("auto/best-free"), Some(AutoPreset::BestFree));
        assert_eq!(parse_preset("Auto/BEST-Free"), Some(AutoPreset::BestFree));
    }

    #[test]
    fn parse_preset_rejects_non_auto() {
        assert_eq!(parse_preset("autox"), None);
        assert_eq!(parse_preset("openai/gpt-4o"), None);
        assert_eq!(parse_preset("aut"), None);
        assert_eq!(parse_preset(""), None);
        assert_eq!(parse_preset("auto/best"), None);
    }

    #[test]
    fn weights_defaults_sum_to_one() {
        for preset in [
            AutoPreset::Balanced,
            AutoPreset::BestCoding,
            AutoPreset::BestFree,
        ] {
            let w = weights_for(preset, &Settings::default());
            let sum = w.reliability + w.speed + w.intelligence + w.cost;
            assert!((sum - 1.0).abs() < 0.001, "{preset:?} sums to {sum}");
        }
        let balanced = weights_for(AutoPreset::Balanced, &Settings::default());
        assert_eq!(
            (
                balanced.reliability,
                balanced.speed,
                balanced.intelligence,
                balanced.cost
            ),
            (0.40, 0.25, 0.20, 0.15)
        );
    }

    #[test]
    fn weights_override_honored_when_valid() {
        let mut settings = Settings::default();
        settings.extra.insert(
            "autoWeights".to_string(),
            serde_json::json!({
                "balanced": {"reliability": 0.25, "speed": 0.25, "intelligence": 0.25, "cost": 0.25}
            }),
        );
        let w = weights_for(AutoPreset::Balanced, &settings);
        assert_eq!(w.reliability, 0.25);
        assert_eq!(w.cost, 0.25);
        // Other presets keep defaults.
        let coding = weights_for(AutoPreset::BestCoding, &settings);
        assert_eq!(coding.intelligence, 0.45);
    }

    #[test]
    fn weights_override_ignored_when_invalid() {
        // Bad sum.
        let mut settings = Settings::default();
        settings.extra.insert(
            "autoWeights".to_string(),
            serde_json::json!({
                "balanced": {"reliability": 0.5, "speed": 0.2, "intelligence": 0.1, "cost": 0.1}
            }),
        );
        let w = weights_for(AutoPreset::Balanced, &settings);
        assert_eq!(w.reliability, 0.40);
        // Missing keys.
        let mut settings = Settings::default();
        settings.extra.insert(
            "autoWeights".to_string(),
            serde_json::json!({
                "balanced": {"reliability": 0.5, "speed": 0.5}
            }),
        );
        let w = weights_for(AutoPreset::Balanced, &settings);
        assert_eq!(w.reliability, 0.40);
    }

    #[test]
    fn select_drops_degraded_models() {
        let snapshot = AppDb {
            provider_connections: vec![conn("autodeg-conn-1", "autodeg")],
            custom_models: vec![custom("autodeg", "m1")],
            ..AppDb::default()
        };
        crate::core::health::health_registry().record_probe(
            "autodeg-conn-1",
            "autodeg",
            Some(503),
            None,
        );
        let circuit = CircuitBreakerRegistry::default();
        let out = select_candidates(AutoPreset::Balanced, &snapshot, &HashSet::new(), &circuit);
        crate::core::health::health_registry().clear_connection("autodeg-conn-1");
        assert!(out.is_empty(), "degraded provider must be dropped: {out:?}");
    }

    #[test]
    fn select_drops_open_circuit_models() {
        let snapshot = AppDb {
            provider_connections: vec![conn("autocirc-conn-1", "autocirc")],
            custom_models: vec![custom("autocirc", "m1")],
            ..AppDb::default()
        };
        let circuit = CircuitBreakerRegistry::default();
        let key = CircuitBreakerRegistry::key(
            &format!("{}:{}", "autocirc", "autocirc-conn-1"),
            "/v1/chat/completions",
        );
        circuit.open_for(&key, Duration::from_secs(60));
        assert_eq!(circuit.state(&key), CircuitState::Open);
        let out = select_candidates(AutoPreset::Balanced, &snapshot, &HashSet::new(), &circuit);
        assert!(out.is_empty(), "open circuit must be dropped: {out:?}");
    }

    #[test]
    fn select_best_free_keeps_only_free_rows() {
        let snapshot = AppDb {
            provider_connections: vec![conn("autofree-conn-1", "autofree")],
            custom_models: vec![custom("autofree", "m-free"), custom("autofree", "m-paid")],
            pricing: pricing_table(&[
                (
                    "autofree",
                    "m-free",
                    serde_json::json!({"costModel": "free"}),
                ),
                (
                    "autofree",
                    "m-paid",
                    serde_json::json!({"input": 2.5, "output": 10.0}),
                ),
            ]),
            ..AppDb::default()
        };
        let circuit = CircuitBreakerRegistry::default();
        let out = select_candidates(AutoPreset::BestFree, &snapshot, &HashSet::new(), &circuit);
        assert_eq!(out, vec!["autofree/m-free".to_string()]);
    }

    #[test]
    fn select_ordering_is_deterministic() {
        let snapshot = AppDb {
            provider_connections: vec![conn("autoord-conn-1", "autoord")],
            custom_models: vec![custom("autoord", "m-a"), custom("autoord", "m-b")],
            pricing: pricing_table(&[
                (
                    "autoord",
                    "m-a",
                    serde_json::json!({"input": 0.0, "output": 0.0, "reliabilityScore": 0.9}),
                ),
                (
                    "autoord",
                    "m-b",
                    serde_json::json!({"input": 0.0, "output": 0.0, "reliabilityScore": 0.1}),
                ),
            ]),
            ..AppDb::default()
        };
        let circuit = CircuitBreakerRegistry::default();
        let first = select_candidates(AutoPreset::Balanced, &snapshot, &HashSet::new(), &circuit);
        let second = select_candidates(AutoPreset::Balanced, &snapshot, &HashSet::new(), &circuit);
        assert_eq!(
            first,
            vec!["autoord/m-a".to_string(), "autoord/m-b".to_string()]
        );
        assert_eq!(first, second);
    }
}
