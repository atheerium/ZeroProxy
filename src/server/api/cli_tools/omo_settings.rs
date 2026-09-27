//! OMO (`omo.dev`, "oh-my-openagent") is an OpenCode plugin, not a standalone
//! harness. Its `~/.omo/omo.jsonc` configures a fleet of sub-agents, each with a
//! primary model plus fallbacks, so this card edits the sub-agent assignments
//! rather than a single base URL.
//!
//! The config is JSONC — ~40 comment lines documenting the routing strategy and
//! ~90 trailing commas in the maintainer's copy. Two consequences drive the
//! design here:
//!
//! 1. `serde_json` cannot parse it (it fails at the first `/`). So reads go
//!    through [`strip_jsonc`] first. The same defect is why
//!    `GET /api/cli-tools/opencode-settings` returns HTTP 500 for a commented
//!    `opencode.jsonc` — that handler parses the raw text.
//! 2. The comments are the user's own routing rationale. Re-serialising the
//!    parsed value would silently delete all of them, so writes are *surgical*:
//!    one string value is replaced in place and every other byte is untouched.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result as AnyhowResult};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio::fs;

use crate::server::state::AppState;

/// Written once before the first edit so `DELETE` can restore. Never overwritten,
/// so it always holds the config as the maintainer authored it.
const BACKUP_SUFFIX: &str = "zeroproxy.bak";

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/cli-tools/omo-settings",
        get(get_omo_settings)
            .patch(patch_omo_settings)
            .delete(delete_omo_settings),
    )
}

/// Which half of the config a slot lives in. `agents` and `categories` hold the
/// same shape, so the only thing that differs is the parent object name.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum SlotKind {
    Agent,
    Category,
}

impl SlotKind {
    fn section_name(self) -> &'static str {
        match self {
            SlotKind::Agent => "agents",
            SlotKind::Category => "categories",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchOmoSettingsRequest {
    kind: SlotKind,
    name: String,
    model: String,
}

fn config_path() -> PathBuf {
    home_dir().join(".omo").join("omo.jsonc")
}

fn backup_path() -> PathBuf {
    let path = config_path();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "omo.jsonc".to_string());
    path.with_file_name(format!("{name}.{BACKUP_SUFFIX}"))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

// ---------------------------------------------------------------------------
// JSONC -> JSON
// ---------------------------------------------------------------------------

/// Strip `//` and `/* */` comments and trailing commas so `serde_json` can read
/// the result.
///
/// Comment stripping is string-aware: a `//` inside a JSON string is data, not a
/// comment, and a model id may legitimately contain one.
///
/// **The output is byte-for-byte the same length as the input** — comment
/// characters are overwritten with spaces and newlines are kept. That is what
/// lets [`replace_slot_model`] compute offsets against this text and splice the
/// *original*, so the author's comments survive the write. Collapsing a comment
/// to a shorter placeholder would shift every later offset and corrupt the file.
fn strip_jsonc(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < bytes.len() {
        let c = bytes[i];

        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if c == b'"' {
            in_string = true;
            i += 1;
        } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let end = bytes[i + 2..]
                .windows(2)
                .position(|w| w == b"*/")
                .map(|p| i + 2 + p)
                .unwrap_or(bytes.len());
            for slot in out.iter_mut().take(end).skip(i) {
                if *slot != b'\n' {
                    *slot = b' ';
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }

    remove_trailing_commas(&String::from_utf8_lossy(&out))
}

/// Overwrite the commas that sit before a `}` or `]`, which JSONC allows and JSON
/// does not. In place, for the same offset-stability reason as [`strip_jsonc`].
fn remove_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = bytes.to_vec();
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if c == b',' {
            // Look ahead in the SOURCE, not the output being built, and past
            // whitespace: a comment-stripped file is full of `,\n    }`.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if matches!(bytes.get(j), Some(b'}') | Some(b']')) {
                out[i] = b' ';
            }
        }
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

async fn read_omo_value() -> AnyhowResult<Option<Value>> {
    match fs::read_to_string(config_path()).await {
        Ok(text) => {
            let cleaned = strip_jsonc(&text);
            Ok(Some(serde_json::from_str(&cleaned)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Flatten `agents` / `categories` into the list shape the dashboard renders.
fn slots_from(value: &Value, section: &str) -> Vec<Value> {
    let Some(object) = value
        .get("[opencode]")
        .and_then(|v| v.get(section))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };

    let mut rows: Vec<Value> = object
        .iter()
        .map(|(name, entry)| {
            let fallbacks = entry
                .get("fallback_models")
                .or_else(|| entry.get("models"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.get("model").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            json!({
                "name": name,
                "model": entry.get("model").and_then(Value::as_str).unwrap_or_default(),
                "reasoning": entry.get("reasoning").and_then(Value::as_str).unwrap_or_default(),
                "fallbacks": fallbacks,
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        a.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(b.get("name").and_then(Value::as_str).unwrap_or_default())
    });
    rows
}

pub(super) async fn get_omo_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = super::super::require_dashboard_or_management_api_key(&headers, &state) {
        return response;
    }

    let value = match read_omo_value().await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return Json(json!({
                "installed": true,
                "exists": false,
                "configPath": config_path().to_string_lossy(),
                "agents": [],
                "categories": [],
            }))
            .into_response()
        }
        Err(error) => {
            tracing::warn!(?error, "failed to read omo config");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to read ~/.omo/omo.jsonc: {error}") })),
            )
                .into_response();
        }
    };

    Json(json!({
        "installed": true,
        "exists": true,
        "configPath": config_path().to_string_lossy(),
        "agents": slots_from(&value, "agents"),
        "categories": slots_from(&value, "categories"),
    }))
    .into_response()
}

async fn patch_omo_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PatchOmoSettingsRequest>,
) -> Response {
    if let Err(response) = super::super::require_dashboard_or_management_api_key(&headers, &state) {
        return response;
    }

    if body.name.trim().is_empty() || body.model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "name and model are required" })),
        )
            .into_response();
    }

    if let Err(error) = write_slot_model(body.kind, &body.name, &body.model).await {
        tracing::warn!(?error, "failed to patch omo config");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to update omo config: {error}") })),
        )
            .into_response();
    }

    let value = read_omo_value()
        .await
        .ok()
        .flatten()
        .unwrap_or(Value::Object(Map::new()));
    Json(json!({
        "success": true,
        "configPath": config_path().to_string_lossy(),
        "agents": slots_from(&value, "agents"),
        "categories": slots_from(&value, "categories"),
    }))
    .into_response()
}

async fn delete_omo_settings(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = super::super::require_dashboard_or_management_api_key(&headers, &state) {
        return response;
    }

    let backup = backup_path();
    if !backup.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "No ZeroProxy backup exists yet, so there is nothing to restore." })),
        )
            .into_response();
    }

    match fs::copy(&backup, config_path()).await {
        Ok(_) => Json(json!({
            "success": true,
            "message": "Restored the config as it was before ZeroProxy's first edit.",
            "configPath": config_path().to_string_lossy(),
        }))
        .into_response(),
        Err(error) => {
            tracing::warn!(?error, "failed to restore omo backup");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to restore backup: {error}") })),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Surgical write
// ---------------------------------------------------------------------------

async fn write_slot_model(kind: SlotKind, name: &str, model: &str) -> AnyhowResult<()> {
    let path = config_path();
    let original = fs::read_to_string(&path)
        .await
        .with_context(|| format!("read {}", path.to_string_lossy()))?;

    // Snapshot before the first edit so DELETE can restore the authored config.
    let backup = backup_path();
    if !backup.exists() {
        fs::write(&backup, &original).await.ok();
    }

    let updated = replace_slot_model(&original, kind, name, model)?;
    if updated == original {
        anyhow::bail!(
            "slot `{name}` was not found under `{}`",
            kind.section_name()
        );
    }

    fs::write(&path, updated)
        .await
        .with_context(|| format!("write {}", path.to_string_lossy()))
}

/// Replace the primary `model` of one slot, leaving every other byte identical.
///
/// Parsing to a `Value` and re-serialising would be far simpler and would delete
/// the routing-strategy comments the maintainer wrote. Instead this locates the
/// byte range of the one string literal to change.
fn replace_slot_model(text: &str, kind: SlotKind, name: &str, model: &str) -> AnyhowResult<String> {
    let cleaned = strip_jsonc(text);
    let slot_start = find_object_key(&cleaned, kind.section_name())
        .and_then(|(_, after_key)| find_next_object(&cleaned, after_key))
        .ok_or_else(|| anyhow::anyhow!("`{}` section not found", kind.section_name()))?;

    let slot_end = matching_brace(&cleaned, slot_start)
        .ok_or_else(|| anyhow::anyhow!("unterminated `{}` section", kind.section_name()))?;

    let needle = format!("\"{name}\"");
    let mut cursor = slot_start;
    while let Some(found) = cleaned[cursor..slot_end].find(&needle) {
        let at = cursor + found;
        let after = at + needle.len();
        let trimmed = cleaned[after..].trim_start();
        let lead = after + (cleaned[after..].len() - trimmed.len());
        if trimmed.starts_with(':') {
            if let Some(brace) = cleaned[lead + 1..].find('{') {
                let brace_at = lead + 1 + brace;
                let end = matching_brace(&cleaned, brace_at)
                    .ok_or_else(|| anyhow::anyhow!("unterminated object for slot `{name}`"))?;
                if let Some((_, after_value)) = find_object_key(&cleaned[brace_at..end], "model") {
                    // `find_object_key` reports offsets relative to the slice it
                    // was given; rebase onto the full document.
                    let after_value = brace_at + after_value;
                    let trimmed_value = cleaned[after_value..end].trim_start();
                    let value_at =
                        after_value + (cleaned[after_value..end].len() - trimmed_value.len());
                    let quote = value_at;
                    let quote_end = find_string_end(&cleaned, quote)
                        .ok_or_else(|| anyhow::anyhow!("unterminated model string for `{name}`"))?;
                    let mut out = String::with_capacity(text.len() + model.len());
                    out.push_str(&text[..quote]);
                    out.push_str(&json_string(model));
                    out.push_str(&text[quote_end + 1..]);
                    return Ok(out);
                }
            }
        }
        cursor = at + needle.len();
    }

    anyhow::bail!("slot `{name}` has no `model` field")
}

fn json_string(value: &str) -> String {
    Value::String(value.to_string()).to_string()
}

/// Index of the key literal, plus the index just past its `:` so callers can
/// read the value directly.
fn find_object_key(text: &str, key: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{key}\"");
    let mut cursor = 0;
    while let Some(found) = text[cursor..].find(&needle) {
        let at = cursor + found;
        let after = at + needle.len();
        let trimmed = text[after..].trim_start();
        if trimmed.starts_with(':') {
            let colon = after + (text[after..].len() - trimmed.len());
            return Some((at, colon + 1));
        }
        cursor = at + needle.len();
    }
    None
}

/// Index of the first `{` at or after `from`.
fn find_next_object(text: &str, from: usize) -> Option<usize> {
    text[from..].find('{').map(|i| from + i)
}

/// Index of the `}` matching the `{` at `open`, or `None` when unbalanced.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &c) in bytes.iter().enumerate().skip(open) {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Index of the closing quote of the string literal starting at `open`.
fn find_string_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut escaped = false;
    for (i, &c) in bytes.iter().enumerate().skip(open + 1) {
        if escaped {
            escaped = false;
        } else if c == b'\\' {
            escaped = true;
        } else if c == b'"' {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  // routing strategy
  "codegraph": {},
  "[opencode]": {
    "agents": {
      // primary workhorse
      "sisyphus": {
        "model": "opencode/mimo-v2.6-flash-free",
        "reasoning": "high",
        "fallback_models": [
          { "model": "openrouter/nvidia/x:free", },
        ],
      },
    },
    "categories": {
      "quick": { "model": "zai/glm-4.7-flash", },
    },
  },
  "_migrations": [],
}"#;

    #[test]
    fn strip_jsonc_removes_comments_and_trailing_commas() {
        let cleaned = strip_jsonc(SAMPLE);
        assert!(!cleaned.contains("routing strategy"));
        assert!(!cleaned.contains("primary workhorse"));
        let parsed: Value =
            serde_json::from_str(&cleaned).expect("stripped text must be valid JSON");
        assert_eq!(
            parsed["[opencode]"]["agents"]["sisyphus"]["model"],
            "opencode/mimo-v2.6-flash-free"
        );
        assert_eq!(
            parsed["[opencode]"]["categories"]["quick"]["model"],
            "zai/glm-4.7-flash"
        );
    }

    #[test]
    fn strip_jsonc_keeps_comment_markers_inside_strings() {
        let cleaned = strip_jsonc(r#"{"a":"http://x/y","b":"/* not a comment */"}"#);
        let parsed: Value = serde_json::from_str(&cleaned).expect("valid JSON");
        assert_eq!(parsed["a"], "http://x/y");
        assert_eq!(parsed["b"], "/* not a comment */");
    }

    #[test]
    fn patch_replaces_only_the_targeted_model() {
        let out = replace_slot_model(
            SAMPLE,
            SlotKind::Agent,
            "sisyphus",
            "nvidia/nvidia/nemotron-3-ultra-550b-a55b",
        )
        .expect("patch applies");

        // Comments survive, because the write is surgical.
        assert!(out.contains("routing strategy"));
        assert!(out.contains("primary workhorse"));
        // The targeted model changed; nothing else did.
        assert!(out.contains("\"model\": \"nvidia/nvidia/nemotron-3-ultra-550b-a55b\""));
        assert!(!out.contains("opencode/mimo-v2.6-flash-free"));
        // Fallbacks and other slots are untouched.
        assert!(out.contains("openrouter/nvidia/x:free"));
        assert!(out.contains("zai/glm-4.7-flash"));
    }

    #[test]
    fn patch_targets_categories_independently_of_agents() {
        let out = replace_slot_model(SAMPLE, SlotKind::Category, "quick", "openrouter/free:free")
            .expect("patch applies");
        assert!(out.contains("\"model\": \"openrouter/free:free\""));
        assert!(out.contains("opencode/mimo-v2.6-flash-free"));
    }

    #[test]
    fn patch_rejects_an_unknown_slot() {
        assert!(replace_slot_model(SAMPLE, SlotKind::Agent, "no-such-agent", "x/y").is_err());
    }

    #[test]
    fn slots_from_merges_fallbacks_and_models_and_sorts() {
        let value: Value = serde_json::from_str(&strip_jsonc(SAMPLE)).expect("valid JSON");
        let agents = slots_from(&value, "agents");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0]["name"], "sisyphus");
        assert_eq!(agents[0]["fallbacks"][0], "openrouter/nvidia/x:free");

        let categories = slots_from(&value, "categories");
        assert_eq!(categories[0]["name"], "quick");
    }
}
