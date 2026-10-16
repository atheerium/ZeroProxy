use axum::extract::State;
use axum::{
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::server::state::AppState;
use crate::types::CustomModel;

fn require_management_access(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    super::require_dashboard_or_management_api_key(headers, state)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/models/custom",
            get(list_custom_models)
                .post(create_custom_model)
                .delete(delete_custom_model_by_query),
        )
        .route(
            "/api/models/custom/{id}",
            get(get_custom_model)
                .put(update_custom_model)
                .delete(delete_custom_model),
        )
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomModelRequest {
    pub provider_alias: String,
    pub id: String,
    #[serde(default = "default_model_type", alias = "type")]
    pub r#type: String,
    pub name: Option<String>,
}

fn default_model_type() -> String {
    "llm".to_string()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomModelRequest {
    pub provider_alias: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCustomModelQuery {
    pub provider_alias: Option<String>,
    pub id: Option<String>,
    #[serde(default = "default_model_type")]
    pub r#type: String,
}

async fn list_custom_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    let snapshot = state.db.snapshot();
    Json(json!({ "models": snapshot.custom_models.clone() })).into_response()
}

async fn get_custom_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    let snapshot = state.db.snapshot();
    let model = snapshot.custom_models.iter().find(|m| m.id == id).cloned();
    Json(model).into_response()
}

async fn create_custom_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateCustomModelRequest>,
) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    if req.provider_alias.is_empty() || req.id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "providerAlias and id required" })),
        )
            .into_response();
    }

    let exists_before = state.db.snapshot().custom_models.iter().any(|model| {
        model.provider_alias == req.provider_alias
            && model.id == req.id
            && model.r#type == req.r#type
    });

    let custom_model = CustomModel {
        provider_alias: req.provider_alias.clone(),
        id: req.id.clone(),
        r#type: req.r#type.clone(),
        name: req.name.clone(),
        extra: Default::default(),
        ..Default::default()
    };

    let result = state
        .db
        .update(move |db| {
            let exists = db.custom_models.iter().any(|model| {
                model.provider_alias == req.provider_alias
                    && model.id == req.id
                    && model.r#type == req.r#type
            });
            if !exists {
                db.custom_models.push(custom_model);
            }
        })
        .await;

    match result {
        Ok(_) => Json(json!({ "success": true, "added": !exists_before })).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to add custom model" })),
        )
            .into_response(),
    }
}

async fn update_custom_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<UpdateCustomModelRequest>,
) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    let result = state
        .db
        .update(|db| {
            if let Some(model) = db.custom_models.iter_mut().find(|m| m.id == id) {
                if let Some(provider_alias) = req.provider_alias {
                    model.provider_alias = provider_alias;
                }
                if let Some(name) = req.name {
                    model.name = Some(name);
                }
            }
        })
        .await;

    match result {
        Ok(_) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            Json(serde_json::json!({ "success": false, "error": e.to_string() })).into_response()
        }
    }
}

async fn delete_custom_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    // Rows are keyed `{provider_alias}/{id}` (see `patch.rs::custom_models_map`),
    // so the path may carry either the composite key or a bare model id. Matching
    // only on `m.id` made every composite-keyed request a silent no-op that still
    // answered `success: true`.
    let removed = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&removed);
    let not_found = format!("Custom model not found: {id}");
    let result = state
        .db
        .update(move |db| {
            let before = db.custom_models.len();
            db.custom_models.retain(|m| !custom_model_matches(&id, m));
            counter.store(before - db.custom_models.len(), Ordering::SeqCst);
        })
        .await;

    match result {
        Ok(_) if removed.load(Ordering::SeqCst) > 0 => {
            Json(serde_json::json!({ "success": true })).into_response()
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": not_found })),
        )
            .into_response(),
        Err(e) => {
            Json(serde_json::json!({ "success": false, "error": e.to_string() })).into_response()
        }
    }
}

fn custom_model_matches(identifier: &str, model: &CustomModel) -> bool {
    if identifier.contains('/') {
        return format!("{}/{}", model.provider_alias, model.id) == identifier;
    }
    model.id == identifier
}

async fn delete_custom_model_by_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<DeleteCustomModelQuery>,
) -> Response {
    if let Err(response) = require_management_access(&headers, &state) {
        return response;
    }

    let provider_alias = query.provider_alias.unwrap_or_default();
    let id = query.id.unwrap_or_default();
    if provider_alias.is_empty() || id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "providerAlias and id required" })),
        )
            .into_response();
    }

    let model_type = query.r#type;
    let result = state
        .db
        .update(move |db| {
            db.custom_models.retain(|model| {
                !(model.provider_alias == provider_alias
                    && model.id == id
                    && model.r#type == model_type)
            });
        })
        .await;

    match result {
        Ok(_) => Json(json!({ "success": true })).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to delete custom model" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::custom_model_matches;
    use crate::types::CustomModel;

    fn model(alias: &str, id: &str) -> CustomModel {
        CustomModel {
            provider_alias: alias.to_string(),
            id: id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn composite_key_matches_only_its_own_alias_and_id() {
        let ar_flash = model("ar", "deepseek-v4-flash");
        let trk_flash = model("trk", "deepseek-v4-flash");

        assert!(custom_model_matches("ar/deepseek-v4-flash", &ar_flash));
        assert!(!custom_model_matches("trk/deepseek-v4-flash", &ar_flash));
        assert!(custom_model_matches("trk/deepseek-v4-flash", &trk_flash));
    }

    #[test]
    fn bare_id_still_matches_by_model_id() {
        let ar_flash = model("ar", "deepseek-v4-flash");

        assert!(custom_model_matches("deepseek-v4-flash", &ar_flash));
        assert!(!custom_model_matches("deepseek-v3", &ar_flash));
    }

    #[test]
    fn nested_model_id_is_not_split_on_its_slash() {
        let nested = model("cfp", "deepseek-ai/deepseek-v4-flash-0731");

        assert!(custom_model_matches(
            "cfp/deepseek-ai/deepseek-v4-flash-0731",
            &nested
        ));
        assert!(!custom_model_matches("cfp/deepseek-ai", &nested));
    }
}
