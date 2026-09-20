use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Build OpenCode CLI-equivalent headers (matches OmniRoute opencodeHeaders.ts
/// reference). Used by opencode-zen provider to unlock the upstream free pool.
pub fn opencode_headers(
    provider: &str,
    credentials_id: Option<&str>,
    incoming_raw_headers: &BTreeMap<String, String>,
    body: Option<&Value>,
    stream: bool,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // Default UA (matches our working probe).
    let default_ua = "opencode/1.18.18 ai-sdk/provider-utils/4.0.23 runtime/bun/1.3.14";
    // Client default: cli (our working probe used cli).
    let default_client = "cli";
    let default_project = "global";

    // Session: deterministic hash of conversation messages if body present,
    // else UUID or credential-based fallback. Match OmniRoute: conversation-stable.
    let session_id = if let Some(b) = body {
        if let Some(messages) = b.get("messages").and_then(Value::as_array) {
            let msg_str = format!("{:?}", messages);
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            msg_str.hash(&mut hasher);
            format!("opencode_session_{}", hasher.finish())
        } else {
            let raw_session = incoming_raw_headers
                .get("x-opencode-session")
                .cloned()
                .unwrap_or_else(|| "opencode_default_session".to_string());
            // If session is empty string (credential.id empty), don't send empty header.
            if raw_session.is_empty() {
                "opencode_default_session".to_string()
            } else {
                raw_session
            }
        }
    } else {
        incoming_raw_headers
            .get("x-opencode-session")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "opencode_default_session".to_string())
    };

    // Forward client's User-Agent when it contains "opencode" (client talking through proxy).
    let downstream_ua = incoming_raw_headers
        .get("user-agent")
        .or_else(|| incoming_raw_headers.get("User-Agent"))
        .map(String::as_str)
        .unwrap_or("");
    let ua_str = if downstream_ua.to_lowercase().contains("opencode") {
        downstream_ua.to_string()
    } else {
        default_ua.to_string()
    };
    headers.insert(
        reqwest::header::HeaderName::from_bytes(b"User-Agent").unwrap(),
        HeaderValue::from_str(&ua_str).unwrap_or_else(|_| HeaderValue::from_static(default_ua)),
    );

    // Client: forward from incoming if present, else cli.
    let client_str = incoming_raw_headers
        .get("x-opencode-client")
        .map(String::as_str)
        .unwrap_or(default_client);
    headers.insert(
        reqwest::header::HeaderName::from_bytes(b"x-opencode-client").unwrap(),
        HeaderValue::from_str(client_str)
            .unwrap_or_else(|_| HeaderValue::from_static(default_client)),
    );

    // Session: never send empty header.
    headers.insert(
        reqwest::header::HeaderName::from_bytes(b"x-opencode-session").unwrap(),
        HeaderValue::from_str(&session_id)
            .unwrap_or_else(|_| HeaderValue::from_static("opencode_default_session")),
    );

    // Request ID: fresh UUID per upstream call.
    let request_id_str = incoming_raw_headers
        .get("x-opencode-request")
        .map(String::as_str)
        .map(|r| {
            if r.is_empty() {
                format!("opencode_req_{}", uuid::Uuid::new_v4())
            } else {
                r.to_string()
            }
        })
        .unwrap_or_else(|| format!("opencode_req_{}", uuid::Uuid::new_v4()));
    headers.insert(
        reqwest::header::HeaderName::from_bytes(b"x-opencode-request").unwrap(),
        HeaderValue::from_str(&request_id_str)
            .unwrap_or_else(|_| HeaderValue::from_static("opencode_req_default")),
    );

    // Project: forward or default global.
    let project_str = incoming_raw_headers
        .get("x-opencode-project")
        .map(String::as_str)
        .unwrap_or(default_project);
    headers.insert(
        reqwest::header::HeaderName::from_bytes(b"x-opencode-project").unwrap(),
        HeaderValue::from_str(project_str)
            .unwrap_or_else(|_| HeaderValue::from_static(default_project)),
    );

    if stream {
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        );
    }

    headers
}
