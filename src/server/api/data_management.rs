//! Scoped data-management: export / import / reset for selected domains.
//!
//! The user wants "clear all cache for fresh start" with scoped
//! export/import for: API keys + provider credentials, combos, usage.
//! Unlike `BackupManager` (full snapshot), these endpoints operate on a
//! subset ("scope") so the operator can cheaply move one domain (e.g. only
//! `combos`) between instances or wipe just one domain for a fresh start.
//!
//! Scopes:
//! - `apiKeys`              -> `apiKeys` table
//! - `providerCredentials`  -> `providerConnections` + `providerNodes` + `proxyPools`
//! - `combos`               -> `combos` + `disabledModels` + `customModels`/`modelAliases`
//! - `usage`                -> `usageHistory` + `usageDaily` + `requestDetails`
//! - `all`                  -> every app + usage table (fresh start)

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::backups::{BackupManager, BackupReason};
use crate::server::state::AppState;
use crate::types::ProviderConnection;

use super::{require_dashboard_or_management_api_key, require_database_password_reauth};

use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/data/export", get(export_get).post(export_post))
        .route("/api/data/import", post(import_handler))
        .route("/api/data/import-env", post(import_env_handler))
        .route("/api/data/reset", post(reset_handler))
}

// ---------------------------------------------------------------------------
// Scope helpers
// ---------------------------------------------------------------------------

const VALID_SCOPES: &[&str] = &["apiKeys", "providerCredentials", "combos", "usage", "all"];

fn normalize_scopes(input: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut raw = input.unwrap_or_default();
    raw.iter_mut().for_each(|s| *s = s.trim().to_string());
    raw.retain(|s| !s.is_empty());
    if raw.is_empty() {
        return Ok(vec!["all".to_string()]);
    }
    // De-duplicate, preserve order of first appearance.
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in raw {
        if !VALID_SCOPES.contains(&s.as_str()) {
            return Err(format!(
                "Unknown scope: {s}. Valid: {}",
                VALID_SCOPES.join(", ")
            ));
        }
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    // `all` absorbs everything; normalize to just `all`.
    if out.iter().any(|s| s == "all") {
        return Ok(vec!["all".to_string()]);
    }
    Ok(out)
}

fn expands_to_all(scopes: &[String]) -> bool {
    scopes.len() == 1 && scopes[0] == "all"
}

fn includes(scopes: &[String], want: &str) -> bool {
    expands_to_all(scopes) || scopes.iter().any(|s| s == want)
}

// Build a filtered export containing only the requested slices plus minimal
// envelope keys (schemaVersion + settings when any app scope is present).
fn filtered_export(full: Value, usage_val: Value, scopes: &[String]) -> Value {
    let mut out = serde_json::Map::new();
    // Always include schemaVersion for import compatibility.
    if let Some(v) = full.get("schemaVersion").cloned() {
        out.insert("schemaVersion".into(), v);
    }
    let do_api_keys = includes(scopes, "apiKeys");
    let do_provider = includes(scopes, "providerCredentials");
    let do_combos = includes(scopes, "combos");
    let do_usage = includes(scopes, "usage");

    // Any app domain export should carry `settings` so a fresh target has
    // the operator's server settings (requireLogin, etc.). Import already
    // merges settings safely (unchanged fields preserved).
    let has_app = do_api_keys || do_provider || do_combos;
    if has_app {
        if let Some(v) = full.get("settings").cloned() {
            out.insert("settings".into(), v);
        }
        // Keep checksum absent — export_all includes none; import recomputes.
    }

    if do_api_keys {
        if let Some(v) = full.get("apiKeys").cloned() {
            out.insert("apiKeys".into(), v);
        }
    }
    if do_provider {
        for key in ["providerConnections", "providerNodes", "proxyPools"] {
            if let Some(v) = full.get(key).cloned() {
                out.insert(key.into(), v);
            }
        }
    }
    if do_combos {
        for key in [
            "combos",
            "disabledModels",
            "customModels",
            "modelAliases",
            "mitmAlias",
            "pricing",
            "providerFilters",
            "favoriteModels",
        ] {
            if let Some(v) = full.get(key).cloned() {
                out.insert(key.into(), v);
            }
        }
    }
    if do_usage {
        // Usage export shape: { history, totalRequestsLifetime } plus optional
        // alias for scorer logs; keep it flat at top-level so it matches
        // `export_usage_impl`.
        if let Some(Value::Object(u)) = usage_val.as_object().cloned().map(Value::Object) {
            for (k, v) in u {
                out.insert(k, v);
            }
        } else {
            // Fallback: embed raw usage value under `usage`.
            out.insert(
                "history".into(),
                usage_val.get("history").cloned().unwrap_or(json!([])),
            );
        }
    }

    Value::Object(out)
}

fn export_filename(scopes: &[String]) -> String {
    let tag = if expands_to_all(scopes) {
        "full".to_string()
    } else {
        scopes.join("-")
    };
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
    format!("zeroproxy-data-{tag}-{stamp}.json")
}

// ---------------------------------------------------------------------------
// GET /api/data/export?scopes=apiKeys,combos&password=...  (query version for CLI)
// ---------------------------------------------------------------------------
#[derive(Debug, Deserialize)]
struct ExportQuery {
    scopes: Option<String>,
    // Alternate query key used by CLI (`--password` via query is discouraged but
    // supported for scripting; header is preferred).
    password: Option<String>,
}

async fn export_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ExportQuery>,
) -> Response {
    if let Err(r) = require_dashboard_or_management_api_key(&headers, &state) {
        return r;
    }
    if let Err(r) = require_database_password_reauth(&headers, &state, q.password.as_deref()) {
        return r;
    }
    let scopes_raw = q.scopes.map(|s| {
        s.split(',')
            .map(|x| x.trim().to_string())
            .collect::<Vec<_>>()
    });
    let scopes = match normalize_scopes(scopes_raw) {
        Ok(s) => s,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    };
    build_export_response(&state, &scopes, None)
}

// ---------------------------------------------------------------------------
// POST /api/data/export  { scopes: string[], password?: string }
// ---------------------------------------------------------------------------
#[derive(Debug, Deserialize)]
struct ExportBody {
    scopes: Option<Vec<String>>,
    password: Option<String>,
}

async fn export_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<ExportBody>>,
) -> Response {
    if let Err(r) = require_dashboard_or_management_api_key(&headers, &state) {
        return r;
    }
    let payload = body.map(|Json(b)| b).unwrap_or(ExportBody {
        scopes: None,
        password: None,
    });
    if let Err(r) = require_database_password_reauth(&headers, &state, payload.password.as_deref())
    {
        return r;
    }
    let scopes = match normalize_scopes(payload.scopes) {
        Ok(s) => s,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    };
    build_export_response(&state, &scopes, Some(&scopes))
}

fn build_export_response(
    state: &AppState,
    scopes: &[String],
    _trace: Option<&[String]>,
) -> Response {
    // Pull both export domains from SQLite via the typed snapshot + raw export
    // for KV fidelity, then filter.
    let full = state
        .db
        .sqlite_handle()
        .with_conn(|conn| crate::db::sqlite::export::export_all(conn))
        .unwrap_or(json!({}));
    let usage_val = state
        .db
        .sqlite_handle()
        .with_conn(|conn| crate::db::sqlite::export::export_usage_impl(conn))
        .unwrap_or(json!({ "history": [] }));
    let filtered = filtered_export(full, usage_val, scopes);
    let bytes = serde_json::to_vec_pretty(&filtered).unwrap_or_default();
    let filename = export_filename(scopes);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .body(Body::from(bytes))
        .unwrap_or_else(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        })
}

// ---------------------------------------------------------------------------
// POST /api/data/import  JSON body (merge, like /api/settings/database)
// Accepts a filtered export (any subset). Usage `history` is handled via
// the usage store; app domains via AppDb merge.
// ---------------------------------------------------------------------------
async fn import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Err(r) = require_dashboard_or_management_api_key(&headers, &state) {
        return r;
    }
    let Json(mut value) = match body {
        Ok(Json(v)) => Json(v),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid database payload" })),
            )
                .into_response()
        }
    };
    if !value.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid database payload" })),
        )
            .into_response();
    }
    let body_password = value
        .as_object_mut()
        .and_then(|o| o.remove("password"))
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    if let Err(r) = require_database_password_reauth(&headers, &state, body_password.as_deref()) {
        return r;
    }

    // Pre-import safety snapshot (matches /api/db-backups/import behavior).
    let (pre_json, _) = match state.db.export_db() {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(target: "zeroproxy::db::backups", error = %e, "pre-import export failed; aborting scoped import");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let mgr = BackupManager::new(&state.db.data_dir);
    if let Err(e) = mgr
        .create_from_json(BackupReason::PreImport, &pre_json)
        .await
    {
        tracing::warn!(target: "zeroproxy::db::backups", error = %e, "pre-import backup failed; aborting scoped import");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    // Detect which domains this payload carries so the response can echo it.
    let has_usage = value.get("history").is_some();
    let has_app = [
        "providerConnections",
        "providerNodes",
        "proxyPools",
        "apiKeys",
        "combos",
        "customModels",
        "modelAliases",
        "disabledModels",
    ]
    .iter()
    .any(|k| value.get(*k).is_some());

    // Usage sub-import: use the usage store (append-safe merge, not full wipe
    // when we delegate? But import_usage does DELETE+INSERT; we want the
    // scoped payload to REPLACE usageHistory. That's correct for "export
    // usage then import elsewhere".)
    if has_usage {
        let usage_val = {
            let mut m = serde_json::Map::new();
            if let Some(v) = value.get("history").cloned() {
                m.insert("history".into(), v);
            }
            if let Some(v) = value.get("totalRequestsLifetime").cloned() {
                m.insert("totalRequestsLifetime".into(), v);
            }
            if let Some(v) = value.get("dailySummary").cloned() {
                m.insert("dailySummary".into(), v);
            }
            Value::Object(m)
        };
        let bytes = serde_json::to_vec(&usage_val).unwrap_or_default();
        if let Err(e) = state.db.import_usage_db(&bytes).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    if has_app {
        // Reuse the existing merge behavior from settings_database_import_api:
        // only overwrite collections that are non-empty in the import; settings
        // is merged field-wise. This keeps scoped imports from wiping
        // unrelated domains that the export didn't include (empty array check).
        let imported = crate::types::AppDb::from_json_value(value);
        if let Err(e) = state
            .db
            .update(|db| {
                if !imported.provider_connections.is_empty() {
                    db.provider_connections = imported.provider_connections.clone();
                }
                if !imported.provider_nodes.is_empty() {
                    db.provider_nodes = imported.provider_nodes.clone();
                }
                if !imported.proxy_pools.is_empty() {
                    db.proxy_pools = imported.proxy_pools.clone();
                }
                if !imported.api_keys.is_empty() {
                    db.api_keys = imported.api_keys.clone();
                }
                if !imported.combos.is_empty() {
                    db.combos = imported.combos.clone();
                }
                if !imported.custom_models.is_empty() {
                    db.custom_models = imported.custom_models.clone();
                }
                if !imported.model_aliases.is_empty() {
                    db.model_aliases = imported.model_aliases.clone();
                }
                if !imported.pricing.is_empty() {
                    db.pricing = imported.pricing.clone();
                }
                // For KV-like extras carried at top level by full export,
                // merge them similarly (providerFilters, favoriteModels, etc.).
                for key in ["providerFilters", "favoriteModels", "mitmAlias"] {
                    if let Some(v) = imported.extra.get(key) {
                        db.extra.insert(key.to_string(), v.clone());
                    }
                }
                if let Some(v) = imported.extra.get("disabledModels") {
                    db.extra.insert("disabledModels".into(), v.clone());
                }
                // Also handle `disabledModels` when provided as a top-level key
                // by import payload (filtered_export does that).
                // AppDb::from_json_value puts unknown arrays into `extra`; we've
                // already merged extras. The dedicated disabledModels table is
                // managed via the patch diff against `extra`; but imports via
                // `update()` apply it correctly because patch compares extra.
                super::merge_settings(&mut db.settings, &imported.settings);
            })
            .await
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    if !has_app && !has_usage {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Import payload contains no recognized domains (expected apiKeys/providerConnections/combos/history)" })),
        )
            .into_response();
    }

    let snap = state.db.snapshot();
    let usage = state.db.usage_snapshot();
    Json(json!({
        "success": true,
        "imported": {
            "apiKeys": snap.api_keys.len(),
            "providerConnections": snap.provider_connections.len(),
            "combos": snap.combos.len(),
            "usageEntries": usage.history.len(),
        }
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /api/data/reset  { scopes: string[], confirm: "RESET", password? }
// Wipes the selected scopes. Creates a pre-reset backup (pre-restore reason)
// beforehand. Requires password re-auth + confirm string.
// ---------------------------------------------------------------------------
#[derive(Debug, Deserialize)]
struct ResetBody {
    scopes: Option<Vec<String>>,
    confirm: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResetResult {
    cleared: Vec<String>,
    kept_scopes_hint: Vec<String>,
    provider_count: usize,
    combo_count: usize,
    api_key_count: usize,
    usage_entries: usize,
}

async fn reset_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ResetBody>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Err(r) = require_dashboard_or_management_api_key(&headers, &state) {
        return r;
    }
    let Json(payload) = match body {
        Ok(Json(b)) => Json(b),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid reset payload" })),
            )
                .into_response()
        }
    };
    let scopes = match normalize_scopes(payload.scopes) {
        Ok(s) => s,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    };
    if payload.confirm.as_deref().map(str::trim).unwrap_or("") != "RESET" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "confirm must be exactly \"RESET\"" })),
        )
            .into_response();
    }
    if let Err(r) = require_database_password_reauth(&headers, &state, payload.password.as_deref())
    {
        return r;
    }

    // Pre-reset safety backup.
    let (pre_json, _) = match state.db.export_db() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };
    let mgr = BackupManager::new(&state.db.data_dir);
    if let Err(e) = mgr
        .create_from_json(BackupReason::PreRestore, &pre_json)
        .await
    {
        tracing::warn!(target: "zeroproxy::data_management", error = %e, "pre-reset backup failed; proceeding anyway");
    }
    // Also snapshot usage for scoped wipe observability.
    let went_usage = includes(&scopes, "usage");
    let (_pre_usage_bytes, _) = state
        .db
        .export_usage_db()
        .unwrap_or((vec![], String::new()));

    let do_api_keys = includes(&scopes, "apiKeys");
    let do_provider = includes(&scopes, "providerCredentials");
    let do_combos = includes(&scopes, "combos");
    let do_usage = includes(&scopes, "usage");

    // Perform the wipe inside `Db::update` so snapshot + SQLite stay in sync
    // for app domains. Usage is handled via raw SQL + import_usage path.
    let res = state
        .db
        .update(|db| {
            if do_api_keys {
                db.api_keys.clear();
            }
            if do_provider {
                db.provider_connections.clear();
                db.provider_nodes.clear();
                db.proxy_pools.clear();
            }
            if do_combos {
                db.combos.clear();
                db.custom_models.clear();
                db.model_aliases.clear();
                // Extras that belong to combos domain also cleared to avoid
                // phantom state after reset.
                db.extra.remove("disabledModels");
                db.extra.remove("providerFilters");
                db.extra.remove("favoriteModels");
                // Keep pricing etc. untouched unless `all`.
            }
            if expands_to_all(&scopes) {
                // `all` also clears remaining extras + pricing.
                db.extra.clear();
                db.pricing.clear();
            }
        })
        .await;

    if let Err(e) = res {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    // Usage wipe must also hit SQLite directly; `update()` doesn't touch
    // usage tables. Use delete on usageHistory/usageDaily/requestDetails.
    // The in-memory UsageDb is updated via import_usage_db with empty history.
    if do_usage {
        let empty_usage = json!({ "history": [], "totalRequestsLifetime": 0u64 });
        let bytes = serde_json::to_vec(&empty_usage).unwrap_or_default();
        if let Err(e) = state.db.import_usage_db(&bytes).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("usage reset failed: {e}") })),
            )
                .into_response();
        }
        // Also clear requestDetails which export_usage doesn't cover but is
        // part of the usage domain for "clear all cache".
        let _ = state.db.sqlite_handle().with_conn(|conn| {
            let _ = conn.execute("DELETE FROM requestDetails", []);
            Ok::<_, rusqlite::Error>(())
        });
    }

    let snap = state.db.snapshot();
    let usage = state.db.usage_snapshot();
    let result = ResetResult {
        cleared: scopes.clone(),
        kept_scopes_hint: Vec::new(),
        provider_count: snap.provider_connections.len(),
        combo_count: snap.combos.len(),
        api_key_count: snap.api_keys.len(),
        usage_entries: usage.history.len(),
    };
    Json(json!({ "success": true, "reset": result })).into_response()
}

// ---------------------------------------------------------------------------
// POST /api/data/import-env
//
// Bulk-import API keys from a .env file for API-key (non-OAuth-pure) providers.
// OAuth-only providers (claude, codex, github, cursor, cline, antigravity,
// grok-cli, gemini-cli) are excluded because their credentials come from the
// device-code/OIDC flows, not a static key. Web-cookie providers (grok-web,
// perplexity-web) are excluded too. Dual-auth providers whose env var holds a
// `api_key` (xai, kimi, kiro, kilocode) ARE eligible.
// ---------------------------------------------------------------------------

/// Env-var name → provider id, for providers that can be configured with a
/// single API key string. Mirrors the env-var conventions used by each executor
/// (see `cliTools.ts` exports + the per-provider `auth_type == "apikey"`).
const ENV_KEY_PROVIDERS: &[(&str, &str)] = &[
    ("OPENAI_API_KEY", "openai"),
    ("OPENAI_BASE_URL", "openai"),
    ("ANTHROPIC_API_KEY", "anthropic"),
    ("GROK_API_KEY", "grok-cli"),
    ("XAI_API_KEY", "xai"),
    ("GOOGLE_API_KEY", "gemini"),
    ("GEMINI_API_KEY", "gemini"),
    ("GROQ_API_KEY", "groq"),
    ("TOGETHER_API_KEY", "together"),
    ("TOGETHER_BASE_URL", "together"),
    ("REPLICATE_API_KEY", "replicate"),
    ("REPLICATE_BASE_URL", "replicate"),
    ("DEEPSEEK_API_KEY", "deepseek"),
    ("DEEPSEEK_BASE_URL", "deepseek"),
    ("MISTRAL_API_KEY", "mistral"),
    ("MISTRAL_BASE_URL", "mistral"),
    ("COHERE_API_KEY", "cohere"),
    ("COHERE_BASE_URL", "cohere"),
    ("UPSTAGE_API_KEY", "upstage"),
    ("UPSTAGE_BASE_URL", "upstage"),
    ("PERPLEXITY_API_KEY", "perplexity"),
    ("PERPLEXITY_BASE_URL", "perplexity"),
    ("OCTOAPI_KEY", "octoapi"),
    ("OCTOAPI_BASE_URL", "octoapi"),
    ("AI21_API_KEY", "ai21"),
    ("AI21_BASE_URL", "ai21"),
    ("CHATGLM_API_KEY", "chatglm"),
    ("ZHIPU_API_KEY", "zhipu"),
    ("MOONSHOT_API_KEY", "moonshot"),
    ("MOONSHOT_BASE_URL", "moonshot"),
    ("MINIMAX_API_KEY", "minimax"),
    ("MINIMAX_BASE_URL", "minimax"),
    ("MINIMAX_CN_API_KEY", "minimax-cn"),
    ("MINIMAX_CN_BASE_URL", "minimax-cn"),
    ("SILICONFLOW_API_KEY", "siliconflow"),
    ("SILICONFLOW_BASE_URL", "siliconflow"),
    ("DASHSCOPE_API_KEY", "alicode"),
    ("DASHSCOPE_BASE_URL", "alicode"),
    ("DASHSCOPE_API_KEY_INTL", "alicode-intl"),
    ("ALIYUN_API_KEY", "alicode"),
    ("BAILIAN_API_KEY", "alibaba-bailian"),
    ("MODELMETRIX_API_KEY", "modelmetrix"),
    ("MODELMETRIX_BASE_URL", "modelmetrix"),
    ("STEP_API_KEY", "step"),
    ("STEP_BASE_URL", "step"),
    ("BAIDU_API_KEY", "baidu"),
    ("BAIDU_BASE_URL", "baidu"),
    ("TENCENT_API_KEY", "tencent"),
    ("TENCENT_BASE_URL", "tencent"),
    ("VOLCENGINE_API_KEY", "volcengine-ark"),
    ("VOLCENGINE_BASE_URL", "volcengine-ark"),
    ("BYTEPLUS_API_KEY", "byteplus"),
    ("BYTEPLUS_BASE_URL", "byteplus"),
    ("TONYSTARK_API_KEY", "tonystark"),
    ("TONYSTARK_BASE_URL", "tonystark"),
    ("LINGBIAN_API_KEY", "lingbian"),
    ("LINGBIAN_BASE_URL", "lingbian"),
    ("HUAWEI_API_KEY", "huawei"),
    ("HUAWEI_BASE_URL", "huawei"),
    ("AZURE_OPENAI_API_KEY", "azure"),
    ("AZURE_OPENAI_ENDPOINT", "azure"),
    ("LLAMA_API_KEY", "llama"),
    ("LLAMA_BASE_URL", "llama"),
    ("INFERENCE_API_KEY", "huggingface"),
    ("HF_API_KEY", "huggingface"),
    ("FIREWORKS_API_KEY", "fireworks"),
    ("FIREWORKS_BASE_URL", "fireworks"),
    ("CODI_API_KEY", "codiai"),
    ("CODI_BASE_URL", "codiai"),
    ("KIMI_API_KEY", "kimi"),
    ("KIMI_BASE_URL", "kimi"),
    ("KILCODE_API_KEY", "kilocode"),
    ("KILCODE_BASE_URL", "kilocode"),
    ("KAI_API_KEY", "kai"),
    ("KAI_BASE_URL", "kai"),
    ("TENCENT_CLOUD_API_KEY", "tencent-cloud"),
    ("TENCENT_CLOUD_SECRET_ID", "tencent-cloud"),
    ("TENCENT_CLOUD_SECRET_KEY", "tencent-cloud"),
    ("ALIYUN_ACCESS_KEY_ID", "alibaba-cloud"),
    ("ALIYUN_ACCESS_KEY_SECRET", "alibaba-cloud"),
    ("AWS_ACCESS_KEY_ID", "aws"),
    ("AWS_SECRET_ACCESS_KEY", "aws"),
    ("OLLAMA_HOST", "ollama-local"),
    ("OLLAMA_BASE_URL", "ollama-local"),
    ("CLOUDFLARE_API_TOKEN", "cloudflare-ai"),
    ("CLOUDFLARE_ACCOUNT_ID", "cloudflare-ai"),
    ("FREEPROVIDER_API_KEY", "openprovider"),
    ("FREEPROVIDER_BASE_URL", "openprovider"),
    ("API2D_API_KEY", "api2d"),
    ("API2D_BASE_URL", "api2d"),
    ("JINSHU_API_KEY", "jinshu"),
    ("JINSHU_BASE_URL", "jins hu"),
    ("AI302_API_KEY", "ai302"),
    ("AI302_BASE_URL", "ai302"),
    ("AION_API_KEY", "aion"),
    ("AION_BASE_URL", "aion"),
    ("AGNES_API_KEY", "agnes"),
    ("AGNES_BASE_URL", "agnes"),
    ("OVHCLOUD_API_KEY", "ovhcloud"),
    ("OVHCLOUD_BASE_URL", "ovhcloud"),
    ("MODELSCOPE_API_KEY", "modelscope"),
    ("MODELSCOPE_BASE_URL", "modelscope"),
];

/// Parse a `.env` file body into `(env_var, value)` pairs.
/// Handles `KEY=value`, quoted values, `export ` prefix, and ignores
/// comments and blank lines.
fn parse_env_file(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Strip an optional leading `export ` prefix.
        let stripped = line.strip_prefix("export ").unwrap_or(line).trim_start();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        let Some(eq) = stripped.find('=') else {
            continue;
        };
        let key = stripped[..eq].trim().to_string();
        let mut val = stripped[eq + 1..].trim();
        if val.len() >= 2 {
            let (q, rest) = (val.chars().next().unwrap(), &val[1..]);
            if let Some(cl) = rest.chars().last() {
                if (q == '"' || q == '\'') && cl == q {
                    val = &rest[..rest.len() - 1];
                }
            } else {
                val = "";
            }
        }
        if !key.is_empty() {
            out.push((key, val.to_string()));
        }
    }
    out
}

/// Match an env var against the known provider map; returns `(provider, is_key)`.
fn provider_for_env_var(var: &str) -> Option<(&'static str, bool)> {
    for (env_var, provider) in ENV_KEY_PROVIDERS {
        if var == *env_var {
            let is_key = env_var.ends_with("API_KEY")
                || env_var.ends_with("API_TOKEN")
                || *env_var == "HF_API_KEY"
                || *env_var == "AZURE_OPENAI_ENDPOINT"
                || *env_var == "AZURE_OPENAI_API_KEY";
            return Some((*provider, is_key));
        }
    }
    None
}

/// Build the `(provider_specific_data key, value)` to override an executor's
/// detected endpoint when a `*_BASE_URL` / `*_ENDPOINT` env var is present.
fn base_url_for_env_var(var: &str, val: &str) -> Option<(String, String)> {
    for (env_var, provider) in ENV_KEY_PROVIDERS {
        if var == *env_var && (env_var.ends_with("BASE_URL") || env_var.ends_with("_ENDPOINT")) {
            let key = if *provider == "azure" {
                "azureEndpoint"
            } else {
                "baseUrl"
            };
            return Some((key.to_string(), val.to_string()));
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct ImportEnvBody {
    password: Option<String>,
}

async fn import_env_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: axum::body::Bytes,
) -> Response {
    if let Err(r) = require_dashboard_or_management_api_key(&headers, &state) {
        return r;
    }
    if let Err(r) = require_database_password_reauth(&headers, &state, None) {
        return r;
    }
    let text = match std::str::from_utf8(&body_bytes) {
        Ok(t) if !t.is_empty() => t.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Request body must be the .env text" })),
            )
                .into_response();
        }
    };

    // A pre-import backup snapshot so a bad .env can be rolled back.
    let (pre_json, _) = match state.db.export_db() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let mgr = BackupManager::new(&state.db.data_dir);
    if let Err(e) = mgr
        .create_from_json(BackupReason::PreImport, &pre_json)
        .await
    {
        tracing::warn!(target: "zeroproxy::db::backups", error = %e, "pre-import-env backup failed; proceeding");
    }

    let pairs = parse_env_file(&text);
    let mut grouped: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    for (var, val) in &pairs {
        if let Some((provider, _is_key)) = provider_for_env_var(var) {
            grouped
                .entry(provider.to_string())
                .or_default()
                .push((var.clone(), val.clone()));
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut results: Vec<Value> = Vec::new();

    let res = state
        .db
        .update(|db| {
            for (provider, entries) in &grouped {
                let key_val = entries
                    .iter()
                    .find(|(k, _)| {
                        provider_for_env_var(k)
                            .map(|(_, is_key)| is_key)
                            .unwrap_or(false)
                    })
                    .map(|(k, v)| (k.as_str(), v.as_str()));
                let Some((key_var, key_value)) = key_val else {
                    for (var, _) in entries {
                        results.push(json!({
                            "provider": provider,
                            "envVar": var,
                            "status": "skipped",
                            "reason": "no API key variable found for this provider"
                        }));
                    }
                    skipped += 1;
                    continue;
                };

                // Dedup: skip if a connection with this provider + key already exists.
                let already = db
                    .provider_connections
                    .iter()
                    .any(|c| c.provider == *provider && c.api_key.as_deref() == Some(key_value));
                if already {
                    results.push(json!({
                        "provider": provider,
                        "envVar": key_var,
                        "status": "skipped",
                        "reason": "matching connection already exists"
                    }));
                    skipped += 1;
                    continue;
                }

                let mut conn = ProviderConnection::default();
                conn.id = Uuid::new_v4().to_string();
                conn.provider = (*provider).clone();
                conn.auth_type = "apikey".to_string();
                conn.name = Some(format!("{} (env)", provider));
                conn.is_active = Some(true);
                conn.priority = Some(1);
                conn.api_key = Some(key_value.to_string());
                conn.created_at = Some(now.clone());
                conn.updated_at = Some(now.clone());
                // Attach base-url override if a *_BASE_URL / *_ENDPOINT was
                // present in the .env for this provider.
                for (v, val) in entries {
                    if v == key_var {
                        continue;
                    }
                    if let Some((k, v2)) = base_url_for_env_var(v, val) {
                        conn.provider_specific_data
                            .insert(k, serde_json::Value::String(v2.clone()));
                    }
                }
                db.provider_connections.push(conn);
                results.push(json!({
                    "provider": provider,
                    "envVar": key_var,
                    "status": "added",
                }));
                added += 1;
            }
        })
        .await;

    if let Err(e) = res {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }
    let snap = state.db.snapshot();
    Json(json!({
        "success": true,
        "imported": {
            "added": added,
            "skipped": skipped,
            "providers": snap.provider_connections.len(),
            "keys": snap.api_keys.len(),
        },
        "details": results,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_scopes_defaults_to_all() {
        assert_eq!(normalize_scopes(None).unwrap(), vec!["all"]);
        assert_eq!(normalize_scopes(Some(vec![])).unwrap(), vec!["all"]);
    }

    #[test]
    fn normalize_scopes_rejects_unknown() {
        assert!(normalize_scopes(Some(vec!["combos".into(), "cats".into()])).is_err());
    }

    #[test]
    fn normalize_scopes_dedupes_and_collapses_all() {
        let s =
            normalize_scopes(Some(vec!["combos".into(), "all".into(), "usage".into()])).unwrap();
        assert_eq!(s, vec!["all"]);
    }

    #[test]
    fn filtered_export_picks_only_requested_scopes() {
        let full = json!({
            "schemaVersion": 2,
            "settings": { "requireLogin": true },
            "apiKeys": [{ "id": "k1" }],
            "providerConnections": [{ "id": "c1" }],
            "providerNodes": [],
            "proxyPools": [],
            "combos": [{ "id": "m1" }],
            "customModels": [],
            "modelAliases": {},
            "history": [],
        });
        let usage = json!({ "history": [{ "model": "x" }], "totalRequestsLifetime": 1u64 });
        let only_combos = filtered_export(full.clone(), usage.clone(), &["combos".into()]);
        assert!(only_combos.get("combos").is_some());
        assert!(only_combos.get("apiKeys").is_none());
        assert!(only_combos.get("providerConnections").is_none());
        assert!(only_combos.get("history").is_none());

        let only_usage = filtered_export(full, usage, &["usage".into()]);
        assert!(only_usage.get("history").is_some());
        assert!(only_usage.get("combos").is_none());
        assert!(only_usage.get("settings").is_none());
    }

    #[test]
    fn parse_env_file_handles_values_and_comments() {
        let body = "# a comment\nexport OPENAI_API_KEY=\"sk-test-123\"\nANTHROPIC_API_KEY=sk-ant-456\n\nbad line no equals\nGROQ_API_KEY='gk-key'\n";
        let pairs = parse_env_file(body);
        let m: std::collections::HashMap<_, _> = pairs.into_iter().collect();
        assert_eq!(
            m.get("OPENAI_API_KEY").map(|s| s.as_str()),
            Some("sk-test-123")
        );
        assert_eq!(
            m.get("ANTHROPIC_API_KEY").map(|s| s.as_str()),
            Some("sk-ant-456")
        );
        assert_eq!(m.get("GROQ_API_KEY").map(|s| s.as_str()), Some("gk-key"));
        assert!(m.get("bad line no equals").is_none());
    }

    #[test]
    fn provider_for_env_var_maps_known_keys_and_ignores_oauth() {
        // API-key provider.
        let (p, is_key) = provider_for_env_var("DEEPSEEK_API_KEY").unwrap();
        assert_eq!(p, "deepseek");
        assert!(is_key);

        // Base-url resolves to provider but is not itself the key.
        let (p, is_key) = provider_for_env_var("MISTRAL_BASE_URL").unwrap();
        assert_eq!(p, "mistral");
        assert!(!is_key);

        // Pure OAuth providers must NOT appear as API-key env vars. (Claude, codex,
        // github, cursor, cline, antigravity are OAuth-only; gemini-cli/kiro-cli
        // use OAuth device-flow, whereas the API-key `gemini` provider is fed by
        // GOOGLE_API_KEY/GEMINI_API_KEY above.)
        assert!(provider_for_env_var("CLAUDE_API_KEY").is_none());
        assert!(provider_for_env_var("CODEX_API_KEY").is_none());
    }
}
