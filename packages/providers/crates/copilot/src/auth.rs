//! Copilot session tokens: exchange the long-lived GitHub OAuth token for a
//! short-lived API bearer, cache it, refresh ahead of expiry.

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use agentloop_contracts::{ProviderId, branding, now_ms};
use agentloop_core::ProviderError;
use agentloop_provider_common::status_to_provider_error;

use crate::config::{COPILOT_PROVIDER_ID, CopilotConfig};

/// Refresh this many seconds before the reported expiry.
const REFRESH_MARGIN_SECS: u64 = 120;

/// An exchanged Copilot session: the API bearer plus where to call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopilotSession {
    pub(crate) bearer: String,
    pub(crate) api_base: String,
    pub(crate) expires_at_secs: u64,
}

impl CopilotSession {
    pub(crate) fn is_fresh(&self, now_secs: u64) -> bool {
        now_secs + REFRESH_MARGIN_SECS < self.expires_at_secs
    }
}

#[derive(Debug, Deserialize)]
struct ExchangeResponse {
    token: String,
    expires_at: u64,
    #[serde(default)]
    endpoints: Option<ExchangeEndpoints>,
}

#[derive(Debug, Deserialize)]
struct ExchangeEndpoints {
    #[serde(default)]
    api: Option<String>,
}

/// Parse the token-exchange response body into a session.
pub(crate) fn parse_exchange_response(
    provider: &ProviderId,
    value: serde_json::Value,
    fallback_api_base: &str,
) -> Result<CopilotSession, ProviderError> {
    let response: ExchangeResponse =
        serde_json::from_value(value).map_err(|err| ProviderError::Stream {
            provider: provider.clone(),
            message: format!("Copilot token exchange returned unexpected JSON: {err}"),
        })?;
    let api_base = response
        .endpoints
        .and_then(|endpoints| endpoints.api)
        .map(|api| api.trim_end_matches('/').to_owned())
        .filter(|api| !api.is_empty())
        .unwrap_or_else(|| fallback_api_base.trim_end_matches('/').to_owned());
    Ok(CopilotSession {
        bearer: response.token,
        api_base,
        expires_at_secs: response.expires_at,
    })
}

/// Owns the GitHub token and the cached session.
pub(crate) struct TokenExchanger {
    config: CopilotConfig,
    cached: Mutex<Option<CopilotSession>>,
}

impl TokenExchanger {
    pub(crate) fn new(config: CopilotConfig) -> Self {
        Self {
            config,
            cached: Mutex::new(None),
        }
    }

    /// A fresh session, exchanging or refreshing when needed.
    pub(crate) async fn session(&self, client: &Client) -> Result<CopilotSession, ProviderError> {
        let provider = ProviderId::from(COPILOT_PROVIDER_ID);
        let now_secs = now_ms() / 1000;

        let mut cached = self.cached.lock().await;
        if let Some(session) = cached.as_ref() {
            if session.is_fresh(now_secs) {
                return Ok(session.clone());
            }
        }

        tracing::debug!(target: "copilot", "exchanging GitHub token for a Copilot session");
        let response = client
            .get(&self.config.token_url)
            .header(
                "Authorization",
                format!("token {}", self.config.github_token),
            )
            .header("Accept", "application/json")
            .header("User-Agent", branding::USER_AGENT)
            .send()
            .await
            .map_err(|err| ProviderError::Http {
                provider: provider.clone(),
                message: format!("Copilot token exchange failed: {err}"),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(ProviderError::AuthRejected {
                    provider,
                    message: format!(
                        "GitHub rejected the Copilot token exchange ({status}): {body}. \
                         The account may lack a Copilot subscription, or the stored \
                         sign-in expired — sign in again with VS Code or the Copilot CLI."
                    ),
                });
            }
            return Err(status_to_provider_error(&provider, status, body, None));
        }

        let value =
            response
                .json::<serde_json::Value>()
                .await
                .map_err(|err| ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("Copilot token exchange body was not JSON: {err}"),
                })?;
        let session = parse_exchange_response(&provider, value, &self.config.fallback_api_base)?;
        *cached = Some(session.clone());
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ProviderId {
        ProviderId::from(COPILOT_PROVIDER_ID)
    }

    #[test]
    fn parses_full_exchange_response() {
        let session = parse_exchange_response(
            &provider(),
            serde_json::json!({
                "token": "tid=abc;exp=99",
                "expires_at": 1_800_000_000u64,
                "endpoints": {"api": "https://api.enterprise.githubcopilot.com/"}
            }),
            "https://api.githubcopilot.com",
        )
        .expect("parses");
        assert_eq!(session.bearer, "tid=abc;exp=99");
        assert_eq!(session.api_base, "https://api.enterprise.githubcopilot.com");
        assert_eq!(session.expires_at_secs, 1_800_000_000);
    }

    #[test]
    fn missing_endpoints_fall_back() {
        let session = parse_exchange_response(
            &provider(),
            serde_json::json!({"token": "tid=x", "expires_at": 100u64}),
            "https://api.githubcopilot.com",
        )
        .expect("parses");
        assert_eq!(session.api_base, "https://api.githubcopilot.com");
    }

    #[test]
    fn malformed_exchange_is_a_stream_error() {
        let err = parse_exchange_response(
            &provider(),
            serde_json::json!({"nope": true}),
            "https://api.githubcopilot.com",
        )
        .expect_err("must fail");
        assert!(err.to_string().contains("unexpected JSON"), "{err}");
    }

    #[test]
    fn freshness_respects_refresh_margin() {
        let session = CopilotSession {
            bearer: "b".to_owned(),
            api_base: "https://api".to_owned(),
            expires_at_secs: 1_000,
        };
        assert!(session.is_fresh(1_000 - REFRESH_MARGIN_SECS - 1));
        assert!(!session.is_fresh(1_000 - REFRESH_MARGIN_SECS));
        assert!(!session.is_fresh(2_000));
    }
}
