//! Kimi Web executor — `www.kimi.ai` Connect-RPC chat API.
//!
//! Port of OmniRoute `open-sse/executors/kimi-web.ts`:
//! - OpenAI chat body → single user turn (system + user/assistant transcript)
//! - Connect-RPC wire protocol (5-byte framed JSON)
//! - Browser-fingerprint headers
//! - Stream events with `op` + `mask` + nested `block.{text,think}.content`
//!   → OpenAI SSE chunks (`reasoning_content` for think deltas)
//! - Refresh-token flow on 401 (15-min access_token TTL)
//! - `${code}: ${message}` error extraction from end-of-stream error frame
//!
//! Auth: user's `access_token` (from kimi.ai localStorage) — may be raw JWT,
//! JSON dump `{access_token, refresh_token}`, `Bearer x`, or
//! `access_token=<value>` / `kimi-auth=<value>` kv. Stored in
//! `credentials.api_key` (access) and `credentials.refresh_token` (refresh).

use std::sync::Arc;

use base64::Engine;
use futures_util::StreamExt;
use hyper::http;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT,
};
use reqwest::Body as ReqwestBody;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::proxy::ProxyTarget;
use crate::types::ProviderConnection;

use super::{ClientPool, TransportKind, UpstreamResponse};

const KIMI_BASE: &str = "https://www.kimi.ai";
const KIMI_CHAT_PATH: &str = "/apiv2/kimi.gateway.chat.v1.ChatService/Chat";
const KIMI_REFRESH_PATH: &str = "/api/auth/token/refresh";
const KIMI_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

/// Per-frame MAX_FRAME_LEN (8 MiB) — Kimi's largest legitimate event is <1 KiB.
const MAX_FRAME_LEN: usize = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Kimi model config registry
// ---------------------------------------------------------------------------

struct KimiModelConfig {
    scenario: &'static str,
    kimi_plus_id: Option<&'static str>,
    supported_reasoning_efforts: &'static [&'static str],
    default_reasoning_effort: Option<&'static str>,
}

fn kimi_model_registry() -> &'static [(&'static str, KimiModelConfig)] {
    &[
        (
            "k3",
            KimiModelConfig {
                scenario: "SCENARIO_K2D5",
                kimi_plus_id: None,
                supported_reasoning_efforts: &["REASONING_EFFORT_NONE", "REASONING_EFFORT_LOW"],
                default_reasoning_effort: Some("REASONING_EFFORT_NONE"),
            },
        ),
        (
            "k2d6",
            KimiModelConfig {
                scenario: "SCENARIO_K2D5",
                kimi_plus_id: None,
                supported_reasoning_efforts: &["REASONING_EFFORT_NONE", "REASONING_EFFORT_LOW"],
                default_reasoning_effort: Some("REASONING_EFFORT_NONE"),
            },
        ),
    ]
}

fn resolve_kimi_model(model: &str) -> Option<&'static KimiModelConfig> {
    kimi_model_registry()
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, cfg)| cfg)
}

fn normalize_reasoning_effort(value: Option<&str>, cfg: &KimiModelConfig) -> Option<String> {
    let raw = value.map(|s| s.trim()).filter(|s| !s.is_empty());
    let normalized = match raw {
        Some(v) if v.to_uppercase().starts_with("REASONING_EFFORT_") => v.to_uppercase(),
        Some(v) => format!("REASONING_EFFORT_{}", v.to_uppercase()),
        None => cfg.default_reasoning_effort?.to_string(),
    };
    if cfg
        .supported_reasoning_efforts
        .iter()
        .any(|s| *s == normalized.as_str())
    {
        Some(normalized)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Credential normalization
// ---------------------------------------------------------------------------

/// Extract the kimi access token from a raw value that may be:
/// - raw JWT
/// - JSON dump `{"access_token":"..."[, "refresh_token":"..."]}`
/// - `Bearer <token>` header value
/// - `access_token=<value>` or `kimi-auth=<value>` cookie-style kv
pub fn extract_kimi_access_token(raw_value: &str) -> String {
    let raw = raw_value.trim();
    if raw.is_empty() {
        return String::new();
    }
    if raw.starts_with('{') && raw.ends_with('}') {
        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
            for k in ["access_token", "token"] {
                if let Some(s) = parsed.get(k).and_then(Value::as_str) {
                    let s = s.trim();
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
        }
    }
    if let Some(caps) = regex::Regex::new(r"(?i)^(?:authorization:\s*)?bearer\s+([^;\s]+)")
        .ok()
        .and_then(|re| re.captures(raw))
    {
        return caps.get(1).unwrap().as_str().to_string();
    }
    for key in ["access_token", "kimi-auth"] {
        let pat = format!(r"(?:^|[\s;]){}=([^;\s]+)", regex::escape(key));
        if let Some(caps) = regex::Regex::new(&pat).ok().and_then(|re| re.captures(raw)) {
            return caps.get(1).unwrap().as_str().to_string();
        }
    }
    if !raw.contains('=') && !raw.contains(';') {
        return raw.to_string();
    }
    String::new()
}

pub fn extract_kimi_refresh_token(raw_value: &str) -> String {
    let raw = raw_value.trim();
    if raw.is_empty() {
        return String::new();
    }
    if raw.starts_with('{') && raw.ends_with('}') {
        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
            if let Some(s) = parsed.get("refresh_token").and_then(Value::as_str) {
                let s = s.trim();
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    if let Some(caps) = regex::Regex::new(r"(?:^|[\s;])refresh_token=([^;\s]+)")
        .ok()
        .and_then(|re| re.captures(raw))
    {
        return caps.get(1).unwrap().as_str().to_string();
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct KimiWebExecutionRequest {
    pub model: String,
    pub body: Value,
    pub stream: bool,
    pub credentials: ProviderConnection,
    pub proxy: Option<ProxyTarget>,
}

#[derive(Debug)]
pub enum KimiWebExecutorError {
    MissingCredentials(String),
    InvalidCredentials(String),
    Serialize(serde_json::Error),
    Request(reqwest::Error),
    UnsupportedModel(String),
    ConnectProtocol(String),
    RefreshFailed(String),
}

impl From<reqwest::Error> for KimiWebExecutorError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<serde_json::Error> for KimiWebExecutorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialize(error)
    }
}

impl std::fmt::Display for KimiWebExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredentials(p) => write!(f, "Missing credentials for {p}"),
            Self::InvalidCredentials(msg) => write!(f, "Invalid credentials: {msg}"),
            Self::Serialize(e) => write!(f, "Serialization error: {e}"),
            Self::Request(e) => write!(f, "Request error: {e}"),
            Self::UnsupportedModel(m) => write!(f, "Unsupported Kimi model: {m}"),
            Self::ConnectProtocol(msg) => write!(f, "Connect protocol error: {msg}"),
            Self::RefreshFailed(msg) => write!(f, "Kimi refresh failed: {msg}"),
        }
    }
}

impl std::error::Error for KimiWebExecutorError {}

pub struct KimiWebExecutorResponse {
    pub response: UpstreamResponse,
    pub url: String,
    pub headers: HeaderMap,
    pub transformed_body: Value,
    pub transport: TransportKind,
}

impl std::fmt::Debug for KimiWebExecutorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KimiWebExecutorResponse")
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("transformed_body", &self.transformed_body)
            .field("transport", &self.transport)
            .finish()
    }
}

pub struct KimiWebExecutor {
    pool: Arc<ClientPool>,
}

// ---------------------------------------------------------------------------
// Connect-RPC framing
// ---------------------------------------------------------------------------

fn encode_connect_frame(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(0x00);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

struct ConnectFrame {
    flags: u8,
    message: Option<Value>,
}

fn decode_connect_frame(buf: &[u8], offset: usize) -> Result<(usize, ConnectFrame), String> {
    if offset + 5 > buf.len() {
        return Ok((
            0,
            ConnectFrame {
                flags: 0,
                message: None,
            },
        ));
    }
    let flags = buf[offset];
    let len = u32::from_be_bytes([
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
    ]) as usize;
    if len > MAX_FRAME_LEN {
        return Err(format!(
            "Kimi Connect frame exceeded MAX_FRAME_LEN ({} > {})",
            len, MAX_FRAME_LEN
        ));
    }
    if offset + 5 + len > buf.len() {
        return Ok((
            0,
            ConnectFrame {
                flags: 0,
                message: None,
            },
        ));
    }
    // Lenient framing: ignore unknown flag bits (only 0x01 compress + 0x02
    // end-stream are defined), and skip compressed payloads we cannot parse
    // instead of failing the whole stream.
    if (flags & 0x01) != 0 {
        return Ok((
            5 + len,
            ConnectFrame {
                flags,
                message: None,
            },
        ));
    }
    let payload = &buf[offset + 5..offset + 5 + len];
    let message = if len > 0 {
        match serde_json::from_slice::<Value>(payload) {
            Ok(v) => Some(v),
            // Best-effort deltas: a malformed data frame is skipped; an
            // EndStream frame with an unparseable body is treated as a clean
            // end (callers check content presence before erroring).
            Err(_) => None,
        }
    } else {
        None
    };
    Ok((5 + len, ConnectFrame { flags, message }))
}

fn end_stream_error(frame: &ConnectFrame) -> Option<String> {
    if (frame.flags & 0x02) == 0 {
        return None;
    }
    let err = frame.message.as_ref()?.get("error")?;
    let obj = err.as_object()?;
    let code = obj.get("code").and_then(Value::as_str).unwrap_or("unknown");
    let message = obj
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("upstream error");
    Some(format!("{code}: {message}"))
}

fn extract_kimi_delta(msg: Option<&Value>) -> Option<(&'static str, String)> {
    let msg = msg?;
    let op = msg.get("op").and_then(Value::as_str).unwrap_or("");
    let mask = msg.get("mask").and_then(Value::as_str).unwrap_or("");
    let block = msg.get("block").cloned().unwrap_or(Value::Null);
    let block = block.as_object()?;

    if op == "append" {
        if mask == "block.text.content" {
            let text = block
                .get("text")
                .and_then(Value::as_object)
                .and_then(|o| o.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return (!text.is_empty()).then(|| ("text", text.to_string()));
        }
        if mask == "block.think.content" {
            let text = block
                .get("think")
                .and_then(Value::as_object)
                .and_then(|o| o.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return (!text.is_empty()).then(|| ("think", text.to_string()));
        }
        return None;
    }

    if op == "set" {
        if mask == "block.text" {
            let text = block
                .get("text")
                .and_then(Value::as_object)
                .and_then(|o| o.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return (!text.is_empty()).then(|| ("text", text.to_string()));
        }
        if mask == "block.think" {
            let text = block
                .get("think")
                .and_then(Value::as_object)
                .and_then(|o| o.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return (!text.is_empty()).then(|| ("think", text.to_string()));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Message folding
// ---------------------------------------------------------------------------

fn fold_messages(messages: &[Value]) -> Result<(String, String), String> {
    let mut system_parts: Vec<String> = Vec::new();
    let mut conversation_parts: Vec<String> = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("user");
        if role == "tool" || role == "function" {
            return Err("Kimi Web does not support tool result messages".to_string());
        }
        if msg.get("tool_calls").is_some() {
            return Err("Kimi Web does not support assistant tool calls".to_string());
        }
        let content = msg.get("content").cloned().unwrap_or(Value::Null);
        let text = if let Some(s) = content.as_str() {
            s.to_string()
        } else if let Some(arr) = content.as_array() {
            let mut buf = String::new();
            for part in arr {
                let p = part
                    .as_object()
                    .ok_or_else(|| "Kimi Web only supports text message content".to_string())?;
                let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
                if ptype == "text" || ptype == "input_text" {
                    if let Some(s) = p.get("text").and_then(Value::as_str) {
                        buf.push_str(s);
                    }
                } else {
                    return Err(
                        "Kimi Web does not support image, audio, file, or tool content".to_string(),
                    );
                }
            }
            buf
        } else {
            String::new()
        };
        if text.trim().is_empty() {
            continue;
        }
        match role {
            "system" | "developer" => system_parts.push(text),
            "user" => {
                if conversation_parts.is_empty() {
                    conversation_parts.push(text);
                } else {
                    conversation_parts.push(format!("User: {text}"));
                }
            }
            "assistant" => conversation_parts.push(format!("Assistant: {text}")),
            _ => return Err(format!("Kimi Web does not support message role {role}")),
        }
    }

    Ok((
        conversation_parts.join("\n\n").trim().to_string(),
        system_parts.join("\n\n").trim().to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Response conversion helpers
// ---------------------------------------------------------------------------

fn sse_chunk(
    cid: &str,
    created: i64,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
) -> String {
    format!(
        "data: {}\n\n",
        serde_json::to_string(&json!({
            "id": cid,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "system_fingerprint": Value::Null,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason
                    .map(|s| Value::String(s.to_string()))
                    .unwrap_or(Value::Null),
                "logprobs": Value::Null,
            }],
        }))
        .unwrap_or_default()
    )
}

async fn stream_kimi_to_sse(
    response: reqwest::Response,
    model: &str,
) -> Result<UpstreamResponse, KimiWebExecutorError> {
    let cid = format!(
        "chatcmpl-kimi-{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    let created = chrono::Utc::now().timestamp();
    let mut out = String::new();
    let mut emitted_role = false;
    let mut byte_stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let bytes = chunk_result.map_err(KimiWebExecutorError::Request)?;
        buf.extend_from_slice(&bytes);

        let mut offset = 0;
        loop {
            let (consumed, frame) = match decode_connect_frame(&buf, offset) {
                Ok(v) => v,
                Err(e) => return Err(KimiWebExecutorError::ConnectProtocol(e)),
            };
            if consumed == 0 {
                break;
            }
            offset += consumed;

            if (frame.flags & 0x02) != 0 {
                if let Some(err) = end_stream_error(&frame) {
                    return Err(KimiWebExecutorError::ConnectProtocol(format!(
                        "Kimi Connect EndStream error: {err}"
                    )));
                }
                if !emitted_role {
                    out.push_str(&sse_chunk(
                        &cid,
                        created,
                        model,
                        json!({ "role": "assistant", "content": "" }),
                        None,
                    ));
                }
                out.push_str(&sse_chunk(&cid, created, model, json!({}), Some("stop")));
                out.push_str("data: [DONE]\n\n");
                buf.drain(..offset);
                let bytes = out.into_bytes();
                let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
                *http_resp.status_mut() = reqwest::StatusCode::OK;
                http_resp
                    .headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
                return Ok(UpstreamResponse::Reqwest(reqwest::Response::from(
                    http_resp,
                )));
            }

            if let Some((kind, text)) = extract_kimi_delta(frame.message.as_ref()) {
                if !emitted_role {
                    out.push_str(&sse_chunk(
                        &cid,
                        created,
                        model,
                        json!({ "role": "assistant", "content": "" }),
                        None,
                    ));
                    emitted_role = true;
                }
                let delta = if kind == "think" {
                    json!({ "reasoning_content": text })
                } else {
                    json!({ "content": text })
                };
                out.push_str(&sse_chunk(&cid, created, model, delta, None));
            }
        }
        buf.drain(..offset);
    }

    // Tolerate a missing EndStream frame when deltas were already emitted
    // (gateway sometimes closes the body right after the last data frame).
    if emitted_role {
        out.push_str(&sse_chunk(&cid, created, model, json!({}), Some("stop")));
        out.push_str("data: [DONE]\n\n");
        let bytes = out.into_bytes();
        let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
        *http_resp.status_mut() = reqwest::StatusCode::OK;
        http_resp
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        return Ok(UpstreamResponse::Reqwest(reqwest::Response::from(
            http_resp,
        )));
    }

    Err(KimiWebExecutorError::ConnectProtocol(
        "Kimi Connect stream ended without a successful EndStream frame".to_string(),
    ))
}

async fn collect_kimi_content(
    response: reqwest::Response,
) -> Result<(String, String), KimiWebExecutorError> {
    let mut answer = String::new();
    let mut reasoning = String::new();
    let mut byte_stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut saw_end = false;

    while let Some(chunk_result) = byte_stream.next().await {
        let bytes = chunk_result.map_err(KimiWebExecutorError::Request)?;
        buf.extend_from_slice(&bytes);
        let mut offset = 0;
        loop {
            let (consumed, frame) = match decode_connect_frame(&buf, offset) {
                Ok(v) => v,
                Err(e) => return Err(KimiWebExecutorError::ConnectProtocol(e)),
            };
            if consumed == 0 {
                break;
            }
            offset += consumed;
            if (frame.flags & 0x02) != 0 {
                if let Some(err) = end_stream_error(&frame) {
                    return Err(KimiWebExecutorError::ConnectProtocol(format!(
                        "Kimi Connect EndStream error: {err}"
                    )));
                }
                saw_end = true;
                break;
            }
            if let Some((kind, text)) = extract_kimi_delta(frame.message.as_ref()) {
                if kind == "think" {
                    reasoning.push_str(&text);
                } else {
                    answer.push_str(&text);
                }
            }
        }
        buf.drain(..offset);
        if saw_end {
            break;
        }
    }

    if !saw_end {
        // Same tolerance as the streaming path: content without an EndStream
        // frame still counts as a usable answer.
        if !answer.is_empty() || !reasoning.is_empty() {
            return Ok((answer, reasoning));
        }
        return Err(KimiWebExecutorError::ConnectProtocol(
            "Kimi Connect stream ended without a successful EndStream frame".to_string(),
        ));
    }
    Ok((answer, reasoning))
}

fn json_error(status: u16, message: &str, err_type: &str, code: Option<&str>) -> UpstreamResponse {
    let mut body = json!({ "error": { "message": message, "type": err_type } });
    if let Some(code) = code {
        body["error"]["code"] = Value::String(code.to_string());
    }
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
    *http_resp.status_mut() =
        reqwest::StatusCode::from_u16(status).unwrap_or(reqwest::StatusCode::BAD_GATEWAY);
    http_resp
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    UpstreamResponse::Reqwest(reqwest::Response::from(http_resp))
}

// ---------------------------------------------------------------------------
// Refresh flow
// ---------------------------------------------------------------------------

async fn refresh_kimi_token(
    pool: &ClientPool,
    refresh_token: &str,
    proxy: Option<&ProxyTarget>,
) -> Result<(String, String), KimiWebExecutorError> {
    let url = format!("{KIMI_BASE}{KIMI_REFRESH_PATH}");
    let client = pool.get("kimi-web", proxy)?;
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {refresh_token}"))
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(ORIGIN, KIMI_BASE)
        .header(REFERER, format!("{KIMI_BASE}/"))
        .header(USER_AGENT, KIMI_USER_AGENT)
        .send()
        .await?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        return Err(KimiWebExecutorError::RefreshFailed(format!(
            "HTTP {status}: {body}"
        )));
    }
    let json: Value = resp.json().await?;
    let access = json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            KimiWebExecutorError::RefreshFailed(
                "Invalid response from Kimi: missing access_token".to_string(),
            )
        })?
        .to_string();
    let new_refresh = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or(refresh_token)
        .to_string();
    Ok((access, new_refresh))
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

impl KimiWebExecutor {
    pub fn new(pool: Arc<ClientPool>) -> Self {
        Self { pool }
    }

    pub async fn execute_request(
        &self,
        request: KimiWebExecutionRequest,
    ) -> Result<KimiWebExecutorResponse, KimiWebExecutorError> {
        let url = format!("{KIMI_BASE}{KIMI_CHAT_PATH}");

        let model_config = resolve_kimi_model(&request.model).ok_or_else(|| {
            KimiWebExecutorError::UnsupportedModel(format!(
                "Model '{}' is not in the Kimi web registry. Supported: {}",
                request.model,
                kimi_model_registry()
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        // Tools: reject non-empty (Kimi web does not support OpenAI tool calling)
        let tools = request.body.get("tools");
        if let Some(arr) = tools.and_then(Value::as_array) {
            if !arr.is_empty() {
                return Ok(KimiWebExecutorResponse {
                    response: json_error(
                        400,
                        "Kimi Web does not support OpenAI function tools",
                        "invalid_request",
                        Some("tools_not_supported"),
                    ),
                    url: url.clone(),
                    headers: HeaderMap::new(),
                    transformed_body: request.body.clone(),
                    transport: TransportKind::Reqwest,
                });
            }
        }
        let functions = request.body.get("functions");
        if let Some(arr) = functions.and_then(Value::as_array) {
            if !arr.is_empty() {
                return Ok(KimiWebExecutorResponse {
                    response: json_error(
                        400,
                        "Kimi Web does not support legacy function tools",
                        "invalid_request",
                        Some("tools_not_supported"),
                    ),
                    url: url.clone(),
                    headers: HeaderMap::new(),
                    transformed_body: request.body.clone(),
                    transport: TransportKind::Reqwest,
                });
            }
        }

        // Extract access/refresh tokens from raw api_key + provider_specific_data
        let raw_api_key = request.credentials.api_key.as_deref().unwrap_or("");
        let mut access_token = extract_kimi_access_token(raw_api_key);
        if access_token.is_empty() {
            if let Some(at) = request.credentials.access_token.as_deref() {
                access_token = extract_kimi_access_token(at);
            }
        }
        if access_token.is_empty() {
            return Ok(KimiWebExecutorResponse {
                response: json_error(
                    400,
                    "Missing Kimi access_token — log in at www.kimi.ai and capture access_token from localStorage.",
                    "missing_credentials",
                    Some("missing_access_token"),
                ),
                url: url.clone(),
                headers: HeaderMap::new(),
                transformed_body: request.body.clone(),
                transport: TransportKind::Reqwest,
            });
        }

        let mut refresh_token = request
            .credentials
            .refresh_token
            .as_deref()
            .map(extract_kimi_refresh_token)
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        if refresh_token.is_empty() {
            refresh_token = request
                .credentials
                .provider_specific_data
                .get("refreshToken")
                .and_then(Value::as_str)
                .map(extract_kimi_refresh_token)
                .unwrap_or_default();
        }
        if refresh_token.is_empty() {
            // Fallback: try to extract from raw api_key if it's a JSON dump
            refresh_token = extract_kimi_refresh_token(raw_api_key);
        }

        let messages_vec = request
            .body
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (prompt, system_prompt) = fold_messages(&messages_vec)
            .map_err(|e| KimiWebExecutorError::ConnectProtocol(format!("Invalid messages: {e}")))?;
        if prompt.is_empty() {
            return Ok(KimiWebExecutorResponse {
                response: json_error(
                    400,
                    "Kimi Web requires a non-empty user message",
                    "invalid_request",
                    Some("empty_prompt"),
                ),
                url: url.clone(),
                headers: HeaderMap::new(),
                transformed_body: request.body.clone(),
                transport: TransportKind::Reqwest,
            });
        }

        let reasoning_effort = normalize_reasoning_effort(
            request.body.get("reasoning_effort").and_then(Value::as_str),
            model_config,
        );

        let request_body = build_request_body(
            &prompt,
            &system_prompt,
            model_config,
            reasoning_effort.as_deref(),
        );
        let transformed_body = request_body.clone();

        let json_bytes = serde_json::to_vec(&request_body)?;
        let framed = encode_connect_frame(&json_bytes);

        let client = self.pool.get("kimi-web", request.proxy.as_ref())?;
        let headers = build_headers(&access_token)?;

        let mut current_refresh_token = refresh_token.clone();
        let _ = current_refresh_token.clone();
        let mut last_response: Result<reqwest::Response, reqwest::Error> = client
            .post(&url)
            .headers(headers.clone())
            .body(framed.clone())
            .send()
            .await;
        // Re-bind for readability in the refresh branch below.
        let mut current_access_token = access_token.clone();

        // 401 → try refresh once → retry once
        if let Ok(ref resp) = last_response {
            if resp.status().as_u16() == 401 && !current_refresh_token.is_empty() {
                match refresh_kimi_token(&self.pool, &current_refresh_token, request.proxy.as_ref())
                    .await
                {
                    Ok((new_access, new_refresh)) => {
                        current_access_token = new_access;
                        current_refresh_token = new_refresh;
                        let headers2 = build_headers(&current_access_token)?;
                        last_response = client
                            .post(&url)
                            .headers(headers2.clone())
                            .body(framed.clone())
                            .send()
                            .await;
                    }
                    Err(e) => {
                        // Refresh failed; surface original 401 with hint
                        return Ok(KimiWebExecutorResponse {
                            response: json_error(
                                401,
                                &format!(
                                    "Kimi access_token expired and refresh failed: {e}. \
                                 Re-paste a fresh access_token from kimi.ai localStorage."
                                ),
                                "upstream_error",
                                Some("HTTP_401"),
                            ),
                            url: url.clone(),
                            headers,
                            transformed_body,
                            transport: TransportKind::Reqwest,
                        });
                    }
                }
            }
            let _ = current_access_token;
            let _ = current_refresh_token;
        }

        let response = last_response?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let err_body = response.text().await.unwrap_or_default();
            let (msg, code) = match status {
                401 | 403 => (
                    "Kimi auth failed — access_token may be expired and no refresh_token is configured. \
                     Re-paste from kimi.ai localStorage or include refresh_token."
                        .to_string(),
                    format!("HTTP_{status}"),
                ),
                429 => ("Kimi rate limited. Wait and retry.".to_string(), format!("HTTP_{status}")),
                _ => (
                    format!("Kimi returned HTTP {status}: {err_body}"),
                    format!("HTTP_{status}"),
                ),
            };
            return Ok(KimiWebExecutorResponse {
                response: json_error(status, &msg, "upstream_error", Some(&code)),
                url: url.clone(),
                headers,
                transformed_body,
                transport: TransportKind::Reqwest,
            });
        }

        if request.stream {
            let converted = stream_kimi_to_sse(response, &request.model).await?;
            Ok(KimiWebExecutorResponse {
                response: converted,
                url: url.clone(),
                headers,
                transformed_body,
                transport: TransportKind::Reqwest,
            })
        } else {
            let (content, reasoning) = collect_kimi_content(response).await?;
            let cid = format!(
                "chatcmpl-kimi-{}",
                &Uuid::new_v4().simple().to_string()[..12]
            );
            let created = chrono::Utc::now().timestamp();
            let mut message = json!({ "role": "assistant", "content": content });
            if !reasoning.is_empty() {
                message["reasoning_content"] = Value::String(reasoning);
            }
            let body = json!({
                "id": cid,
                "object": "chat.completion",
                "created": created,
                "model": request.model,
                "system_fingerprint": Value::Null,
                "choices": [{
                    "index": 0,
                    "message": message,
                    "finish_reason": "stop",
                    "logprobs": Value::Null,
                }],
                "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 },
            });
            let bytes = serde_json::to_vec(&body).unwrap_or_default();
            let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
            *http_resp.status_mut() = reqwest::StatusCode::OK;
            http_resp
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            Ok(KimiWebExecutorResponse {
                response: UpstreamResponse::Reqwest(reqwest::Response::from(http_resp)),
                url: url.clone(),
                headers,
                transformed_body,
                transport: TransportKind::Reqwest,
            })
        }
    }
}

fn build_request_body(
    prompt: &str,
    system_prompt: &str,
    model_config: &KimiModelConfig,
    reasoning_effort: Option<&str>,
) -> Value {
    let mut options = json!({
        "thinking": true,
        "enable_plugin": false,
    });
    if !system_prompt.is_empty() {
        options["system_prompt"] = Value::String(system_prompt.to_string());
    }
    if let Some(effort) = reasoning_effort {
        options["reasoning_effort"] = Value::String(effort.to_string());
    }
    let mut body = json!({
        "chat_id": "",
        "scenario": model_config.scenario,
        "tools": [],
        "message": {
            "id": "",
            "parent_id": "",
            "children_message_ids": [],
            "role": "user",
            "blocks": [{
                "id": "",
                "message_id": "",
                "text": { "content": prompt }
            }],
            "scenario": model_config.scenario,
            "labels": [],
            "references": [],
            "is_goal": false,
        },
        "options": options,
        "project_id": "",
    });
    if let Some(kpi) = model_config.kimi_plus_id {
        body["kimiplus_id"] = Value::String(kpi.to_string());
    }
    body
}

fn build_headers(access_token: &str) -> Result<HeaderMap, KimiWebExecutorError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}")).map_err(|_| {
            KimiWebExecutorError::InvalidCredentials("Invalid auth header".to_string())
        })?,
    );
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/connect+json"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(KIMI_USER_AGENT));
    headers.insert(ORIGIN, HeaderValue::from_static(KIMI_BASE));
    headers.insert(REFERER, HeaderValue::from_static("https://www.kimi.ai/"));
    headers.insert("connect-protocol-version", HeaderValue::from_static("1"));
    Ok(headers)
}

// Silence unused-import warning for base64 (kept for parity w/ OmniRoute helpers if needed later)
#[allow(dead_code)]
fn _base64_keepalive(_b: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_kimi_access_token_handles_all_formats() {
        // raw JWT
        assert_eq!(
            extract_kimi_access_token("eyJhbGc.payload.sig"),
            "eyJhbGc.payload.sig"
        );
        // JSON dump
        assert_eq!(
            extract_kimi_access_token(r#"{"access_token":"abc.def.ghi","refresh_token":"xyz"}"#),
            "abc.def.ghi"
        );
        // Bearer prefix
        assert_eq!(
            extract_kimi_access_token("Bearer eyJhbGc.payload.sig"),
            "eyJhbGc.payload.sig"
        );
        // authorization: Bearer (header-style)
        assert_eq!(
            extract_kimi_access_token("authorization: Bearer eyJhbGc.payload.sig"),
            "eyJhbGc.payload.sig"
        );
        // access_token= kv
        assert_eq!(
            extract_kimi_access_token("access_token=eyJhbGc.payload.sig"),
            "eyJhbGc.payload.sig"
        );
        // kimi-auth= kv
        assert_eq!(
            extract_kimi_access_token("kimi-auth=eyJhbGc.payload.sig"),
            "eyJhbGc.payload.sig"
        );
        // empty
        assert_eq!(extract_kimi_access_token(""), "");
    }

    #[test]
    fn extract_kimi_refresh_token_handles_formats() {
        assert_eq!(
            extract_kimi_refresh_token(r#"{"access_token":"a","refresh_token":"r1"}"#),
            "r1"
        );
        assert_eq!(extract_kimi_refresh_token("refresh_token=r1"), "r1");
        assert_eq!(extract_kimi_refresh_token("; refresh_token=r2;"), "r2");
        assert_eq!(extract_kimi_refresh_token(""), "");
        // no refresh_token present → empty
        assert_eq!(extract_kimi_refresh_token(r#"{"access_token":"a"}"#), "");
    }

    #[test]
    fn normalize_reasoning_effort_picks_valid_enum() {
        let cfg = resolve_kimi_model("k3").unwrap();
        assert_eq!(
            normalize_reasoning_effort(Some("low"), cfg).as_deref(),
            Some("REASONING_EFFORT_LOW")
        );
        assert_eq!(
            normalize_reasoning_effort(Some("REASONING_EFFORT_NONE"), cfg).as_deref(),
            Some("REASONING_EFFORT_NONE")
        );
        // Default when none requested
        assert_eq!(
            normalize_reasoning_effort(None, cfg).as_deref(),
            Some("REASONING_EFFORT_NONE")
        );
        // Invalid → None
        assert_eq!(normalize_reasoning_effort(Some("medium"), cfg), None);
    }

    #[test]
    fn connect_frame_roundtrip() {
        let payload = b"{\"foo\":1}";
        let framed = encode_connect_frame(payload);
        let (consumed, frame) = decode_connect_frame(&framed, 0).unwrap();
        assert_eq!(consumed, framed.len());
        assert_eq!(frame.flags, 0x00);
        assert_eq!(frame.message.unwrap(), json!({"foo": 1}));
    }

    #[test]
    fn decode_connect_frame_handles_incomplete() {
        let payload = b"{\"foo\":1}";
        let framed = encode_connect_frame(payload);
        // Truncated
        let (consumed, _frame) = decode_connect_frame(&framed[..6], 0).unwrap();
        assert_eq!(consumed, 0);
    }

    #[test]
    fn extract_delta_text_and_think() {
        let m1 =
            json!({"op":"append","mask":"block.text.content","block":{"text":{"content":"hi"}}});
        assert_eq!(
            extract_kimi_delta(Some(&m1)),
            Some(("text", "hi".to_string()))
        );

        let m2 =
            json!({"op":"append","mask":"block.think.content","block":{"think":{"content":"r1"}}});
        assert_eq!(
            extract_kimi_delta(Some(&m2)),
            Some(("think", "r1".to_string()))
        );

        // initial set with mask=block.text carries initial content
        let m3 = json!({"op":"set","mask":"block.text","block":{"text":{"content":"seed"}}});
        assert_eq!(
            extract_kimi_delta(Some(&m3)),
            Some(("text", "seed".to_string()))
        );

        // heartbeat / unknown mask → None
        let m4 = json!({"op":"append","mask":"block.meta.id","block":{"meta":{"id":"abc"}}});
        assert_eq!(extract_kimi_delta(Some(&m4)), None);

        assert_eq!(extract_kimi_delta(None), None);
    }

    #[test]
    fn end_stream_error_format() {
        let f = ConnectFrame {
            flags: 0x02,
            message: Some(
                json!({"error": {"code": "unauthenticated", "message": "token expired"}}),
            ),
        };
        assert_eq!(
            end_stream_error(&f).as_deref(),
            Some("unauthenticated: token expired")
        );
        // flags without 0x02 → no error
        let f2 = ConnectFrame {
            flags: 0x00,
            message: f.message.clone(),
        };
        assert_eq!(end_stream_error(&f2), None);
        // error missing
        let f3 = ConnectFrame {
            flags: 0x02,
            message: Some(json!({})),
        };
        assert_eq!(end_stream_error(&f3), None);
    }

    #[test]
    fn fold_messages_basic() {
        let msgs = vec![
            json!({"role":"system","content":"be helpful"}),
            json!({"role":"user","content":"hi"}),
        ];
        let (prompt, system) = fold_messages(&msgs).unwrap();
        assert_eq!(prompt, "hi");
        assert_eq!(system, "be helpful");

        // Multi-turn rolls into transcript
        let msgs = vec![
            json!({"role":"user","content":"u1"}),
            json!({"role":"assistant","content":"a1"}),
            json!({"role":"user","content":"u2"}),
        ];
        let (prompt, _) = fold_messages(&msgs).unwrap();
        assert_eq!(prompt, "u1\n\nAssistant: a1\n\nUser: u2");
    }

    #[test]
    fn fold_messages_rejects_tool_calls() {
        let msgs = vec![json!({"role":"assistant","content":"x","tool_calls":[{"id":"1"}]})];
        assert!(fold_messages(&msgs).is_err());
    }

    #[test]
    fn decode_connect_frame_skips_compressed_and_unknown_flags() {
        // Compressed frame: consumed + skipped, never fatal.
        let payload = b"{\"op\":\"append\"}";
        let mut framed = vec![0x01];
        framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        framed.extend_from_slice(payload);
        let (consumed, frame) = decode_connect_frame(&framed, 0).unwrap();
        assert_eq!(consumed, framed.len());
        assert!(frame.message.is_none());

        // Unknown flag bits are ignored; valid JSON still parses.
        let mut framed2 = vec![0x08];
        framed2.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        framed2.extend_from_slice(payload);
        let (consumed2, frame2) = decode_connect_frame(&framed2, 0).unwrap();
        assert_eq!(consumed2, framed2.len());
        assert_eq!(frame2.message.unwrap(), json!({"op":"append"}));

        // Malformed JSON data frame is skipped, never fatal.
        let bad = b"not-json{";
        let mut framed3 = vec![0x00];
        framed3.extend_from_slice(&(bad.len() as u32).to_be_bytes());
        framed3.extend_from_slice(bad);
        let (consumed3, frame3) = decode_connect_frame(&framed3, 0).unwrap();
        assert_eq!(consumed3, framed3.len());
        assert!(frame3.message.is_none());
    }
}
