//! DeepSeek Web executor — `chat.deepseek.com` session-based chat API.
//!
//! Port of OmniRoute `open-sse/executors/deepseek-web.ts`:
//! - OpenAI chat body → single prompt string
//! - DeepSeekHashV1 PoW (Keccak-p[1600,23] sponge, 23-round SHA3-256 variant)
//! - Browser-fingerprint headers (X-Client-Bundle-Id, X-Client-Version, etc.)
//! - SSE stream with patch-based fragment updates → OpenAI SSE chunks
//! - Search citation cleanup + FINISHED drain window
//! - HTTP-200 `code !== 0` error mapping; 401/403 re-acquire + retry once
//!
//! Auth: user's `userToken` (from chat.deepseek.com localStorage) — may be raw
//! string or JSON-wrapped `{"value":"..."}` — stored in `credentials.api_key`.
//! Exchanged for a short-lived `accessToken` via `/api/v0/users/current`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use futures_util::StreamExt;
use hyper::http;
use keccak::Keccak;
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use reqwest::Body as ReqwestBody;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::proxy::ProxyTarget;
use crate::types::ProviderConnection;

use super::{ClientPool, TransportKind, UpstreamResponse};

const DEEPSEEK_BASE: &str = "https://chat.deepseek.com";
const DEEPSEEK_API_BASE: &str = "https://chat.deepseek.com/api";
const DEEPSEEK_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

const POW_ALGORITHM: &str = "DeepSeekHashV1";
const POW_MAX_DIFFICULTY: u32 = 250_000;
const POW_MAX_SALT_LEN: usize = 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct DeepSeekWebExecutionRequest {
    pub model: String,
    pub body: Value,
    pub stream: bool,
    pub credentials: ProviderConnection,
    pub proxy: Option<ProxyTarget>,
}

#[derive(Debug)]
pub enum DeepSeekWebExecutorError {
    MissingCredentials(String),
    InvalidCredentials(String),
    Serialize(serde_json::Error),
    Request(reqwest::Error),
    PoWFailed(String),
}

impl From<reqwest::Error> for DeepSeekWebExecutorError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<serde_json::Error> for DeepSeekWebExecutorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialize(error)
    }
}

impl std::fmt::Display for DeepSeekWebExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredentials(p) => write!(f, "Missing credentials for {p}"),
            Self::InvalidCredentials(msg) => write!(f, "Invalid credentials: {msg}"),
            Self::Serialize(e) => write!(f, "Serialization error: {e}"),
            Self::Request(e) => write!(f, "Request error: {e}"),
            Self::PoWFailed(msg) => write!(f, "PoW failed: {msg}"),
        }
    }
}

impl std::error::Error for DeepSeekWebExecutorError {}

pub struct DeepSeekWebExecutorResponse {
    pub response: UpstreamResponse,
    pub url: String,
    pub headers: HeaderMap,
    pub transformed_body: Value,
    pub transport: TransportKind,
}

impl std::fmt::Debug for DeepSeekWebExecutorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekWebExecutorResponse")
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("transformed_body", &self.transformed_body)
            .field("transport", &self.transport)
            .finish()
    }
}

pub struct DeepSeekWebExecutor {
    pool: Arc<ClientPool>,
}

// ---------------------------------------------------------------------------
// Fake browser headers / cookie
// ---------------------------------------------------------------------------

fn fake_headers_json() -> Value {
    json!({
        "Accept": "*/*",
        "Accept-Encoding": "gzip, deflate, br, zstd",
        "Accept-Language": "en-US,en;q=0.9",
        "Origin": DEEPSEEK_BASE,
        "Referer": format!("{}/", DEEPSEEK_BASE),
        "User-Agent": DEEPSEEK_USER_AGENT,
        "X-Client-Bundle-Id": "com.deepseek.chat",
        "X-Client-Locale": "en-US",
        "X-Client-Platform": "web",
        "X-Client-Version": "2.0.0",
    })
}

fn generate_fake_cookie() -> String {
    let mut rng = rand::thread_rng();
    let ts = chrono::Utc::now().timestamp_millis();
    let hex = |rng: &mut rand::rngs::ThreadRng, n: usize| -> String {
        (0..n)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect()
    };
    let uid = |rng: &mut rand::rngs::ThreadRng| -> String {
        format!(
            "{:x}{:x}{:x}{:x}-{:x}{:x}{:x}{:x}-4{:x}{:x}{:x}-{:x}{:x}{:x}-{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}",
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
        )
    };
    format!(
        "intercom-HWWAFSESTIME={}; HWWAFSESID={}; Hm_lvt_{}={}; _frid={}",
        ts,
        hex(&mut rng, 18),
        uid(&mut rng),
        ts / 1000,
        uid(&mut rng)
    )
}

// ---------------------------------------------------------------------------
// Access token cache (userToken → accessToken, 1h TTL)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at: i64,
}

static ACCESS_TOKEN_CACHE: once_cell::sync::Lazy<
    tokio::sync::RwLock<HashMap<String, CachedToken>>,
> = once_cell::sync::Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

async fn acquire_access_token(
    pool: &ClientPool,
    user_token: &str,
    proxy: Option<&ProxyTarget>,
) -> Result<String, DeepSeekWebExecutorError> {
    let now = chrono::Utc::now().timestamp();
    {
        let cache = ACCESS_TOKEN_CACHE.read().await;
        if let Some(cached) = cache.get(user_token) {
            if cached.expires_at > now + 60 {
                return Ok(cached.token.clone());
            }
        }
    }

    let client = pool.get("deepseek-web", proxy)?;
    let resp = client
        .get(format!("{DEEPSEEK_API_BASE}/v0/users/current"))
        .header(AUTHORIZATION, format!("Bearer {user_token}"))
        .headers(headers_from_json(&fake_headers_json()))
        .send()
        .await?;

    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        ACCESS_TOKEN_CACHE.write().await.remove(user_token);
        return Err(DeepSeekWebExecutorError::InvalidCredentials(
            "DeepSeek token invalid or expired — get a fresh userToken from chat.deepseek.com localStorage."
                .to_string(),
        ));
    }
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        return Err(DeepSeekWebExecutorError::InvalidCredentials(format!(
            "DeepSeek token exchange failed (HTTP {status}): {body}"
        )));
    }

    let json: Value = resp.json().await?;
    if let Some(code) = json.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let msg = json
                .get("msg")
                .or_else(|| json.pointer("/data/biz_msg"))
                .and_then(Value::as_str)
                .unwrap_or("Unknown error");
            ACCESS_TOKEN_CACHE.write().await.remove(user_token);
            return Err(DeepSeekWebExecutorError::InvalidCredentials(format!(
                "DeepSeek rejected token: {msg}"
            )));
        }
    }
    let biz_data = json.pointer("/data/biz_data").cloned().unwrap_or(Value::Null);
    let access_token = biz_data
        .get("token")
        .and_then(Value::as_str)
        .or_else(|| json.pointer("/data/token").and_then(Value::as_str))
        .ok_or_else(|| {
            DeepSeekWebExecutorError::InvalidCredentials(
                "No access_token in DeepSeek response".to_string(),
            )
        })?
        .to_string();

    {
        let mut cache = ACCESS_TOKEN_CACHE.write().await;
        cache.insert(
            user_token.to_string(),
            CachedToken {
                token: access_token.clone(),
                expires_at: now + 3600,
            },
        );
    }
    Ok(access_token)
}

// ---------------------------------------------------------------------------
// PoW: DeepSeekHashV1 (SHA3-256 sponge with 23 KECCAK-p[1600] rounds)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PowChallenge {
    algorithm: String,
    challenge: String,
    salt: String,
    #[serde(default)]
    signature: String,
    difficulty: u32,
    expire_at: i64,
    #[serde(default)]
    target_path: String,
}

/// DeepSeekHashV1 digest: SHA3-256 sponge with KECCAK-p[1600,23] (last 23 rounds).
/// `keccak::Keccak::with_p1600::<23>` runs rounds 0..22 — we need rounds 1..23
/// (FIPS 202 uses 24 rounds, indices 0..23; DeepSeekHashV1 skips the first).
/// The RustCrypto keccak 0.2 backend runs the first `ROUNDS` of FIPS 202, so we
/// pass 23 to skip only the first round (index 0) and match the reference.
fn deepseek_hash_v1(input: &str) -> [u8; 32] {
    let mut state = [0u64; 25];
    let bytes = input.as_bytes();
    let rate = 136usize;
    let mut offset = 0usize;
    let keccak = Keccak::default();
    while offset + rate <= bytes.len() {
        for i in 0..rate {
            state[i / 8] ^= (bytes[offset + i] as u64) << ((i % 8) * 8);
        }
        keccak.with_p1600::<23>(|f| f(&mut state));
        offset += rate;
    }
    let remaining = bytes.len() - offset;
    for i in 0..remaining {
        state[i / 8] ^= (bytes[offset + i] as u64) << ((i % 8) * 8);
    }
    // SHA-3 padding: 0x06 ... 0x80
    state[remaining / 8] ^= 0x06u64 << ((remaining % 8) * 8);
    state[(rate - 1) / 8] ^= 0x80u64 << 56;
    keccak.with_p1600::<23>(|f| f(&mut state));
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = (state[i / 8] >> ((i % 8) * 8)) as u8;
    }
    out
}

fn solve_pow(prefix: &str, challenge_hex: &str, difficulty: u32) -> i64 {
    let target = match hex::decode(challenge_hex.to_lowercase()) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return -1,
    };
    for nonce in 0..difficulty as i64 {
        let input = format!("{prefix}{nonce}");
        let hash = deepseek_hash_v1(&input);
        if hash == target {
            return nonce;
        }
        if nonce % 10000 == 0 && nonce > 0 {
            // Yield periodically so the runtime isn't blocked on a single request
            std::thread::yield_now();
        }
    }
    -1
}

async fn get_pow_response(
    pool: &ClientPool,
    access_token: &str,
    proxy: Option<&ProxyTarget>,
) -> Result<String, DeepSeekWebExecutorError> {
    let client = pool.get("deepseek-web", proxy)?;
    let resp = client
        .post(format!("{DEEPSEEK_API_BASE}/v0/chat/create_pow_challenge"))
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/json")
        .headers(headers_from_json(&fake_headers_json()))
        .json(&json!({ "target_path": "/api/v0/chat/completion" }))
        .send()
        .await?;

    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        return Err(DeepSeekWebExecutorError::PoWFailed(format!(
            "PoW challenge HTTP {status}: {body}"
        )));
    }

    let json: Value = resp.json().await?;
    let biz_data = json.pointer("/data/biz_data").cloned().unwrap_or(Value::Null);
    let challenge: PowChallenge = serde_json::from_value(biz_data.get("challenge").cloned().unwrap_or(Value::Null))
        .map_err(|e| DeepSeekWebExecutorError::PoWFailed(format!("Invalid PoW challenge: {e}")))?;

    if challenge.algorithm != POW_ALGORITHM {
        return Err(DeepSeekWebExecutorError::PoWFailed(format!(
            "Unsupported PoW algorithm: {}",
            challenge.algorithm
        )));
    }
    if challenge.salt.is_empty() || challenge.salt.len() > POW_MAX_SALT_LEN {
        return Err(DeepSeekWebExecutorError::PoWFailed(format!(
            "PoW salt length out of range: {}",
            challenge.salt.len()
        )));
    }
    let difficulty = if challenge.difficulty == 0 {
        1
    } else {
        challenge.difficulty.min(POW_MAX_DIFFICULTY)
    };
    let prefix = format!("{}_{}_", challenge.salt, challenge.expire_at);
    let target_path = if challenge.target_path.is_empty() {
        "/api/v0/chat/completion".to_string()
    } else {
        challenge.target_path.clone()
    };

    // Run the CPU-bound PoW on a blocking thread (capped at 250k iterations; ~1s on modern hw)
    let prefix_for_task = prefix.clone();
    let challenge_for_task = challenge.challenge.clone();
    let answer = tokio::task::spawn_blocking(move || solve_pow(&prefix_for_task, &challenge_for_task, difficulty))
        .await
        .map_err(|e| DeepSeekWebExecutorError::PoWFailed(format!("PoW join error: {e}")))?;
    if answer < 0 {
        return Err(DeepSeekWebExecutorError::PoWFailed(format!(
            "Could not solve PoW in {difficulty} iterations"
        )));
    }

    let payload = json!({
        "algorithm": challenge.algorithm,
        "challenge": challenge.challenge,
        "salt": challenge.salt,
        "answer": answer,
        "signature": challenge.signature,
        "target_path": target_path,
    });
    let b64 = base64::engine::general_purpose::STANDARD.encode(payload.to_string().as_bytes());
    Ok(b64)
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

async fn create_session(
    pool: &ClientPool,
    access_token: &str,
    proxy: Option<&ProxyTarget>,
) -> Result<String, DeepSeekWebExecutorError> {
    let client = pool.get("deepseek-web", proxy)?;
    let resp = client
        .post(format!("{DEEPSEEK_API_BASE}/v0/chat_session/create"))
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/json")
        .header(COOKIE, generate_fake_cookie())
        .headers(headers_from_json(&fake_headers_json()))
        .body("{}")
        .send()
        .await?;

    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        return Err(DeepSeekWebExecutorError::InvalidCredentials(format!(
            "DeepSeek session creation HTTP {status}: {body}"
        )));
    }

    let json: Value = resp.json().await?;
    let biz_data = json.pointer("/data/biz_data").cloned().unwrap_or(Value::Null);
    let session_id = biz_data
        .pointer("/chat_session/id")
        .and_then(Value::as_str)
        .or_else(|| biz_data.get("id").and_then(Value::as_str))
        .ok_or_else(|| {
            DeepSeekWebExecutorError::InvalidCredentials(format!(
                "No session id in DeepSeek response: code={}",
                json.get("code").and_then(Value::as_i64).unwrap_or(-1)
            ))
        })?
        .to_string();
    Ok(session_id)
}

async fn delete_session(
    pool: &ClientPool,
    access_token: &str,
    session_id: &str,
    proxy: Option<&ProxyTarget>,
) {
    if let Ok(client) = pool.get("deepseek-web", proxy) {
        let _ = client
            .post(format!("{DEEPSEEK_API_BASE}/v0/chat_session/delete"))
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .header(CONTENT_TYPE, "application/json")
            .headers(headers_from_json(&fake_headers_json()))
            .json(&json!({ "chat_session_id": session_id }))
            .send()
            .await;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn headers_from_json(map: &Value) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(obj) = map.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                if let (Ok(name), Ok(value)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(s),
                ) {
                    headers.insert(name, value);
                }
            }
        }
    }
    headers
}

pub fn extract_user_token(api_key: Option<&str>) -> Option<String> {
    let raw = api_key?;
    if raw.is_empty() {
        return None;
    }
    // JSON-wrapped tokens: {"value":"..."}
    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        if let Some(v) = parsed.get("value").and_then(Value::as_str) {
            return Some(v.to_string());
        }
    }
    Some(raw.to_string())
}

fn is_thinking_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("r1") || m.contains("think") || m.contains("reason")
}

fn is_search_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("search")
}

fn is_expert_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("pro") || m.contains("expert")
}

fn resolve_model_options(model: &str, body: &Value) -> (String, bool, bool) {
    let model_type = if is_expert_model(model) { "expert" } else { "default" };
    let thinking = is_thinking_model(model)
        || body.get("thinking_enabled").and_then(Value::as_bool) == Some(true)
        || body.get("thinking").and_then(Value::as_bool) == Some(true)
        || body.get("reasoning_effort").map(|v| !v.is_null()).unwrap_or(false);
    let search = is_search_model(model)
        || body.get("search_enabled").and_then(Value::as_bool) == Some(true)
        || body.get("search").and_then(Value::as_bool) == Some(true)
        || body.get("web_search").and_then(Value::as_bool) == Some(true);
    (model_type.to_string(), thinking, search)
}

fn extract_message_text(content: &Value) -> String {
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
    }
    content.as_str().unwrap_or("").to_string()
}

fn build_prompt(messages: &[Value], history_window: usize) -> String {
    let mut system_parts: Vec<String> = Vec::new();
    let mut conversation: Vec<(String, String)> = Vec::new();
    let mut last_user = String::new();
    for m in messages {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
        let text = extract_message_text(m.get("content").unwrap_or(&Value::Null)).trim().to_string();
        if text.is_empty() {
            continue;
        }
        match role {
            "system" => system_parts.push(text),
            "user" | "assistant" => {
                conversation.push((role.to_string(), text.clone()));
                if role == "user" {
                    last_user = text;
                }
            }
            "tool" => {
                let name = m
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .or_else(|| m.get("name").and_then(Value::as_str))
                    .unwrap_or("tool");
                conversation.push(("tool".to_string(), format!("({name}) {text}")));
            }
            _ => {}
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if !system_parts.is_empty() {
        parts.push(system_parts.join("\n\n"));
    }
    let effective_window = if history_window > 0 {
        history_window
    } else if conversation.len() > 1 {
        20
    } else {
        0
    };
    if effective_window > 0 && conversation.len() > 1 {
        let recent = conversation
            .iter()
            .rev()
            .take(effective_window)
            .rev()
            .map(|(role, text)| match role.as_str() {
                "assistant" => format!("Assistant: {text}"),
                "tool" => format!("Tool result {text}"),
                _ => format!("User: {text}"),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        parts.push(recent);
    } else if !last_user.is_empty() {
        parts.push(last_user);
    }
    parts.join("\n\n")
}

// Strip search-model content artifacts from a stream chunk.
fn format_stream_content(raw: &str, is_search: bool) -> String {
    if !is_search {
        return raw.to_string();
    }
    let mut s = raw.to_string();
    // Strip search status prefixes
    for prefix in ["SEARCH", "WEB_SEARCH", "SEARCHING"] {
        if s.starts_with(prefix) {
            s = s.trim_start_matches(prefix).to_string();
        }
    }
    // Drop lone FINISHED markers
    if s.trim() == "FINISHED" {
        return String::new();
    }
    // [citation:N] -> [N]
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 9 <= bytes.len() && &bytes[i..i + 9] == b"[citation" {
            // find ':' and closing ']'
            if let Some(colon) = s[i + 9..].find(':') {
                let after = i + 9 + colon + 1;
                if let Some(close) = s[after..].find(']') {
                    result.push('[');
                    result.push_str(&s[after..after + close]);
                    result.push(']');
                    i = after + close + 1;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn append_search_citations(search_results: &[Value], is_search: bool) -> Option<String> {
    if !is_search || search_results.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (i, c) in search_results.iter().enumerate() {
        let title = c.get("title").and_then(Value::as_str).unwrap_or("");
        let url = c.get("url").and_then(Value::as_str).unwrap_or("");
        let cite_index = c.get("cite_index").and_then(Value::as_i64).unwrap_or(i as i64 + 1);
        if !title.is_empty() {
            parts.push(format!("[{cite_index}] [{title}]({url})"));
        } else if !url.is_empty() {
            parts.push(format!("[{cite_index}] {url}"));
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("\n\n**Sources:**\n{}", parts.join("\n")))
}

// ---------------------------------------------------------------------------
// OpenAI SSE chunk helper
// ---------------------------------------------------------------------------

fn sse_chunk(cid: &str, created: i64, model: &str, delta: Value, finish_reason: Option<&str>) -> String {
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
                "finish_reason": finish_reason.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
                "logprobs": Value::Null,
            }],
        }))
        .unwrap_or_default()
    )
}

// ---------------------------------------------------------------------------
// Stream transform: DeepSeek SSE → OpenAI SSE
// ---------------------------------------------------------------------------

struct SseState {
    cid: String,
    created: i64,
    model: String,
    is_thinking: bool,
    is_search: bool,
    emitted_role: bool,
    current_path: &'static str, // "thinking" | "content" | ""
    search_results: Vec<Value>,
    finished: bool,
    finish_drain_pending: bool,
    finish_drain_at: Option<std::time::Instant>,
}

const FINISH_DRAIN_MS: u64 = 80;

fn emit_chunk(state: &mut SseState, out: &mut String, delta: Value, finish: Option<&str>) {
    if !state.emitted_role && (delta.get("content").is_some()
        || delta.get("reasoning_content").is_some()
        || delta.get("role").is_some())
    {
        out.push_str(&sse_chunk(
            &state.cid,
            state.created,
            &state.model,
            json!({ "role": "assistant", "content": "" }),
            None,
        ));
        state.emitted_role = true;
    }
    out.push_str(&sse_chunk(&state.cid, state.created, &state.model, delta, finish));
}

fn path_for(state: &SseState) -> &'static str {
    if !state.current_path.is_empty() {
        return state.current_path;
    }
    if state.is_thinking {
        "thinking"
    } else if state.is_search {
        "content"
    } else {
        "content"
    }
}

fn apply_fragment_type(state: &mut SseState, frag: &Value, set_path: bool) {
    if set_path {
        let t = frag.get("type").and_then(Value::as_str).unwrap_or("").to_uppercase();
        if t == "THINK" {
            state.current_path = "thinking";
        } else if t == "ANSWER" || t == "RESPONSE" {
            state.current_path = "content";
        }
    } else {
        let t = frag.get("type").and_then(Value::as_str).unwrap_or("").to_uppercase();
        if t == "THINK" {
            state.current_path = "thinking";
        } else if t == "ANSWER" || t == "RESPONSE" {
            state.current_path = "content";
        }
    }
    let content = frag.get("content").and_then(Value::as_str).unwrap_or("");
    if content.is_empty() {
        return;
    }
    let cleaned = format_stream_content(content, state.is_search);
    if cleaned.is_empty() {
        return;
    }
    let path = path_for(state);
    let delta = if path == "thinking" {
        json!({ "reasoning_content": cleaned })
    } else {
        json!({ "content": cleaned })
    };
    emit_chunk(state, &mut String::new(), delta, None);
}

fn handle_data_line(state: &mut SseState, payload: &str, out: &mut String) {
    if payload == "[DONE]" {
        finish_stream(state, out);
        return;
    }
    let val: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    let p = val.get("p").and_then(Value::as_str).unwrap_or("");
    let o = val.get("o").and_then(Value::as_str).unwrap_or("");
    let v = val.get("v");

    // v.response wrapper: {response: {thinking_enabled, fragments}}
    if let Some(resp_obj) = v.and_then(|x| x.as_object())
        .and_then(|o| o.get("response"))
        .and_then(Value::as_object)
    {
        if resp_obj.get("thinking_enabled").and_then(Value::as_bool) == Some(true) {
            state.current_path = "thinking";
        } else if resp_obj.get("thinking_enabled").and_then(Value::as_bool) == Some(false) {
            state.current_path = "content";
        }
        if let Some(frags) = resp_obj.get("fragments").and_then(Value::as_array) {
            for frag in frags {
                apply_fragment_type(state, frag, false);
            }
        }
    }

    if p == "response/fragments" {
        if let Some(arr) = v.and_then(Value::as_array) {
            for frag in arr {
                apply_fragment_type(state, frag, true);
            }
        } else if let Some(obj) = v.and_then(Value::as_object) {
            apply_fragment_type(state, &Value::Object(obj.clone()), true);
        }
    }

    if p == "response" {
        if let Some(arr) = v.and_then(Value::as_array) {
            for entry in arr {
                if entry.get("p").and_then(Value::as_str) == Some("response")
                    && entry.pointer("/v/thinking_enabled").and_then(Value::as_bool) == Some(true)
                {
                    state.current_path = "thinking";
                }
                if let Some(arr2) = entry.get("v").and_then(Value::as_array) {
                    let joined: String = arr2
                        .iter()
                        .filter_map(|x| x.get("content").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("");
                    if !joined.is_empty() {
                        let cleaned = format_stream_content(&joined, state.is_search);
                        if !cleaned.is_empty() {
                            let path = path_for(state);
                            let delta = if path == "thinking" {
                                json!({ "reasoning_content": cleaned })
                            } else {
                                json!({ "content": cleaned })
                            };
                            emit_chunk(state, out, delta, None);
                        }
                    }
                }
            }
        }
    }

    if p == "response/search_status" {
        return;
    }

    if p == "response/search_results" {
        if let Some(arr) = v.and_then(Value::as_array) {
            if o != "BATCH" {
                state.search_results.clear();
                state.search_results.extend(arr.iter().cloned());
            } else {
                for op in arr {
                    let path = op.get("p").and_then(Value::as_str).unwrap_or("");
                    if let Some(cap) = path.find("/cite_index") {
                        if let Ok(idx) = path[..cap].parse::<usize>() {
                            if let Some(sr) = state.search_results.get_mut(idx) {
                                if let Some(ci) = op.get("v").and_then(Value::as_i64) {
                                    sr["cite_index"] = Value::Number(ci.into());
                                }
                            }
                        }
                    }
                }
            }
        }
        schedule_drain_finish(state);
        return;
    }

    if let Some(s) = v.and_then(Value::as_str) {
        let cleaned = format_stream_content(s, state.is_search);
        if !cleaned.is_empty() {
            let path = path_for(state);
            let delta = if path == "thinking" {
                json!({ "reasoning_content": cleaned })
            } else {
                json!({ "content": cleaned })
            };
            emit_chunk(state, out, delta, None);
        }
    }

    if p == "response/status" && v.and_then(Value::as_str) == Some("FINISHED") {
        schedule_drain_finish(state);
    } else if state.finish_drain_pending {
        // Late payload extends drain window
        schedule_drain_finish(state);
    }
}

fn schedule_drain_finish(state: &mut SseState) {
    state.finish_drain_pending = true;
    state.finish_drain_at = Some(std::time::Instant::now() + Duration::from_millis(FINISH_DRAIN_MS));
}

fn maybe_finish_drain(state: &mut SseState, out: &mut String) {
    if state.finish_drain_pending {
        if let Some(at) = state.finish_drain_at {
            if std::time::Instant::now() >= at {
                finish_stream(state, out);
            }
        }
    }
}

fn finish_stream(state: &mut SseState, out: &mut String) {
    if state.finished {
        return;
    }
    state.finished = true;
    state.finish_drain_pending = false;
    if let Some(citations) = append_search_citations(&state.search_results, state.is_search) {
        emit_chunk(state, out, json!({ "content": citations }), None);
    }
    emit_chunk(state, out, json!({}), Some("stop"));
    out.push_str("data: [DONE]\n\n");
}

async fn transform_deepseek_stream(
    response: reqwest::Response,
    model: &str,
    is_thinking: bool,
    is_search: bool,
) -> Result<UpstreamResponse, DeepSeekWebExecutorError> {
    let cid = format!("chatcmpl-ds-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut state = SseState {
        cid,
        created,
        model: model.to_string(),
        is_thinking,
        is_search,
        emitted_role: false,
        current_path: "",
        search_results: Vec::new(),
        finished: false,
        finish_drain_pending: false,
        finish_drain_at: None,
    };

    let mut out = String::new();
    let mut byte_stream = response.bytes_stream();
    let mut line_buf = String::new();
    while let Some(chunk_result) = byte_stream.next().await {
        let bytes = chunk_result.map_err(DeepSeekWebExecutorError::Request)?;
        line_buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(newline_pos) = line_buf.find('\n') {
            let line = line_buf[..newline_pos].trim().to_string();
            line_buf = line_buf[newline_pos + 1..].to_string();
            if line.is_empty() {
                maybe_finish_drain(&mut state, &mut out);
                continue;
            }
            let trimmed = line.strip_prefix("data:").unwrap_or(&line).trim();
            handle_data_line(&mut state, trimmed, &mut out);
            if state.finished {
                break;
            }
        }
        if state.finished {
            break;
        }
        maybe_finish_drain(&mut state, &mut out);
        if state.finished {
            break;
        }
    }
    if !state.finished {
        finish_stream(&mut state, &mut out);
    }

    let bytes = out.into_bytes();
    let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
    *http_resp.status_mut() = reqwest::StatusCode::OK;
    http_resp.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    Ok(UpstreamResponse::Reqwest(reqwest::Response::from(http_resp)))
}

async fn collect_deepseek_content_v2(
    response: reqwest::Response,
    model: &str,
    is_thinking: bool,
    is_search: bool,
) -> Result<(String, String), DeepSeekWebExecutorError> {
    #[derive(Default)]
    struct Acc {
        content: String,
        reasoning: String,
        current_path: &'static str,
        search_results: Vec<Value>,
        is_thinking: bool,
        is_search: bool,
    }
    let mut acc = Acc { is_thinking, is_search, ..Default::default() };
    let mut byte_stream = response.bytes_stream();
    let mut line_buf = String::new();
    while let Some(chunk_result) = byte_stream.next().await {
        let bytes = chunk_result.map_err(DeepSeekWebExecutorError::Request)?;
        line_buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(newline_pos) = line_buf.find('\n') {
            let line = line_buf[..newline_pos].trim().to_string();
            line_buf = line_buf[newline_pos + 1..].to_string();
            if line.is_empty() {
                continue;
            }
            let payload = line.strip_prefix("data:").unwrap_or(&line).trim();
            if payload == "[DONE]" {
                break;
            }
            let val: Value = match serde_json::from_str(payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let p = val.get("p").and_then(Value::as_str).unwrap_or("");
            let o = val.get("o").and_then(Value::as_str).unwrap_or("");
            let v = val.get("v");

            if let Some(resp_obj) = v.and_then(|x| x.as_object())
                .and_then(|o| o.get("response"))
                .and_then(Value::as_object)
            {
                if resp_obj.get("thinking_enabled").and_then(Value::as_bool) == Some(true) {
                    acc.current_path = "thinking";
                } else if resp_obj.get("thinking_enabled").and_then(Value::as_bool) == Some(false) {
                    acc.current_path = "content";
                }
                if let Some(frags) = resp_obj.get("fragments").and_then(Value::as_array) {
                    for frag in frags {
                        let t = frag.get("type").and_then(Value::as_str).unwrap_or("").to_uppercase();
                        if t == "THINK" { acc.current_path = "thinking"; }
                        else if t == "ANSWER" || t == "RESPONSE" { acc.current_path = "content"; }
                        if let Some(c) = frag.get("content").and_then(Value::as_str) {
                            let cleaned = format_stream_content(c, acc.is_search);
                            if !cleaned.is_empty() {
                                let path = if !acc.current_path.is_empty() { acc.current_path } else if acc.is_thinking { "thinking" } else { "content" };
                                if path == "thinking" { acc.reasoning.push_str(&cleaned); }
                                else { acc.content.push_str(&cleaned); }
                            }
                        }
                    }
                }
            }

            if p == "response/fragments" {
                let frags_iter: Vec<Value> = if let Some(arr) = v.and_then(Value::as_array) {
                    arr.clone()
                } else if let Some(obj) = v.and_then(Value::as_object) {
                    vec![Value::Object(obj.clone())]
                } else {
                    Vec::new()
                };
                for frag in &frags_iter {
                    let t = frag.get("type").and_then(Value::as_str).unwrap_or("").to_uppercase();
                    if t == "THINK" { acc.current_path = "thinking"; }
                    else if t == "ANSWER" || t == "RESPONSE" { acc.current_path = "content"; }
                    if let Some(c) = frag.get("content").and_then(Value::as_str) {
                        let cleaned = format_stream_content(c, acc.is_search);
                        if !cleaned.is_empty() {
                            let path = if !acc.current_path.is_empty() { acc.current_path } else if acc.is_thinking { "thinking" } else { "content" };
                            if path == "thinking" { acc.reasoning.push_str(&cleaned); }
                            else { acc.content.push_str(&cleaned); }
                        }
                    }
                }
            }

            if p == "response" {
                if let Some(arr) = v.and_then(Value::as_array) {
                    for entry in arr {
                        if entry.get("p").and_then(Value::as_str) == Some("response")
                            && entry.pointer("/v/thinking_enabled").and_then(Value::as_bool) == Some(true)
                        {
                            acc.current_path = "thinking";
                        }
                        if let Some(arr2) = entry.get("v").and_then(Value::as_array) {
                            let joined: String = arr2.iter()
                                .filter_map(|x| x.get("content").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("");
                            if !joined.is_empty() {
                                let cleaned = format_stream_content(&joined, acc.is_search);
                                let path = if !acc.current_path.is_empty() { acc.current_path } else if acc.is_thinking { "thinking" } else { "content" };
                                if path == "thinking" { acc.reasoning.push_str(&cleaned); }
                                else { acc.content.push_str(&cleaned); }
                            }
                        }
                    }
                }
            }

            if p == "response/search_results" {
                if let Some(arr) = v.and_then(Value::as_array) {
                    if o != "BATCH" {
                        acc.search_results.clear();
                        acc.search_results.extend(arr.iter().cloned());
                    }
                }
            }

            if let Some(s) = v.and_then(Value::as_str) {
                let cleaned = format_stream_content(s, acc.is_search);
                if !cleaned.is_empty() {
                    let path = if !acc.current_path.is_empty() { acc.current_path } else if acc.is_thinking { "thinking" } else { "content" };
                    if path == "thinking" { acc.reasoning.push_str(&cleaned); }
                    else { acc.content.push_str(&cleaned); }
                }
            }
        }
    }
    if let Some(citations) = append_search_citations(&acc.search_results, acc.is_search) {
        acc.content.push_str(&citations);
    }
    Ok((acc.content, acc.reasoning))
}

/// JSON error response builder
fn json_error(status: u16, message: &str, err_type: &str, code: Option<&str>) -> UpstreamResponse {
    let mut body = json!({ "error": { "message": message, "type": err_type } });
    if let Some(code) = code {
        body["error"]["code"] = Value::String(code.to_string());
    }
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
    *http_resp.status_mut() =
        reqwest::StatusCode::from_u16(status).unwrap_or(reqwest::StatusCode::BAD_GATEWAY);
    http_resp.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    UpstreamResponse::Reqwest(reqwest::Response::from(http_resp))
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

impl DeepSeekWebExecutor {
    pub fn new(pool: Arc<ClientPool>) -> Self {
        Self { pool }
    }

    pub async fn execute_request(
        &self,
        request: DeepSeekWebExecutionRequest,
    ) -> Result<DeepSeekWebExecutorResponse, DeepSeekWebExecutorError> {
        let url = format!("{DEEPSEEK_API_BASE}/v0/chat/completion");

        let user_token = extract_user_token(request.credentials.api_key.as_deref()).ok_or_else(|| {
            DeepSeekWebExecutorError::MissingCredentials(
                "deepseek-web requires a userToken in api_key (paste from chat.deepseek.com localStorage)."
                    .to_string(),
            )
        })?;

        // Step 1: Exchange userToken for accessToken
        let mut access_token =
            acquire_access_token(&self.pool, &user_token, request.proxy.as_ref()).await?;

        // Step 2: Get proof-of-work solution
        let mut pow_response =
            get_pow_response(&self.pool, &access_token, request.proxy.as_ref()).await?;

        // Step 3: Create session
        let session_id = create_session(&self.pool, &access_token, request.proxy.as_ref()).await?;

        // Build prompt from messages
        let messages_vec: Vec<Value> = request
            .body
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let history_window = request
            .credentials
            .provider_specific_data
            .get("historyWindow")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let prompt = build_prompt(&messages_vec, history_window);

        let (model_type, thinking, search) = resolve_model_options(&request.model, &request.body);

        let payload = json!({
            "chat_session_id": session_id,
            "parent_message_id": null,
            "model_type": model_type,
            "prompt": prompt,
            "ref_file_ids": [],
            "thinking_enabled": thinking,
            "search_enabled": search,
            "preempt": false,
        });

        // Build fingerprint headers
        let mut headers = fake_headers_json();
        headers["Authorization"] = Value::String(format!("Bearer {access_token}"));
        headers["Content-Type"] = Value::String("application/json".to_string());
        headers["Accept"] = Value::String("application/json, text/event-stream".to_string());
        headers["X-Ds-Pow-Response"] = Value::String(pow_response.clone());
        headers["X-Client-Timezone-Offset"] = Value::String("0".to_string());
        headers["Cookie"] = Value::String(generate_fake_cookie());
        let req_headers = headers_from_json(&headers);
        let req_headers_for_resp = req_headers.clone();

        let body_bytes = serde_json::to_vec(&payload)?;

        let client = self.pool.get("deepseek-web", request.proxy.as_ref())?;
        let response = client
            .post(&url)
            .headers(req_headers)
            .body(body_bytes)
            .send()
            .await?;

        let mut status = response.status().as_u16();
        let response = if status == 401 || status == 403 {
            // Token may have rotated; clear cache, re-acquire, retry once
            ACCESS_TOKEN_CACHE.write().await.remove(&user_token);
            access_token = acquire_access_token(&self.pool, &user_token, request.proxy.as_ref()).await?;
            pow_response = get_pow_response(&self.pool, &access_token, request.proxy.as_ref()).await?;
            let session_id_new =
                create_session(&self.pool, &access_token, request.proxy.as_ref()).await?;
            delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;
            // session id replaced below
            let mut headers2 = fake_headers_json();
            headers2["Authorization"] = Value::String(format!("Bearer {access_token}"));
            headers2["Content-Type"] = Value::String("application/json".to_string());
            headers2["Accept"] = Value::String("application/json, text/event-stream".to_string());
            headers2["X-Ds-Pow-Response"] = Value::String(pow_response.clone());
            headers2["X-Client-Timezone-Offset"] = Value::String("0".to_string());
            headers2["Cookie"] = Value::String(generate_fake_cookie());
            let req_headers2 = headers_from_json(&headers2);
            let payload2 = json!({
                "chat_session_id": session_id_new,
                "parent_message_id": null,
                "model_type": model_type,
                "prompt": prompt,
                "ref_file_ids": [],
                "thinking_enabled": thinking,
                "search_enabled": search,
                "preempt": false,
            });
            let body_bytes2 = serde_json::to_vec(&payload2)?;
            let resp2 = client
                .post(&url)
                .headers(req_headers2.clone())
                .body(body_bytes2)
                .send()
                .await?;
            status = resp2.status().as_u16();
            // Replace session id for cleanup
            let _ = session_id_new;
            resp2
        } else {
            response
        };

        if !(200..300).contains(&status) {
            let err_body = response.text().await.unwrap_or_default();
            let (msg, code_str) = match status {
                401 | 403 => (
                    "DeepSeek auth failed — userToken may be expired. Re-paste from chat.deepseek.com localStorage.".to_string(),
                    format!("HTTP_{status}"),
                ),
                429 => ("DeepSeek rate limited. Wait and retry.".to_string(), format!("HTTP_{status}")),
                _ => (
                    format!("DeepSeek HTTP {status}: {err_body}"),
                    format!("HTTP_{status}"),
                ),
            };
            delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;
            return Ok(DeepSeekWebExecutorResponse {
                response: json_error(status, &msg, "upstream_error", Some(&code_str)),
                url,
                headers: req_headers_for_resp,
                transformed_body: payload,
                transport: TransportKind::Reqwest,
            });
        }

        // Check content-type for HTTP-200 JSON error
        let ct = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if ct.contains("application/json") {
            let json: Value = response.json().await?;
            if let Some(code) = json.get("code").and_then(Value::as_i64) {
                if code != 0 {
                    let biz_msg = json.pointer("/data/biz_msg").and_then(Value::as_str).unwrap_or("");
                    let msg = json.get("msg").and_then(Value::as_str).unwrap_or(biz_msg);
                    let err_msg = format!("DeepSeek error {code}: {msg}");
                    let mapped_status = match code {
                        40003 => {
                            ACCESS_TOKEN_CACHE.write().await.remove(&user_token);
                            401
                        }
                        40002 => 429,
                        _ => 502,
                    };
                    delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;
                    return Ok(DeepSeekWebExecutorResponse {
                        response: json_error(mapped_status, &err_msg, "upstream_error", Some(&code.to_string())),
                        url,
                        headers: req_headers_for_resp,
                        transformed_body: payload,
                        transport: TransportKind::Reqwest,
                    });
                }
                // Non-stream JSON 200 — pass through
                delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;
                let bytes = serde_json::to_vec(&json)?;
                let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
                *http_resp.status_mut() = reqwest::StatusCode::OK;
                http_resp.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                return Ok(DeepSeekWebExecutorResponse {
                    response: UpstreamResponse::Reqwest(reqwest::Response::from(http_resp)),
                    url,
                    headers: req_headers_for_resp,
                    transformed_body: payload,
                    transport: TransportKind::Reqwest,
                });
            }
            // No code field — treat as non-stream response, pass through
            delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;
            let bytes = serde_json::to_vec(&json)?;
            let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
            *http_resp.status_mut() = reqwest::StatusCode::OK;
            http_resp.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            return Ok(DeepSeekWebExecutorResponse {
                response: UpstreamResponse::Reqwest(reqwest::Response::from(http_resp)),
                url,
                headers: req_headers_for_resp,
                transformed_body: payload,
                transport: TransportKind::Reqwest,
            });
        }

        // Stream path
        let converted = if request.stream {
            transform_deepseek_stream(response, &request.model, thinking, search).await?
        } else {
            let (content, reasoning) =
                collect_deepseek_content_v2(response, &request.model, thinking, search).await?;
            let cid = format!("chatcmpl-ds-{}", &Uuid::new_v4().simple().to_string()[..12]);
            let created = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let mut message = json!({ "role": "assistant", "content": content });
            if thinking && !reasoning.is_empty() {
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
            let bytes = serde_json::to_vec(&body)?;
            let mut http_resp = http::Response::new(ReqwestBody::from(bytes));
            *http_resp.status_mut() = reqwest::StatusCode::OK;
            http_resp.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            UpstreamResponse::Reqwest(reqwest::Response::from(http_resp))
        };

        delete_session(&self.pool, &access_token, &session_id, request.proxy.as_ref()).await;

        Ok(DeepSeekWebExecutorResponse {
            response: converted,
            url,
            headers: req_headers_for_resp,
            transformed_body: payload,
            transport: TransportKind::Reqwest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PoW hash vectors from OmniRoute `tests/unit/deepseek-pow-js-only.test.ts`.
    /// Confirms our Rust `deepseek_hash_v1` matches the FIPS-202 SHA3-256 with
    /// only the last 23 KECCAK-p[1600] rounds.
    #[test]
    fn deepseek_hash_v1_matches_known_vectors() {
        let cases: &[(&str, &str)] = &[
            (
                "1122334455667788_1778891543095_0",
                "311b26ae1e0fe7375e242958ce46db5552a6c67fea3f96880dcd846c63a74286",
            ),
            (
                "1122334455667788_1778891543095_1",
                "526f9103dfe22bcda9481b7f304a157b8edb18c5bb96a2061eefec1fbd0db706",
            ),
            (
                "vector_42_0",
                "1813da791ddac0da3a225e5878ff3d3ea07577e048f8e9575b82a274b71e3810",
            ),
            (
                "bb_1778891543095_0",
                "705e5d630f02d09a8179c6a0fcb0caf7265f08fb206fadca0301224f4422fc64",
            ),
        ];
        for (input, expected) in cases {
            let got = hex::encode(deepseek_hash_v1(input));
            assert_eq!(&got, *expected, "mismatch for input {input:?}");
        }
    }

    #[test]
    fn solve_pow_finds_correct_nonce() {
        // Known vector: prefix "1122334455667788_1778891543095_" + "0" → matches challenge
        let prefix = "1122334455667788_1778891543095_";
        let challenge = "311b26ae1e0fe7375e242958ce46db5552a6c67fea3f96880dcd846c63a74286";
        let nonce = solve_pow(prefix, challenge, 10);
        assert_eq!(nonce, 0);
    }

    #[test]
    fn extract_user_token_handles_json_wrapper() {
        let wrapped = r#"{"value":"abc123","__version":"0"}"#;
        assert_eq!(extract_user_token(Some(wrapped)).as_deref(), Some("abc123"));
        assert_eq!(
            extract_user_token(Some("raw-token-xyz")).as_deref(),
            Some("raw-token-xyz")
        );
        assert_eq!(extract_user_token(None), None);
        assert_eq!(extract_user_token(Some("")), None);
    }

    #[test]
    fn format_stream_content_strips_search_artifacts() {
        assert_eq!(format_stream_content("hello", true), "hello");
        assert_eq!(format_stream_content("SEARCHlooking", true), "looking");
        assert_eq!(format_stream_content("FINISHED", true), "");
        assert_eq!(
            format_stream_content("see [citation:3] for details", true),
            "see [3] for details"
        );
        // Not a search model → pass-through
        assert_eq!(format_stream_content("FINISHED", false), "FINISHED");
    }

    #[test]
    fn is_thinking_and_search_models() {
        assert!(is_thinking_model("deepseek-v4-pro-think"));
        assert!(is_thinking_model("DeepSeek-R1"));
        assert!(is_thinking_model("deepseek-reasoner"));
        assert!(!is_thinking_model("deepseek-chat"));

        assert!(is_search_model("deepseek-search"));
        assert!(is_search_model("DeepSeek-R1-Search"));
        assert!(!is_search_model("deepseek-chat"));

        assert!(is_expert_model("deepseek-v4-pro"));
        assert!(is_expert_model("deepseek-v4-expert"));
        assert!(!is_expert_model("deepseek-chat"));
    }
}
