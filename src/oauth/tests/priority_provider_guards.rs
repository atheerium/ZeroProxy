//! Guards for the providers the maintainer uses every hour.
//!
//! The maintainer's priority set (see AGENTS.md) is: **NVIDIA, OpenRouter, Kilo,
//! OpenCode, Kiro, Gemini/Antigravity, AgentRouter**. These are the providers a
//! broken change actually costs them, so they get pinned assertions rather than
//! incidental coverage.
//!
//! The central bug these exist to prevent is the Kiro "Invalid client provided"
//! class (AGENTS.md Trap 16): a provider config carried an **empty `client_id`**,
//! and the request-dispatch code substituted a literal placeholder
//! (`"zeroproxy"`) for it, which the upstream then rejected with an opaque 401.
//! The placeholder was never an AWS/Kiro problem — it was a locally invented
//! string. A blanket "no config may have an empty `client_id`" rule would be
//! wrong, because device-code providers legitimately register a client per
//! connect (`kiro()` registers via `kiro_registration_body()`), so the correct
//! invariant is narrower and is stated in
//! `authorization_code_configs_have_a_usable_client_id_and_token_url`.
//!
//! A guard RED must compile: adding a duplicate symbol to a module that already
//! imports it fails with `E0255` and the test never runs. That mistake was made
//! once already (see the `HARD_CAPS` guard), so these tests mutate behaviour via
//! data, never via redefinition.

use super::super::providers::{get_config, OAuthProviderConfig};

/// Providers whose flow never reads `client_id`/`token_url` off the config,
/// because the device-code path mints a client per connect instead. This mirrors
/// `is_device_code_provider` in `src/server/api/oauth.rs`; the two lists must
/// stay in step, and `device_code_exemption_list_matches_the_dispatcher` fails
/// if they drift.
const DEVICE_CODE_ONLY: &[&str] = &[
    "github",
    "kiro",
    "kimi",
    "kimi-coding",
    "kilocode",
    "codebuddy",
    "codebuddy-cn",
    "codebuddy-intl",
    "qoder",
    "grok-cli",
    "qwen",
];

/// Providers that authenticate through a bespoke flow and therefore never read
/// `client_id`/`token_url` off the config. Each was *found by this guard
/// failing* and then confirmed against the comment at its definition site.
///
/// `kimchi` and `cursor` carry the literal `"zeroproxy"` in `client_id` — the
/// same placeholder that caused the Kiro 401. Harmless only because these flows
/// never read the field, which is why the exemption is asserted, not assumed.
const BESPOKE_FLOW: &[(&str, &str)] = &[
    (
        "zed",
        "RSA-2048 keypair callback; no OAuth endpoints at all",
    ),
    ("kimchi", "browser_token flow; user pastes a token"),
    ("cursor", "import-token flow via cursor_import module"),
];

/// Every provider id reachable through `providers::get_config`. Pinned so that
/// adding a provider without adding it to a guard fails here rather than
/// silently escaping review.
const ALL_PROVIDERS: &[&str] = &[
    "claude",
    "codex",
    "github",
    "kiro",
    "qwen",
    "iflow",
    "kimi",
    "kimi-coding",
    "kilocode",
    "cline",
    "clinepass",
    "zed",
    "gitlab",
    "codebuddy",
    "openai-native",
    "xai",
    "gemini-cli",
    "qoder",
    "kimchi",
    "cursor",
    "antigravity",
    "codebuddy-cn",
    "codebuddy-intl",
];

/// The priority providers that authenticate through `providers::get_config`
/// (i.e. OAuth). NVIDIA, OpenRouter, OpenCode and AgentRouter are API-key
/// providers and have no entry here; they are covered by the catalog guards
/// instead.
const PRIORITY_OAUTH_PROVIDERS: &[&str] = &["kiro", "kilocode", "antigravity"];

fn config(provider: &str) -> OAuthProviderConfig {
    get_config(provider).unwrap_or_else(|| panic!("{provider} must resolve via get_config"))
}

/// The core invariant, and the one that would have caught the Kiro saga.
///
/// For any provider that is neither device-code-only nor bespoke-flow, both
/// `client_id` and `token_url` must be non-empty. An empty value here is what
/// the request dispatcher used to replace with a fabricated placeholder.
#[test]
fn authorization_code_configs_have_a_usable_client_id_and_token_url() {
    let mut offenders = Vec::new();
    for &provider in ALL_PROVIDERS {
        if DEVICE_CODE_ONLY.contains(&provider) || BESPOKE_FLOW.iter().any(|(p, _)| *p == provider)
        {
            continue;
        }
        let cfg = config(provider);
        if cfg.client_id.trim().is_empty() {
            offenders.push(format!("{provider}: empty client_id"));
        }
        if cfg.token_url.trim().is_empty() {
            offenders.push(format!("{provider}: empty token_url"));
        }
    }
    assert!(
        offenders.is_empty(),
        "non-device-code providers must carry a real client_id and token_url; an empty \
         value is what the dispatcher replaces with a fabricated placeholder (Trap 16): {offenders:?}"
    );
}

/// Entries in `is_device_code_provider` that have no `providers::get_config`
/// entry — a stale arm found by `every_exemption_still_resolves_and_carries_a_reason`.
/// `grok-cli` appears in the dispatcher's device-code list and as an xai scope
/// string (`"grok-cli:access"`), but has no config. Pinned here so that adding a
/// real grok-cli config surfaces as a test failure asking for this to be updated,
/// rather than leaving the inconsistency invisible.
const STALE_DISPATCHER_ENTRIES: &[&str] = &["grok-cli"];

/// Every exemption names a provider that exists, and every bespoke provider
/// still resolves. Keeps the two allowlists from accumulating providers that were
/// renamed or removed.
#[test]
fn every_exemption_still_resolves_and_carries_a_reason() {
    for &(provider, reason) in BESPOKE_FLOW {
        assert!(
            !reason.trim().is_empty(),
            "{provider} is exempt with no stated reason"
        );
        assert!(
            get_config(provider).is_some(),
            "exempt provider {provider} no longer resolves"
        );
    }
    for &provider in DEVICE_CODE_ONLY {
        if STALE_DISPATCHER_ENTRIES.contains(&provider) {
            assert!(
                get_config(provider).is_none(),
                "{provider} now has a get_config entry; drop it from STALE_DISPATCHER_ENTRIES \
                 and confirm which flow it actually uses"
            );
            continue;
        }
        assert!(
            get_config(provider).is_some(),
            "exempt provider {provider} no longer resolves"
        );
    }
}

/// Every provider resolves, and `cfg.id` agrees with the key used to look it up.
/// A mismatch means some dispatch arm compares against a different spelling than
/// the one the config declares.
#[test]
fn every_listed_provider_resolves_and_its_id_matches_its_key() {
    for &provider in ALL_PROVIDERS {
        let cfg = config(provider);
        assert_eq!(
            cfg.id, provider,
            "get_config({provider}) returned id {}",
            cfg.id
        );
    }
}

/// Guards against the exemption list drifting away from the real dispatcher.
/// `is_device_code_provider` is private to `src/server/api/oauth.rs`, so this
/// pins the *duplicated* list and fails if a provider is exempted from the
/// client_id rule without also being exempted in the dispatcher — which is the
/// state in which the placeholder substitution becomes reachable again.
#[test]
fn device_code_exemption_list_matches_the_dispatcher() {
    // The dispatcher's list, transcribed. If oauth.rs changes, this test is the
    // reminder to update both.
    let dispatcher: &[&str] = &[
        "github",
        "kiro",
        "kimi",
        "kimi-coding",
        "kilocode",
        "codebuddy",
        "codebuddy-cn",
        "codebuddy-intl",
        "qoder",
        "grok-cli",
        "qwen",
    ];
    let mut a: Vec<&str> = DEVICE_CODE_ONLY.to_vec();
    let mut b: Vec<&str> = dispatcher.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(
        a, b,
        "DEVICE_CODE_ONLY drifted from is_device_code_provider"
    );
}

/// The priority OAuth providers keep their pinned, working configuration.
///
/// These are the exact values verified live against AWS/Google on 2026-09-28.
/// Pinning them means a well-meaning "cleanup" that empties a field (the exact
/// Kiro regression) fails here instead of in the dashboard.
#[test]
fn priority_oauth_providers_keep_their_pinned_configuration() {
    let kiro = config("kiro");
    assert_eq!(
        kiro.client_id, "",
        "kiro registers a client per connect; see Trap 16"
    );
    assert!(
        kiro.authorize_url.starts_with("https://oidc."),
        "kiro authorize_url must stay region-aware AWS sso-oidc, got {}",
        kiro.authorize_url
    );
    assert_eq!(
        kiro.scopes.len(),
        3,
        "kiro must keep its 3 codewhisperer scopes"
    );
    assert!(
        kiro.extra_params.is_empty(),
        "kiro extra_params must stay empty; the device flow uses kiro_registration_body() \
         and a snake_case leftover here is the pre-Trap-16 bug bait"
    );

    let kilo = config("kilocode");
    assert!(
        kilo.client_id.starts_with("zeroproxy"),
        "kilocode uses a fixed public client id"
    );
    assert!(
        kilo.authorize_url.contains("api.kilo.ai"),
        "kilocode authorize_url regressed: {}",
        kilo.authorize_url
    );
    assert!(
        kilo.extra_params.iter().any(|(k, _)| *k == "poll_url_base"),
        "kilocode device poll needs poll_url_base in extra_params"
    );

    let antigravity = config("antigravity");
    assert!(
        antigravity
            .client_id
            .ends_with("apps.googleusercontent.com"),
        "antigravity must keep its real Google client id, got {}",
        antigravity.client_id
    );
    assert_eq!(antigravity.refresh_lead_ms, 5 * 60 * 1000);
    assert!(antigravity.scopes.len() >= 5);
}

/// The priority list is not allowed to shrink unnoticed. If a provider is
/// dropped from `PRIORITY_OAUTH_PROVIDERS` this fails, so removal has to be a
/// deliberate edit to a test rather than a quiet deletion.
#[test]
fn the_priority_oauth_list_is_not_silently_emptied() {
    for &provider in PRIORITY_OAUTH_PROVIDERS {
        assert!(
            get_config(provider).is_some(),
            "priority provider {provider} no longer resolves"
        );
    }
    assert!(
        PRIORITY_OAUTH_PROVIDERS.contains(&"kiro")
            && PRIORITY_OAUTH_PROVIDERS.contains(&"antigravity"),
        "the priority list must keep kiro and antigravity"
    );
}
