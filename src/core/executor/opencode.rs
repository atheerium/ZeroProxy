use std::sync::Arc;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::core::proxy::ProxyTarget;
use crate::core::translator::helpers::openai_helper::normalize_developer_role;
use crate::core::utils::session_manager::resolve_session_identity;
use crate::types::{ProviderConnection, ProviderNode};

use super::{ClientPool, TransportKind, UpstreamResponse};

const OPENCODE_BASE: &str = "https://opencode.ai";
const OPENCODE_DEFAULT_PATH: &str = "/zen/v1/chat/completions";
const OPENCODE_RESPONSES_PATH: &str = "/zen/v1/responses";

/// Default User-Agent upstream (9router PR #4132): only opencode/>=1.18 passes gate.
const OPENCODE_UA: &str = "opencode/1.18.31 ai-sdk/provider-utils/4.0.40 runtime/bun/1.3.14";
const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const OPENCODE_FINGERPRINT_TOOLS: &[&str] = &["bash", "glob", "grep", "read"];

fn has_valid_opencode_version(ua: &str) -> bool {
    let lower = ua.to_ascii_lowercase();
    let Some(pos) = lower.find("opencode/") else {
        return false;
    };
    let rest = &lower[pos + "opencode/".len()..];
    let mut parts = rest.split(['.', ' ', '\t']);
    let Some(major) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    let Some(minor) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    major > 1 || (major == 1 && minor >= 17)
}
fn is_native_opencode_session(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("ses_") else {
        return false;
    };
    if rest.len() != 26 {
        return false;
    }
    let (hx, b62) = rest.split_at(12);
    hx.bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && b62.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn translate_opencode_session(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"opencode\0");
    hasher.update(seed.as_bytes());
    let d = hasher.finalize();
    let time_hex: String = d[..6].iter().map(|b| format!("{:02x}", b)).collect();
    let rand_part: String = d[6..20]
        .iter()
        .map(|b| BASE62_CHARS[(*b as usize) % 62] as char)
        .collect();
    format!("ses_{}{}", time_hex, rand_part)
}
fn resolve_gate_session(downstream: Option<&str>, seed: &str) -> String {
    if let Some(native) = downstream {
        if is_native_opencode_session(native) {
            return native.to_string();
        }
    }
    translate_opencode_session(seed)
}
fn tool_name_of(tool: &Value) -> Option<String> {
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            tool.get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
        })?
        .trim();
    (!name.is_empty()).then(|| name.to_string())
}
fn ensure_chat_fingerprint_tools(obj: &mut serde_json::Map<String, Value>) {
    let mut present: std::collections::HashSet<String> = std::collections::HashSet::new();
    let arr_ref = obj
        .entry("tools")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("arr");
    for t in arr_ref.iter() {
        if let Some(n) = tool_name_of(t) {
            present.insert(n);
        }
    }
    for name in OPENCODE_FINGERPRINT_TOOLS {
        if present.contains(*name) {
            continue;
        }
        arr_ref.push(json!({"type":"function","function":{"name":name,"description":format!("OpenCode built-in {} tool", name),"parameters":{"type":"object","properties":{}}}}));
        present.insert((*name).to_string());
    }
}
fn ensure_responses_fingerprint_tools(obj: &mut serde_json::Map<String, Value>) {
    // Flat-shape version (top-level name, no nested function)
    let mut present: std::collections::HashSet<String> = std::collections::HashSet::new();
    let arr_ref = obj
        .entry("tools")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("arr");
    for t in arr_ref.iter() {
        if let Some(n) = t
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            present.insert(n.to_string());
        }
    }
    for name in OPENCODE_FINGERPRINT_TOOLS {
        if present.contains(*name) {
            continue;
        }
        arr_ref.push(json!({"type":"function","name":name,"description":format!("OpenCode built-in {} tool", name),"parameters":{"type":"object","properties":{}}}));
        present.insert((*name).to_string());
    }
}

fn is_responses_model(model: &str) -> bool {
    let clean = model.to_lowercase();
    clean.contains("muse-spark")
        || clean.contains("muse_spark")
        || clean.contains("oc/")
        || clean.contains("oc")
        || (clean.contains("muse") && clean.contains("spark"))
}

#[derive(Clone)]
pub struct OpenCodeExecutor {
    pool: Arc<ClientPool>,
    provider_node: Option<ProviderNode>,
}

#[derive(Debug)]
pub enum OpenCodeExecutorError {
    RequestFailed(String),
    Serialize(serde_json::Error),
    HyperClientInit(std::io::Error),
    Hyper(hyper_util::client::legacy::Error),
    Request(reqwest::Error),
    InvalidHeader(reqwest::header::InvalidHeaderValue),
}

impl From<reqwest::Error> for OpenCodeExecutorError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<reqwest::header::InvalidHeaderValue> for OpenCodeExecutorError {
    fn from(error: reqwest::header::InvalidHeaderValue) -> Self {
        Self::InvalidHeader(error)
    }
}

impl From<hyper_util::client::legacy::Error> for OpenCodeExecutorError {
    fn from(error: hyper_util::client::legacy::Error) -> Self {
        Self::Hyper(error)
    }
}

impl From<std::io::Error> for OpenCodeExecutorError {
    fn from(error: std::io::Error) -> Self {
        Self::HyperClientInit(error)
    }
}

impl From<serde_json::Error> for OpenCodeExecutorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialize(error)
    }
}

pub struct OpenCodeExecutionRequest {
    pub model: String,
    pub body: Value,
    pub stream: bool,
    pub credentials: ProviderConnection,
    pub proxy: Option<ProxyTarget>,
    /// Downstream request headers for passthrough (9router rawHeaders).
    pub raw_headers: std::collections::BTreeMap<String, String>,
}

pub struct OpenCodeExecutorResponse {
    pub response: UpstreamResponse,
    pub url: String,
    pub headers: HeaderMap,
    pub transformed_body: Value,
    pub transport: TransportKind,
}

impl OpenCodeExecutor {
    pub fn new(
        pool: Arc<ClientPool>,
        provider_node: Option<ProviderNode>,
    ) -> Result<Self, OpenCodeExecutorError> {
        Ok(Self {
            pool,
            provider_node,
        })
    }

    pub fn pool(&self) -> &Arc<ClientPool> {
        &self.pool
    }

    fn build_url(&self, model: &str) -> String {
        // 9router 67271d85 (MESSAGES_MODELS emptied): every non-Responses
        // opencode model routes through /zen/v1/chat/completions (drop
        // big-pickle /zen/v1/messages special-case — live 500 verified).
        let path = if is_responses_model(model) {
            OPENCODE_RESPONSES_PATH
        } else {
            OPENCODE_DEFAULT_PATH
        };
        format!("{}{}", OPENCODE_BASE, path)
    }

    /// Build headers for the OpenCode request.
    ///
    /// Session management (9router v0.5.55): resolve a stable per-conversation
    /// session ID and pass through downstream OpenCode-specific headers when
    /// present. Forward the downstream User-Agent if it contains "opencode".
    fn build_headers(
        &self,
        credentials: &ProviderConnection,
        stream: bool,
        body: &Value,
        raw_headers: &std::collections::BTreeMap<String, String>,
    ) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer public"));

        // Conversation-stable seed; translate to canonical ses_ shape (9router PR #4132).
        let resolved_seed = resolve_session_identity(
            Some(&std::collections::HashMap::from_iter(
                raw_headers.iter().map(|(k, v)| (k.clone(), v.clone())),
            )),
            Some(body),
            Some(&credentials.id),
            "opencode",
        )
        .session_id;

        // Pass through downstream OpenCode-specific headers when present,
        // falling back to generated/default values.
        let downstream_ua = raw_headers
            .get("user-agent")
            .or_else(|| raw_headers.get("User-Agent"))
            .map(String::as_str)
            .unwrap_or("");
        let is_opencode_downstream = downstream_ua.to_lowercase().contains("opencode");

        let client = raw_headers
            .get("x-opencode-client")
            .map(String::as_str)
            .unwrap_or("desktop");
        let downstream_session = raw_headers.get("x-opencode-session").map(String::as_str);
        let session = resolve_gate_session(downstream_session, &resolved_seed);
        let request_id = raw_headers
            .get("x-opencode-request")
            .map(String::as_str)
            .unwrap_or("global");

        // User-Agent: forward gate-compatible UA only; else default.
        let ua = if is_opencode_downstream {
            downstream_ua
        } else {
            OPENCODE_UA
        };
        headers.insert(
            "User-Agent",
            HeaderValue::from_str(ua).unwrap_or_else(|_| HeaderValue::from_static(OPENCODE_UA)),
        );
        headers.insert(
            "x-opencode-client",
            HeaderValue::from_str(client).unwrap_or_else(|_| HeaderValue::from_static("desktop")),
        );
        headers.insert(
            "x-opencode-session",
            HeaderValue::from_str(&session)
                .unwrap_or_else(|_| HeaderValue::from_static("ses_unknown")),
        );
        headers.insert(
            "x-opencode-request",
            HeaderValue::from_str(request_id)
                .unwrap_or_else(|_| HeaderValue::from_static("global")),
        );
        headers.insert(
            "x-opencode-project",
            HeaderValue::from_str(
                raw_headers
                    .get("x-opencode-project")
                    .map(String::as_str)
                    .unwrap_or("global"),
            )
            .unwrap_or_else(|_| HeaderValue::from_static("global")),
        );

        if stream {
            headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        }

        headers
    }

    pub async fn execute_request(
        &self,
        mut request: OpenCodeExecutionRequest,
    ) -> Result<OpenCodeExecutorResponse, OpenCodeExecutorError> {
        // Normalize developer→system role; inject tool fingerprint when missing (9router PR #4132).
        normalize_developer_role(&mut request.body);
        if let Some(body_obj) = request.body.as_object_mut() {
            if is_responses_model(&request.model) {
                ensure_responses_fingerprint_tools(body_obj);
            } else {
                ensure_chat_fingerprint_tools(body_obj);
            }
        }

        let url = self.build_url(&request.model);
        let headers = self.build_headers(
            &request.credentials,
            request.stream,
            &request.body,
            &request.raw_headers,
        );

        let client = self.pool.get("opencode", request.proxy.as_ref())?;
        let response = client
            .post(&url)
            .headers(headers.clone())
            .json(&request.body)
            .send()
            .await?;

        Ok(OpenCodeExecutorResponse {
            response: UpstreamResponse::Reqwest(response),
            url,
            headers,
            transformed_body: request.body,
            transport: TransportKind::Reqwest,
        })
    }
}
