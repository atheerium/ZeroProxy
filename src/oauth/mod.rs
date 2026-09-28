//! OAuth 2.0 flows implementation
//!
//! Supports:
//! - PKCE Authorization Code Flow (claude, codex, gitlab)
//! - Device Code Flow (github, kiro, kimi-coding, kilocode, codebuddy)
//! - Import Token (cursor)

use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use url::form_urlencoded;

pub const TOKEN_EXPIRY_BUFFER_MS: u64 = 5 * 60 * 1000;
pub mod background_refresh;
pub mod kilocode;
pub mod pending;
pub mod providers;
pub mod secret;
#[cfg(test)]
pub mod tests;
pub mod zed_auth;

pub enum OAuthFlowKind {
    AuthorizationCodePkce,
    DeviceCode,
    ImportToken,
}

pub use providers::OAuthProviderConfig;

/// Single source of truth for the AWS IAM Identity Center `sso-oidc` parameters
/// Kiro's Builder ID and Identity Center flows register against. Both
/// `kiro_register_client` (the `POST /api/oauth/kiro/device_code` route) and the
/// dashboard's `GET /api/oauth/kiro/device-code` route must post this same body,
/// so the two call sites share it rather than each restating the member names.
pub const KIRO_ISSUER_URL: &str = "https://identitycenter.amazonaws.com/ssoins-722374e8c3c8e6c6";
pub const KIRO_CLIENT_NAME: &str = "kiro-oauth-client";
pub const KIRO_CLIENT_TYPE: &str = "public";
pub const KIRO_DEFAULT_REGION: &str = "us-east-1";
pub const KIRO_DEFAULT_START_URL: &str = "https://view.awsapps.com/start";
pub const KIRO_SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
];
pub const KIRO_GRANT_TYPES: &[&str] = &[
    "urn:ietf:params:oauth:grant-type:device_code",
    "refresh_token",
];

pub fn kiro_oidc_base_url(region: &str) -> String {
    crate::core::env::var("ZEROPROXY_KIRO_OIDC_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("https://oidc.{region}.amazonaws.com"))
        .trim_end_matches('/')
        .to_string()
}

pub mod pkce {
    use super::*;

    pub fn generate_code_verifier() -> String {
        generate_code_verifier_with_len(32)
    }

    pub fn generate_code_verifier_with_len(bytes: usize) -> String {
        let mut random_bytes = vec![0u8; bytes];
        rand::thread_rng().fill_bytes(&mut random_bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes)
    }

    pub fn generate_code_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
    }

    pub fn generate_verifier_and_challenge() -> (String, String) {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        (verifier, challenge)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub interval: u64,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// Result of Kiro AWS SSO OIDC device flow initiation.
/// Contains the device code response plus the dynamically registered client credentials.
#[derive(Debug, Clone)]
pub struct KiroDeviceFlow {
    pub device_code: DeviceCodeResponse,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OAuthError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

pub struct RefreshRequest {
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
}

pub mod device_code {
    use super::*;

    pub async fn start_device_flow(
        _provider_config: &OAuthProviderConfig,
        client_id: &str,
        client_secret: &str,
    ) -> Result<DeviceCodeResponse, OAuthError> {
        // Kiro uses AWS SSO OIDC device authorization; must POST JSON (not form) to /device_authorization.
        if _provider_config.id == "kiro" {
            let base_url = _provider_config.authorize_url.trim_end_matches('/');
            let url = format!("{base_url}/device_authorization");
            let body = serde_json::json!({
                "clientId": client_id,
                "clientSecret": client_secret,
                "startUrl": "https://view.awsapps.com/start"
            });
            let response = reqwest::Client::new()
                .post(&url)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .json(&body)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| OAuthError {
                    error: "request_failed".to_string(),
                    error_description: Some(e.to_string()),
                })?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                tracing::warn!(
                    target: "zeroproxy::oauth",
                    provider = "kiro",
                    "kiro device authorization failed: HTTP {} body={}",
                    status,
                    text
                );
                let error: OAuthError = serde_json::from_str(&text).unwrap_or(OAuthError {
                    error: "unknown_error".to_string(),
                    error_description: Some(format!("HTTP {} body={}", status, text)),
                });
                return Err(error);
            }
            return response.json().await.map_err(|e| OAuthError {
                error: "parse_error".to_string(),
                error_description: Some(e.to_string()),
            });
        }

        let client = reqwest::Client::builder()
            .user_agent("GitHubCopilotChat/0.38.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        let params = [
            ("client_id", client_id),
            ("scope", &_provider_config.scopes.join(" ")),
        ];
        let response = client
            .post(_provider_config.authorize_url)
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError {
                error: "request_failed".to_string(),
                error_description: Some(e.to_string()),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::warn!(
                target: "zeroproxy::oauth",
                provider = _provider_config.id,
                "device code request failed: HTTP {} body={}",
                status,
                text
            );
            let error: OAuthError = serde_json::from_str(&text).unwrap_or(OAuthError {
                error: "unknown_error".to_string(),
                error_description: Some(format!("HTTP {} body={}", status, text)),
            });
            return Err(error);
        }

        response.json().await.map_err(|e| OAuthError {
            error: "parse_error".to_string(),
            error_description: Some(e.to_string()),
        })
    }

    pub async fn poll_for_token(
        provider_config: &OAuthProviderConfig,
        device_code: &str,
        _user_code: &str,
        interval_secs: u64,
    ) -> Result<TokenResponse, OAuthError> {
        let client = reqwest::Client::new();
        let mut current_interval = interval_secs;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(current_interval)).await;

            let client_id = provider_config
                .get_param("client_id")
                .unwrap_or(provider_config.client_id);
            let params = [
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                (
                    "client_id",
                    if client_id.is_empty() {
                        "zeroproxy"
                    } else {
                        client_id
                    },
                ),
                ("device_code", device_code),
            ];
            let response = client
                .post(provider_config.token_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| OAuthError {
                    error: "request_failed".to_string(),
                    error_description: Some(e.to_string()),
                })?;

            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let error = body.get("error").and_then(|e| e.as_str());

            match error {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    current_interval = (current_interval * 2).min(60);
                    continue;
                }
                Some("access_denied") => {
                    return Err(OAuthError {
                        error: "access_denied".to_string(),
                        error_description: Some(
                            "User denied the authorization request".to_string(),
                        ),
                    });
                }
                Some("expired_token") => {
                    return Err(OAuthError {
                        error: "expired_token".to_string(),
                        error_description: Some("The device code has expired".to_string()),
                    });
                }
                _ => {
                    if body.get("access_token").is_some() {
                        let token_response: TokenResponse =
                            serde_json::from_value(body).map_err(|e| OAuthError {
                                error: "parse_error".to_string(),
                                error_description: Some(e.to_string()),
                            })?;
                        return Ok(token_response);
                    }
                    continue;
                }
            }
        }
    }

    pub async fn exchange_code_for_token(
        provider_config: &OAuthProviderConfig,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
        client_id: &str,
    ) -> Result<TokenResponse, OAuthError> {
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", code_verifier),
        ];

        let response = client
            .post(provider_config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError {
                error: "request_failed".to_string(),
                error_description: Some(e.to_string()),
            })?;

        if !response.status().is_success() {
            let error: OAuthError = response.json().await.unwrap_or(OAuthError {
                error: "token_exchange_failed".to_string(),
                error_description: None,
            });
            return Err(error);
        }

        response.json().await.map_err(|e| OAuthError {
            error: "parse_error".to_string(),
            error_description: Some(e.to_string()),
        })
    }

    /// GitHub Copilot special: exchange OAuth token for Copilot token
    pub async fn exchange_github_copilot_token(
        oauth_token: &str,
    ) -> Result<TokenResponse, OAuthError> {
        let client = reqwest::Client::new();
        let response = client
            .post("https://github.com/copilot_internal/v1/token")
            .header("Authorization", format!("Bearer {}", oauth_token))
            .send()
            .await
            .map_err(|e| OAuthError {
                error: "request_failed".to_string(),
                error_description: Some(e.to_string()),
            })?;

        if !response.status().is_success() {
            let error: OAuthError = response.json().await.unwrap_or(OAuthError {
                error: "copilot_token_exchange_failed".to_string(),
                error_description: None,
            });
            return Err(error);
        }

        response.json().await.map_err(|e| OAuthError {
            error: "parse_error".to_string(),
            error_description: Some(e.to_string()),
        })
    }

    /// Kiro AWS SSO OIDC flow: combined client registration + device code start.
    /// Step 1: Register client with Kiro's OIDC endpoint.
    /// Step 2: Start device authorization using the registered client credentials.
    pub async fn kiro_start_device_flow() -> Result<super::KiroDeviceFlow, OAuthError> {
        let (client_id, client_secret) = kiro_register_client().await?;

        let kiro_config = super::providers::kiro();
        let device_resp = start_device_flow(&kiro_config, &client_id, &client_secret).await?;

        Ok(super::KiroDeviceFlow {
            device_code: device_resp,
            client_id,
            client_secret,
        })
    }

    /// Body for AWS IAM Identity Center `RegisterClient` (`POST /client/register`).
    ///
    /// The members are camelCase because that is the AWS wire format — there is
    /// no `client_id`, `client_name`, `client_type`, `grant_types`,
    /// `redirect_uris`, `token_endpoint_auth_method` or `expires_at` member, and
    /// omitting the required `clientName` / `clientType` fails validation.
    pub(crate) fn kiro_registration_body() -> serde_json::Value {
        serde_json::json!({
            "clientName": super::KIRO_CLIENT_NAME,
            "clientType": super::KIRO_CLIENT_TYPE,
            "scopes": super::KIRO_SCOPES,
            "grantTypes": super::KIRO_GRANT_TYPES,
            "issuerUrl": super::KIRO_ISSUER_URL,
        })
    }

    /// Extract the credentials AWS issues from a `RegisterClient` response.
    ///
    /// AWS returns `clientId` / `clientSecret` in camelCase. Both are required by
    /// the following `StartDeviceAuthorization` call, so a response missing
    /// either is an error — never a locally-invented substitute, which would be
    /// rejected downstream as "Invalid client provided".
    pub(crate) fn parse_kiro_registration_response(
        body: &serde_json::Value,
    ) -> Result<(String, String), OAuthError> {
        let missing = |field: &str| OAuthError {
            error: "client_registration_failed".to_string(),
            error_description: Some(format!("AWS RegisterClient response is missing `{field}`")),
        };

        let client_id = body
            .get("clientId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| missing("clientId"))?;
        let client_secret = body
            .get("clientSecret")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| missing("clientSecret"))?;

        Ok((client_id.to_string(), client_secret.to_string()))
    }

    /// Kiro AWS SSO OIDC flow - register client first, then standard device code flow.
    ///
    /// Each call creates a **fresh** client registration. Callers MUST re-register
    /// when a token-endpoint response indicates `invalid_client` or
    /// `expired_client`; the device-code polling path does not do this itself.
    pub async fn kiro_register_client() -> Result<(String, String), OAuthError> {
        let client = reqwest::Client::new();

        let response = client
            .post(format!(
                "{}/client/register",
                super::kiro_oidc_base_url(super::KIRO_DEFAULT_REGION)
            ))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&kiro_registration_body())
            .send()
            .await
            .map_err(|e| OAuthError {
                error: "request_failed".to_string(),
                error_description: Some(e.to_string()),
            })?;

        if !response.status().is_success() {
            let error: OAuthError = response.json().await.unwrap_or(OAuthError {
                error: "client_registration_failed".to_string(),
                error_description: None,
            });
            return Err(error);
        }

        let resp_body: serde_json::Value = response.json().await.map_err(|e| OAuthError {
            error: "parse_error".to_string(),
            error_description: Some(e.to_string()),
        })?;

        parse_kiro_registration_response(&resp_body)
    }

    pub async fn kilocode_start_device_flow(
        provider_config: &OAuthProviderConfig,
    ) -> Result<DeviceCodeResponse, OAuthError> {
        super::kilocode::kilocode_start_device_flow(provider_config).await
    }

    pub async fn kilocode_poll_for_token(
        provider_config: &OAuthProviderConfig,
        device_code: &str,
    ) -> Result<TokenResponse, OAuthError> {
        super::kilocode::kilocode_poll_for_token(provider_config, device_code).await
    }
}

pub mod kiro;
pub mod refresh;
pub mod token_refresh;

pub fn needs_refresh(expires_at: &Option<String>) -> bool {
    token_refresh::needs_refresh(expires_at)
}

pub fn expires_at_from_seconds(expires_in: i64) -> String {
    let expires = chrono::Utc::now() + chrono::Duration::seconds(expires_in);
    expires.to_rfc3339()
}

// Cursor import module - for importing tokens from Cursor's SQLite config.db
pub mod cursor_import {
    use crate::oauth::{expires_at_from_seconds, TokenResponse};

    #[derive(Clone)]
    pub struct CursorTokens {
        pub access_token: String,
        pub refresh_token: Option<String>,
        pub expires_at: Option<String>,
    }

    /// Read tokens from Cursor's SQLite config.db
    pub fn read_cursor_tokens(config_path: &str) -> Result<CursorTokens, String> {
        let conn = rusqlite::Connection::open(config_path)
            .map_err(|e| format!("Failed to open SQLite: {}", e))?;

        let result = conn
            .query_row(
                "SELECT access_token, refresh_token, expires_at FROM user_authentication LIMIT 1",
                [],
                |row| {
                    let access_token: String = row.get(0)?;
                    let refresh_token: Option<String> = row.get(1)?;
                    let expires_at_raw: Option<i64> = row.get(2)?;
                    Ok((access_token, refresh_token, expires_at_raw))
                },
            )
            .map_err(|e| format!("Failed to query: {}", e))?;

        let (access_token, refresh_token, expires_at_raw) = result;
        let expires_at = expires_at_raw.map(expires_at_from_seconds);

        Ok(CursorTokens {
            access_token,
            refresh_token,
            expires_at,
        })
    }

    /// Convert CursorTokens to TokenResponse
    pub fn to_token_response(cursor: CursorTokens) -> TokenResponse {
        TokenResponse {
            access_token: cursor.access_token,
            refresh_token: cursor.refresh_token,
            expires_in: None,
            id_token: None,
            token_type: Some("Bearer".to_string()),
            scope: None,
        }
    }
}

// GitLab PAT (Personal Access Token) support
pub mod gitlab_pat {
    use crate::oauth::TokenResponse;

    pub fn create_token_response(pat: &str) -> TokenResponse {
        TokenResponse {
            access_token: pat.to_string(),
            refresh_token: None,
            expires_in: None,
            id_token: None,
            token_type: Some("Bearer".to_string()),
            scope: Some("api read_user".to_string()),
        }
    }

    pub fn is_valid_pat(pat: &str) -> bool {
        !pat.is_empty() && pat.len() >= 20
    }
}
