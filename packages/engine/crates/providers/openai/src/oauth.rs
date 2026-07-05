//! OpenAI ChatGPT Pro/Plus OAuth (Codex CLI flow): browser PKCE on
//! `localhost:1455` and headless device authorization.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ProviderId, branding};
use agentloop_core::ProviderError;

use crate::config::OPENAI_PROVIDER_ID;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const ISSUER: &str = "https://auth.openai.com";
const OAUTH_PORT: u16 = 1455;
const OAUTH_POLLING_SAFETY_MARGIN_MS: u64 = 3000;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Which OAuth UX the user chose in `/connect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiOAuthMethod {
    Browser,
    Headless,
}

/// Persisted OAuth credentials at `~/.config/agentloop/openai-auth.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiOAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

/// What to show the user while they authorize.
#[derive(Debug)]
pub struct OpenAiOAuthStart {
    pub url: String,
    pub instructions: String,
    pub method: OpenAiOAuthMethod,
    browser: Option<BrowserPending>,
    headless: Option<HeadlessPending>,
}

#[derive(Debug)]
struct BrowserPending {
    redirect_uri: String,
    verifier: String,
    state: String,
}

#[derive(Debug)]
struct HeadlessPending {
    device_auth_id: String,
    user_code: String,
    interval_ms: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    access_token: String,
    refresh_token: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceUserCodeResponse {
    device_auth_id: String,
    user_code: String,
    interval: String,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

struct PkceCodes {
    verifier: String,
    challenge: String,
}

/// Start browser or headless OAuth. Call [`OpenAiOAuthStart::complete`] to
/// poll/callback until tokens arrive or `cancel` trips.
pub async fn start_oauth(method: OpenAiOAuthMethod) -> Result<OpenAiOAuthStart, ProviderError> {
    let provider = provider_id();
    match method {
        OpenAiOAuthMethod::Browser => {
            let pkce = generate_pkce()?;
            let state = random_url_safe(32);
            let redirect_uri = format!("http://localhost:{OAUTH_PORT}/auth/callback");
            let url = build_authorize_url(&redirect_uri, &pkce, &state);
            Ok(OpenAiOAuthStart {
                url,
                instructions:
                    "Complete authorization in your browser. This window will close automatically."
                        .to_owned(),
                method,
                browser: Some(BrowserPending {
                    redirect_uri,
                    verifier: pkce.verifier,
                    state,
                }),
                headless: None,
            })
        }
        OpenAiOAuthMethod::Headless => {
            let client = Client::new();
            let response = client
                .post(format!("{ISSUER}/api/accounts/deviceauth/usercode"))
                .header("Content-Type", "application/json")
                .header("User-Agent", branding::USER_AGENT)
                .json(&serde_json::json!({ "client_id": CLIENT_ID }))
                .send()
                .await
                .map_err(|err| http_error(&provider, err.to_string()))?;
            if !response.status().is_success() {
                return Err(http_error(
                    &provider,
                    format!("device authorization failed: HTTP {}", response.status()),
                ));
            }
            let device: DeviceUserCodeResponse =
                response.json().await.map_err(|err| ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("device authorization response was not JSON: {err}"),
                })?;
            let interval_secs = device.interval.parse::<u64>().unwrap_or(5).max(1);
            Ok(OpenAiOAuthStart {
                url: format!("{ISSUER}/codex/device"),
                instructions: format!("Enter code: {}", device.user_code),
                method,
                browser: None,
                headless: Some(HeadlessPending {
                    device_auth_id: device.device_auth_id,
                    user_code: device.user_code,
                    interval_ms: interval_secs * 1000,
                }),
            })
        }
    }
}

impl OpenAiOAuthStart {
    /// Wait for the user to finish authorization.
    pub async fn complete(
        self,
        cancel: CancellationToken,
    ) -> Result<OpenAiOAuthTokens, ProviderError> {
        match self.method {
            OpenAiOAuthMethod::Browser => {
                let pending = self
                    .browser
                    .ok_or_else(|| internal_error("browser state missing"))?;
                let code = wait_for_browser_callback(&pending.state, cancel.clone()).await?;
                exchange_auth_code(&code, &pending.redirect_uri, &pending.verifier).await
            }
            OpenAiOAuthMethod::Headless => {
                let pending = self
                    .headless
                    .ok_or_else(|| internal_error("headless state missing"))?;
                poll_headless_device(&pending, cancel).await
            }
        }
    }
}

/// Whether stored OAuth credentials exist on disk.
pub fn oauth_tokens_discoverable() -> bool {
    load_oauth_tokens().is_ok()
}

/// Load stored OAuth tokens, refreshing when expired.
pub async fn resolve_oauth_access_token() -> Result<Option<String>, ProviderError> {
    let mut tokens = match load_oauth_tokens() {
        Ok(tokens) => tokens,
        Err(_) => return Ok(None),
    };
    if token_expired(&tokens) {
        tokens = refresh_stored_tokens(&tokens).await?;
        store_oauth_tokens(&tokens)?;
    }
    Ok(Some(tokens.access_token))
}

/// Persist OAuth tokens (merge-upsert the file).
pub fn store_oauth_tokens(tokens: &OpenAiOAuthTokens) -> Result<PathBuf, ProviderError> {
    let Some(dir) = default_config_dir() else {
        return Err(ProviderError::AuthMissing {
            provider: provider_id(),
            hint: "cannot store OpenAI sign-in: neither XDG_CONFIG_HOME nor HOME is set".to_owned(),
        });
    };
    store_oauth_tokens_in(&dir, tokens)
}

fn store_oauth_tokens_in(dir: &Path, tokens: &OpenAiOAuthTokens) -> Result<PathBuf, ProviderError> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|err| store_error("create", dir, &err))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| store_error("restrict permissions on", dir, &err))?;
        }
    }
    let path = dir.join("openai-auth.json");
    let body = serde_json::to_string_pretty(tokens)
        .map_err(|err| internal_error(format!("serialize tokens: {err}")))?;
    let tmp = dir.join(format!("openai-auth.json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, &body).map_err(|err| store_error("write", &tmp, &err))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(store_error("restrict permissions on", &tmp, &err));
        }
    }
    if let Err(err) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(store_error("atomically replace", &path, &err));
    }
    Ok(path)
}

pub(crate) fn load_oauth_tokens() -> Result<OpenAiOAuthTokens, ProviderError> {
    let path = oauth_auth_path()?;
    let raw = std::fs::read_to_string(&path).map_err(|err| ProviderError::AuthMissing {
        provider: provider_id(),
        hint: format!("OpenAI OAuth file unreadable at {}: {err}", path.display()),
    })?;
    serde_json::from_str(&raw).map_err(|err| ProviderError::AuthMissing {
        provider: provider_id(),
        hint: format!(
            "OpenAI OAuth file at {} is not valid JSON: {err}",
            path.display()
        ),
    })
}

pub(crate) fn oauth_account_id() -> Option<String> {
    load_oauth_tokens()
        .ok()
        .and_then(|tokens| tokens.account_id)
}

fn oauth_auth_path() -> Result<PathBuf, ProviderError> {
    default_config_dir()
        .map(|dir| dir.join("openai-auth.json"))
        .ok_or_else(|| ProviderError::AuthMissing {
            provider: provider_id(),
            hint: "OpenAI OAuth path unavailable: neither XDG_CONFIG_HOME nor HOME is set"
                .to_owned(),
        })
}

fn default_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("agentloop"));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config").join("agentloop"))
}

async fn wait_for_browser_callback(
    expected_state: &str,
    cancel: CancellationToken,
) -> Result<String, ProviderError> {
    let provider = provider_id();
    let listener = TcpListener::bind(format!("127.0.0.1:{OAUTH_PORT}"))
        .await
        .map_err(|err| ProviderError::AuthMissing {
            provider: provider.clone(),
            hint: format!(
                "could not bind OAuth callback on localhost:{OAUTH_PORT}: {err} \
                     (is another sign-in already running?)"
            ),
        })?;
    let deadline = tokio::time::Instant::now() + CALLBACK_TIMEOUT;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(ProviderError::AuthMissing {
                    provider,
                    hint: "OAuth callback timeout — authorization took too long".to_owned(),
                });
            }
            accept = listener.accept() => {
                let (mut stream, _) = accept.map_err(|err| http_error(&provider, err.to_string()))?;
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.map_err(|err| http_error(&provider, err.to_string()))?;
                let request = String::from_utf8_lossy(&buf[..n]);
                let Some(first_line) = request.lines().next() else { continue };
                let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                if path.starts_with("/auth/callback") {
                    let query = path.split('?').nth(1).unwrap_or("");
                    let params: std::collections::HashMap<_, _> = query
                        .split('&')
                        .filter_map(|pair| {
                            let mut parts = pair.splitn(2, '=');
                            Some((parts.next()?, parts.next().unwrap_or("")))
                        })
                        .map(|(k, v)| (k.to_owned(), urlencoding_light(v)))
                        .collect();
                    if let Some(error) = params.get("error") {
                        let detail = params.get("error_description").map(String::as_str).unwrap_or(error);
                        let body = oauth_error_html(detail);
                        let body = response_bytes(400, &body);
                        let _ = stream.write_all(&body).await;
                        return Err(ProviderError::AuthRejected {
                            provider: provider.clone(),
                            message: detail.to_owned(),
                        });
                    }
                    let Some(code) = params.get("code") else {
                        let body = oauth_error_html("Missing authorization code");
                        let body = response_bytes(400, &body);
                        let _ = stream.write_all(&body).await;
                        return Err(ProviderError::AuthRejected {
                            provider,
                            message: "missing authorization code".to_owned(),
                        });
                    };
                    let Some(state) = params.get("state") else {
                        return Err(ProviderError::AuthRejected {
                            provider,
                            message: "missing OAuth state".to_owned(),
                        });
                    };
                    if state != expected_state {
                        let body = oauth_error_html("Invalid state — potential CSRF attack");
                        let body = response_bytes(400, &body);
                        let _ = stream.write_all(&body).await;
                        return Err(ProviderError::AuthRejected {
                            provider,
                            message: "invalid OAuth state".to_owned(),
                        });
                    }
                    let body = oauth_success_html();
                    let bytes = response_bytes(200, &body);
                    let _ = stream.write_all(&bytes).await;
                    return Ok(code.clone());
                }
                let bytes = response_bytes(404, "Not found");
                let _ = stream.write_all(&bytes).await;
            }
        }
    }
}

async fn poll_headless_device(
    pending: &HeadlessPending,
    cancel: CancellationToken,
) -> Result<OpenAiOAuthTokens, ProviderError> {
    let provider = provider_id();
    let client = Client::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            _ = tokio::time::sleep(Duration::from_millis(
                pending.interval_ms + OAUTH_POLLING_SAFETY_MARGIN_MS,
            )) => {}
        }
        let response = client
            .post(format!("{ISSUER}/api/accounts/deviceauth/token"))
            .header("Content-Type", "application/json")
            .header("User-Agent", branding::USER_AGENT)
            .json(&serde_json::json!({
                "device_auth_id": pending.device_auth_id,
                "user_code": pending.user_code,
            }))
            .send()
            .await
            .map_err(|err| http_error(&provider, err.to_string()))?;
        if response.status().is_success() {
            let data: DeviceTokenResponse =
                response.json().await.map_err(|err| ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("device token response was not JSON: {err}"),
                })?;
            return exchange_auth_code(
                &data.authorization_code,
                &format!("{ISSUER}/deviceauth/callback"),
                &data.code_verifier,
            )
            .await;
        }
        if response.status() != reqwest::StatusCode::FORBIDDEN
            && response.status() != reqwest::StatusCode::NOT_FOUND
        {
            return Err(ProviderError::AuthRejected {
                provider,
                message: format!("device authorization failed: HTTP {}", response.status()),
            });
        }
    }
}

async fn exchange_auth_code(
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<OpenAiOAuthTokens, ProviderError> {
    let provider = provider_id();
    let client = Client::new();
    let response = client
        .post(format!("{ISSUER}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            url_encode(code),
            url_encode(redirect_uri),
            url_encode(CLIENT_ID),
            url_encode(verifier),
        ))
        .send()
        .await
        .map_err(|err| http_error(&provider, err.to_string()))?;
    if !response.status().is_success() {
        return Err(ProviderError::AuthRejected {
            provider,
            message: format!("token exchange failed: HTTP {}", response.status()),
        });
    }
    let tokens: TokenResponse = response.json().await.map_err(|err| ProviderError::Stream {
        provider: provider.clone(),
        message: format!("token response was not JSON: {err}"),
    })?;
    Ok(map_token_response(tokens))
}

async fn refresh_stored_tokens(
    stored: &OpenAiOAuthTokens,
) -> Result<OpenAiOAuthTokens, ProviderError> {
    let provider = provider_id();
    let client = Client::new();
    let response = client
        .post(format!("{ISSUER}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            url_encode(&stored.refresh_token),
            url_encode(CLIENT_ID),
        ))
        .send()
        .await
        .map_err(|err| http_error(&provider, err.to_string()))?;
    if !response.status().is_success() {
        return Err(ProviderError::AuthRejected {
            provider,
            message: format!("token refresh failed: HTTP {}", response.status()),
        });
    }
    let tokens: TokenResponse = response.json().await.map_err(|err| ProviderError::Stream {
        provider: provider.clone(),
        message: format!("refresh response was not JSON: {err}"),
    })?;
    let mut mapped = map_token_response(tokens);
    if mapped.account_id.is_none() {
        mapped.account_id = stored.account_id.clone();
    }
    Ok(mapped)
}

fn map_token_response(tokens: TokenResponse) -> OpenAiOAuthTokens {
    let expires_at_ms = tokens
        .expires_in
        .map(|secs| now_ms().saturating_add(secs.saturating_mul(1000)));
    let account_id = tokens
        .id_token
        .as_deref()
        .and_then(extract_account_id_from_jwt)
        .or_else(|| extract_account_id_from_jwt(&tokens.access_token));
    OpenAiOAuthTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at_ms,
        account_id,
    }
}

fn extract_account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("https://api.openai.com/auth")
                .and_then(|auth| auth.get("chatgpt_account_id"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .or_else(|| {
            value
                .get("organizations")
                .and_then(|orgs| orgs.as_array())
                .and_then(|orgs| orgs.first())
                .and_then(|org| org.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
}

fn token_expired(tokens: &OpenAiOAuthTokens) -> bool {
    tokens
        .expires_at_ms
        .is_some_and(|expires| now_ms().saturating_add(60_000) >= expires)
}

fn build_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String {
    format!(
        "{ISSUER}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator={}",
        url_encode(CLIENT_ID),
        url_encode(redirect_uri),
        url_encode("openid profile email offline_access"),
        url_encode(&pkce.challenge),
        url_encode(state),
        url_encode("flex"),
    )
}

fn generate_pkce() -> Result<PkceCodes, ProviderError> {
    let verifier = random_url_safe(43);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    Ok(PkceCodes {
        verifier,
        challenge,
    })
}

fn random_url_safe(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 1];
    while out.len() < len {
        if getrandom::getrandom(&mut buf).is_err() {
            break;
        }
        out.push(CHARS[(buf[0] as usize) % CHARS.len()] as char);
    }
    while out.len() < len {
        out.push('a');
    }
    out
}

fn response_bytes(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn oauth_success_html() -> String {
    "<html><body><h1>Authorization successful</h1><p>You can close this window.</p></body></html>"
        .to_owned()
}

fn oauth_error_html(message: &str) -> String {
    format!("<html><body><h1>Authorization failed</h1><p>{message}</p></body></html>")
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn urlencoding_light(value: &str) -> String {
    value.replace('+', " ").replace("%20", " ")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn provider_id() -> ProviderId {
    ProviderId::from(OPENAI_PROVIDER_ID)
}

fn http_error(provider: &ProviderId, message: String) -> ProviderError {
    ProviderError::Http {
        provider: provider.clone(),
        message,
    }
}

fn internal_error(message: impl Into<String>) -> ProviderError {
    ProviderError::Stream {
        provider: provider_id(),
        message: message.into(),
    }
}

fn store_error(action: &str, path: &Path, err: &std::io::Error) -> ProviderError {
    ProviderError::AuthMissing {
        provider: provider_id(),
        hint: format!(
            "the OpenAI sign-in could not be saved: failed to {action} {}: {err}",
            path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_url_safe_base64() {
        let pkce = generate_pkce().expect("pkce");
        assert_eq!(pkce.verifier.len(), 43);
        assert!(!pkce.challenge.contains('='));
    }

    #[test]
    fn store_and_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tokens = OpenAiOAuthTokens {
            access_token: "access".to_owned(),
            refresh_token: "refresh".to_owned(),
            expires_at_ms: Some(1_700_000_000_000),
            account_id: Some("acct".to_owned()),
        };
        store_oauth_tokens_in(dir.path(), &tokens).expect("store");
        temp_env::with_var(
            "XDG_CONFIG_HOME",
            Some(dir.path().parent().unwrap()),
            || {
                temp_env::with_var("HOME", None::<&str>, || {
                    // path uses XDG_CONFIG_HOME/agentloop
                });
            },
        );
        let path = dir.path().join("openai-auth.json");
        let raw = std::fs::read_to_string(path).expect("read");
        let loaded: OpenAiOAuthTokens = serde_json::from_str(&raw).expect("json");
        assert_eq!(loaded.access_token, "access");
        assert_eq!(loaded.account_id.as_deref(), Some("acct"));
    }
}
