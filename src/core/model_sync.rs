//! Background task that periodically fetches available models from upstream
//! providers and persists them as `custom_models` with `source: "auto_sync"`.
//!
//! Parity with 9router's automatic model list refresh — keeps the dashboard
//! model picker populated without requiring manual "Fetch Models" clicks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use tracing::{info, warn};

use crate::server::api::provider_models::{
    storage_alias_for_provider, supports_models_discovery, ProviderModel,
};
use crate::server::state::AppState;

const BOOT_DELAY: Duration = Duration::from_secs(30);
const SYNC_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
static TICK_RUNNING: AtomicBool = AtomicBool::new(false);

struct TickGuard;

impl Drop for TickGuard {
    fn drop(&mut self) {
        TICK_RUNNING.store(false, Ordering::Release);
    }
}

pub fn spawn_model_sync(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(BOOT_DELAY).await;
        loop {
            run_tick(&state).await;
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    });
}

async fn run_tick(state: &AppState) {
    if TICK_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let _guard = TickGuard;

    let snapshot = state.db.snapshot();
    let connections: Vec<_> = snapshot
        .provider_connections
        .iter()
        .filter(|c| c.is_active() && supports_models_discovery(&c.provider))
        .cloned()
        .collect();

    if connections.is_empty() {
        return;
    }

    info!(target: "zeroproxy::model_sync", "starting daily model sync for {} connections", connections.len());

    let mut total_added: usize = 0;
    let mut total_updated: usize = 0;
    let mut total_pruned: usize = 0;
    let mut errors: usize = 0;

    for connection in &connections {
        let provider = &connection.provider;
        let alias = storage_alias_for_provider(provider);

        match crate::server::api::provider_models::fetch_models_for_connection(state, connection)
            .await
        {
            Ok(models) => match apply_sync(state, &alias, &models).await {
                Ok((added, updated, pruned)) => {
                    total_added += added;
                    total_updated += updated;
                    total_pruned += pruned;
                    info!(
                        target: "zeroproxy::model_sync",
                        provider = %provider,
                        added, updated, pruned,
                        "synced models"
                    );
                }
                Err(e) => {
                    warn!(
                        target: "zeroproxy::model_sync",
                        provider = %provider,
                        "db update failed: {e}"
                    );
                    errors += 1;
                }
            },
            Err((status, msg)) => {
                warn!(
                    target: "zeroproxy::model_sync",
                    provider = %provider,
                    status = status.as_u16(),
                    "fetch failed: {msg}"
                );
                errors += 1;
            }
        }
    }

    info!(
        target: "zeroproxy::model_sync",
        "daily sync complete: +{total_added} added, ~{total_updated} updated, -{total_pruned} pruned, {errors} errors"
    );
}

async fn apply_sync(
    state: &AppState,
    provider_alias: &str,
    incoming: &[ProviderModel],
) -> Result<(usize, usize, usize), anyhow::Error> {
    let now = Utc::now().to_rfc3339();
    let source = "auto_sync";

    // Snapshot pre-existing auto_sync model IDs for this provider.
    let prev_ids: std::collections::HashSet<String> = state
        .db
        .snapshot()
        .custom_models
        .iter()
        .filter(|m| {
            m.provider_alias == provider_alias
                && m.extra.get("source").and_then(|v| v.as_str()) == Some(source)
                && m.r#type == "llm"
        })
        .map(|m| m.id.clone())
        .collect();

    let result = state
        .db
        .update(|db| {
            // Prune stale auto_sync models for this provider before re-inserting.
            db.custom_models.retain(|m| {
                !(m.provider_alias == provider_alias
                    && m.extra.get("source").and_then(|v| v.as_str()) == Some(source)
                    && m.r#type == "llm")
            });

            for model in incoming {
                let pre_existing = prev_ids.contains(&model.id);

                let mut extra = model.extra.clone();
                extra.insert(
                    "source".to_string(),
                    serde_json::Value::String(source.to_string()),
                );
                extra.insert(
                    "syncedAt".to_string(),
                    serde_json::Value::String(now.clone()),
                );

                let name = if model.name.is_empty() {
                    None
                } else {
                    Some(model.name.clone())
                };

                if pre_existing {
                    // Update existing in place.
                    for m in &mut db.custom_models {
                        if m.provider_alias == provider_alias
                            && m.id == model.id
                            && m.r#type == "llm"
                        {
                            m.name = name;
                            m.extra = extra;
                            break;
                        }
                    }
                    // Track after the loop in a counter below.
                } else {
                    db.custom_models.push(crate::types::CustomModel {
                        provider_alias: provider_alias.to_string(),
                        id: model.id.clone(),
                        r#type: "llm".to_string(),
                        name,
                        extra,
                        ..Default::default()
                    });
                }
            }
        })
        .await?;

    // Recount from the final db state for accuracy.
    let final_ids: std::collections::HashSet<String> = result
        .custom_models
        .iter()
        .filter(|m| {
            m.provider_alias == provider_alias
                && m.extra.get("source").and_then(|v| v.as_str()) == Some(source)
                && m.r#type == "llm"
        })
        .map(|m| m.id.clone())
        .collect();

    let incoming_ids: std::collections::HashSet<String> =
        incoming.iter().map(|m| m.id.clone()).collect();

    let added = incoming_ids.difference(&prev_ids).count();
    let updated = incoming_ids.intersection(&prev_ids).count();
    let pruned = prev_ids.difference(&final_ids).count();

    Ok((added, updated, pruned))
}
