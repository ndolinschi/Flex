//! GitHub device-code sign-in: obtain the long-lived GitHub OAuth token by
//! showing the user a one-time code to confirm on github.com, then polling
//! until GitHub reports the confirmation.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ProviderId, branding};
use agentloop_core::ProviderError;
use agentloop_provider_common::status_to_provider_error;

use crate::config::COPILOT_PROVIDER_ID;

/// GitHub App client id used by the official Copilot editor sign-ins, so the
/// resulting OAuth token carries Copilot access.
pub const COPILOT_DEVICE_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// GitHub host serving the device flow.
const DEFAULT_BASE_URL: &str = "https://github.com";

/// OAuth scope requested for the sign-in.
const DEVICE_FLOW_SCOPE: &str = "read:user";

/// RFC 8628 grant type sent when polling for the token.
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Polling interval when the device-code response omits one.
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

/// Extra seconds added when GitHub says `slow_down` without a new interval.
const SLOW_DOWN_BUMP_SECS: u64 = 5;

/// A pending device authorization: show [`user_code`](Self::user_code) and
/// [`verification_uri`](Self::verification_uri) to the user, then hand the
/// value back to [`DeviceFlow::poll`] until they confirm on github.com.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceAuthorization {
    /// One-time code the user types at the verification page.
    pub user_code: String,
    /// Page where the user enters the code (usually `https://github.com/login/device`).
    pub verification_uri: String,
    /// Seconds until the device code expires and the flow must be restarted.
    pub expires_in: u64,
    /// Seconds to wait between polls.
    #[serde(default = "default_poll_interval")]
    pub interval: u64,
    device_code: String,
}

fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL_SECS
}

/// Runs the GitHub device-code OAuth flow (RFC 8628): [`start`](Self::start)
/// requests a one-time code, [`poll`](Self::poll) waits for the user to
/// confirm it and returns the long-lived GitHub OAuth token.
pub struct DeviceFlow {
    client: Client,
    base_url: String,
}

impl DeviceFlow {
    /// A flow talking to github.com.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// A flow talking to an alternate host — used by tests to point at a mock
    /// server.
    pub fn with_base_url(url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: url.into().trim_end_matches('/').to_owned(),
        }
    }

    /// Request a device code. Returns the code and verification page to show
    /// the user.
    pub async fn start(&self) -> Result<DeviceAuthorization, ProviderError> {
        let provider = provider_id();
        let url = format!("{}/login/device/code", self.base_url);
        tracing::debug!(target: "copilot", "requesting a GitHub device code");
        let response = self
            .client
            .post(&url)
            .header("Accept", "application/json")
            .header("User-Agent", branding::USER_AGENT)
            .form(&[
                ("client_id", COPILOT_DEVICE_CLIENT_ID),
                ("scope", DEVICE_FLOW_SCOPE),
            ])
            .send()
            .await
            .map_err(|err| ProviderError::Http {
                provider: provider.clone(),
                message: format!("GitHub device-code request failed: {err}"),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(&provider, status, body, None));
        }

        let value =
            response
                .json::<serde_json::Value>()
                .await
                .map_err(|err| ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("GitHub device-code response was not JSON: {err}"),
                })?;
        parse_device_code_response(&provider, value)
    }

    /// Poll until the user confirms the code on github.com, then return the
    /// GitHub OAuth access token (`ghu_…`/`gho_…`).
    ///
    /// Waits the server-provided interval before every poll (GitHub rejects
    /// immediate polls), honors `slow_down`, gives up once
    /// [`DeviceAuthorization::expires_in`] has elapsed, and returns
    /// [`ProviderError::Cancelled`] promptly when `cancel` trips.
    pub async fn poll(
        &self,
        auth: &DeviceAuthorization,
        cancel: CancellationToken,
    ) -> Result<String, ProviderError> {
        let provider = provider_id();
        let url = format!("{}/login/oauth/access_token", self.base_url);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(auth.expires_in);
        let mut interval = auth.interval;

        loop {
            // Wait first: GitHub rejects polls issued before the interval.
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(ProviderError::Cancelled { provider });
                }
                _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(expired_error(&provider));
            }

            let response = tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(ProviderError::Cancelled { provider });
                }
                result = self
                    .client
                    .post(&url)
                    .header("Accept", "application/json")
                    .header("User-Agent", branding::USER_AGENT)
                    .form(&[
                        ("client_id", COPILOT_DEVICE_CLIENT_ID),
                        ("device_code", auth.device_code.as_str()),
                        ("grant_type", DEVICE_GRANT_TYPE),
                    ])
                    .send() => {
                    result.map_err(|err| ProviderError::Http {
                        provider: provider.clone(),
                        message: format!("GitHub device-code poll failed: {err}"),
                    })?
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_else(|err| err.to_string());
                return Err(status_to_provider_error(&provider, status, body, None));
            }

            let value = response.json::<serde_json::Value>().await.map_err(|err| {
                ProviderError::Stream {
                    provider: provider.clone(),
                    message: format!("GitHub device-code poll body was not JSON: {err}"),
                }
            })?;
            match parse_poll_response(&provider, value)? {
                PollState::Token(token) => return Ok(token),
                PollState::Pending => {}
                PollState::SlowDown {
                    interval: suggested,
                } => {
                    interval = bumped_interval(interval, suggested);
                    tracing::debug!(
                        target: "copilot",
                        interval,
                        "GitHub asked to slow the device-code polling"
                    );
                }
            }
        }
    }
}

impl Default for DeviceFlow {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_id() -> ProviderId {
    ProviderId::from(COPILOT_PROVIDER_ID)
}

fn expired_error(provider: &ProviderId) -> ProviderError {
    ProviderError::AuthRejected {
        provider: provider.clone(),
        message: "the device code expired before the sign-in was confirmed on github.com — \
                  run /login again for a fresh code"
            .to_owned(),
    }
}

/// Parse the `POST /login/device/code` response body.
fn parse_device_code_response(
    provider: &ProviderId,
    value: serde_json::Value,
) -> Result<DeviceAuthorization, ProviderError> {
    if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
        let description = value
            .get("error_description")
            .and_then(|description| description.as_str())
            .unwrap_or("no further detail");
        return Err(ProviderError::AuthRejected {
            provider: provider.clone(),
            message: format!("GitHub refused to start the device sign-in ({error}): {description}"),
        });
    }
    serde_json::from_value(value).map_err(|err| ProviderError::Stream {
        provider: provider.clone(),
        message: format!("GitHub device-code response was unexpected JSON: {err}"),
    })
}

/// Non-terminal outcomes of one `POST /login/oauth/access_token` poll.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PollState {
    /// The user has not confirmed the code yet — poll again.
    Pending,
    /// GitHub asked us to poll less often, possibly naming a new interval.
    SlowDown { interval: Option<u64> },
    /// The user confirmed — here is the OAuth access token.
    Token(String),
}

/// The interval to use after a `slow_down`: the server-provided one, or the
/// current interval plus [`SLOW_DOWN_BUMP_SECS`].
fn bumped_interval(current: u64, suggested: Option<u64>) -> u64 {
    suggested.unwrap_or(current + SLOW_DOWN_BUMP_SECS)
}

/// Parse one token-poll response body.
fn parse_poll_response(
    provider: &ProviderId,
    value: serde_json::Value,
) -> Result<PollState, ProviderError> {
    if let Some(token) = value
        .get("access_token")
        .and_then(|token| token.as_str())
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        return Ok(PollState::Token(token.to_owned()));
    }
    let Some(error) = value.get("error").and_then(|error| error.as_str()) else {
        return Err(ProviderError::Stream {
            provider: provider.clone(),
            message: format!(
                "GitHub device-code poll returned neither a token nor an error: {value}"
            ),
        });
    };
    match error {
        "authorization_pending" => Ok(PollState::Pending),
        "slow_down" => Ok(PollState::SlowDown {
            interval: value.get("interval").and_then(|interval| interval.as_u64()),
        }),
        "expired_token" => Err(expired_error(provider)),
        "access_denied" => Err(ProviderError::AuthRejected {
            provider: provider.clone(),
            message: "the request was denied on github.com".to_owned(),
        }),
        other => {
            let description = value
                .get("error_description")
                .and_then(|description| description.as_str())
                .unwrap_or("no further detail");
            Err(ProviderError::AuthRejected {
                provider: provider.clone(),
                message: format!("GitHub device sign-in failed ({other}): {description}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    use super::*;

    fn provider() -> ProviderId {
        provider_id()
    }

    fn test_auth(interval: u64, expires_in: u64) -> DeviceAuthorization {
        DeviceAuthorization {
            user_code: "ABCD-1234".to_owned(),
            verification_uri: "https://github.com/login/device".to_owned(),
            expires_in,
            interval,
            device_code: "dc-123".to_owned(),
        }
    }

    // --- pure parsing -----------------------------------------------------

    #[test]
    fn parses_device_code_response() {
        let auth = parse_device_code_response(
            &provider(),
            serde_json::json!({
                "device_code": "dc-123",
                "user_code": "ABCD-1234",
                "verification_uri": "https://github.com/login/device",
                "expires_in": 899,
                "interval": 7
            }),
        )
        .expect("parses");
        assert_eq!(auth.device_code, "dc-123");
        assert_eq!(auth.user_code, "ABCD-1234");
        assert_eq!(auth.verification_uri, "https://github.com/login/device");
        assert_eq!(auth.expires_in, 899);
        assert_eq!(auth.interval, 7);
    }

    #[test]
    fn device_code_interval_defaults_to_five() {
        let auth = parse_device_code_response(
            &provider(),
            serde_json::json!({
                "device_code": "dc",
                "user_code": "AB-12",
                "verification_uri": "https://github.com/login/device",
                "expires_in": 899
            }),
        )
        .expect("parses");
        assert_eq!(auth.interval, DEFAULT_POLL_INTERVAL_SECS);
    }

    #[test]
    fn device_code_error_is_auth_rejected() {
        let err = parse_device_code_response(
            &provider(),
            serde_json::json!({
                "error": "unauthorized_client",
                "error_description": "The client is not authorized"
            }),
        )
        .expect_err("must fail");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("unauthorized_client"), "{err}");
    }

    #[test]
    fn malformed_device_code_is_a_stream_error() {
        let err = parse_device_code_response(&provider(), serde_json::json!({"nope": true}))
            .expect_err("must fail");
        assert!(matches!(err, ProviderError::Stream { .. }), "{err}");
    }

    #[test]
    fn poll_parses_access_token() {
        let state = parse_poll_response(
            &provider(),
            serde_json::json!({"access_token": "gho_secret", "token_type": "bearer"}),
        )
        .expect("parses");
        assert_eq!(state, PollState::Token("gho_secret".to_owned()));
    }

    #[test]
    fn poll_parses_pending_and_slow_down() {
        assert_eq!(
            parse_poll_response(
                &provider(),
                serde_json::json!({"error": "authorization_pending"})
            )
            .expect("parses"),
            PollState::Pending
        );
        assert_eq!(
            parse_poll_response(
                &provider(),
                serde_json::json!({"error": "slow_down", "interval": 7})
            )
            .expect("parses"),
            PollState::SlowDown { interval: Some(7) }
        );
        assert_eq!(
            parse_poll_response(&provider(), serde_json::json!({"error": "slow_down"}))
                .expect("parses"),
            PollState::SlowDown { interval: None }
        );
    }

    #[test]
    fn slow_down_bumps_the_interval() {
        assert_eq!(bumped_interval(5, None), 10);
        assert_eq!(bumped_interval(5, Some(7)), 7);
    }

    #[test]
    fn poll_expired_token_tells_the_user_to_retry() {
        let err = parse_poll_response(&provider(), serde_json::json!({"error": "expired_token"}))
            .expect_err("must fail");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("expired"), "{err}");
        assert!(err.to_string().contains("/login"), "{err}");
    }

    #[test]
    fn poll_access_denied_names_github() {
        let err = parse_poll_response(&provider(), serde_json::json!({"error": "access_denied"}))
            .expect_err("must fail");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("denied on github.com"), "{err}");
    }

    #[test]
    fn poll_unknown_error_is_auth_rejected() {
        let err = parse_poll_response(
            &provider(),
            serde_json::json!({"error": "incorrect_device_code"}),
        )
        .expect_err("must fail");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("incorrect_device_code"), "{err}");
    }

    #[test]
    fn poll_body_without_token_or_error_is_a_stream_error() {
        let err = parse_poll_response(&provider(), serde_json::json!({"token_type": "bearer"}))
            .expect_err("must fail");
        assert!(matches!(err, ProviderError::Stream { .. }), "{err}");
    }

    // --- end-to-end against a mock server ----------------------------------

    /// Replays a fixed sequence of responses, repeating the last one.
    struct SequenceResponder(Mutex<Vec<ResponseTemplate>>);

    impl SequenceResponder {
        fn new(responses: Vec<ResponseTemplate>) -> Self {
            Self(Mutex::new(responses))
        }
    }

    impl Respond for SequenceResponder {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            let mut queue = self.0.lock().expect("responder lock");
            if queue.len() > 1 {
                queue.remove(0)
            } else {
                queue[0].clone()
            }
        }
    }

    async fn mock_device_code(server: &MockServer, interval: u64) {
        Mock::given(method("POST"))
            .and(path("/login/device/code"))
            .and(body_string_contains(COPILOT_DEVICE_CLIENT_ID))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "dc-123",
                "user_code": "ABCD-1234",
                "verification_uri": "https://github.com/login/device",
                "expires_in": 899,
                "interval": interval
            })))
            .mount(server)
            .await;
    }

    async fn mock_token_sequence(server: &MockServer, responses: Vec<ResponseTemplate>) {
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains(COPILOT_DEVICE_CLIENT_ID))
            .and(body_string_contains("dc-123"))
            .respond_with(SequenceResponder::new(responses))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn start_parses_the_device_code_response() {
        let server = MockServer::start().await;
        mock_device_code(&server, 0).await;

        let auth = DeviceFlow::with_base_url(server.uri())
            .start()
            .await
            .expect("start");
        assert_eq!(auth.user_code, "ABCD-1234");
        assert_eq!(auth.verification_uri, "https://github.com/login/device");
        assert_eq!(auth.expires_in, 899);
        assert_eq!(auth.interval, 0);
        assert_eq!(auth.device_code, "dc-123");
    }

    #[tokio::test]
    async fn poll_waits_through_pending_then_returns_the_token() {
        let server = MockServer::start().await;
        mock_device_code(&server, 0).await;
        mock_token_sequence(
            &server,
            vec![
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"error": "authorization_pending"})),
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"error": "authorization_pending"})),
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"access_token": "gho_ok", "token_type": "bearer"}),
                ),
            ],
        )
        .await;

        let flow = DeviceFlow::with_base_url(server.uri());
        let auth = flow.start().await.expect("start");
        let token = flow
            .poll(&auth, CancellationToken::new())
            .await
            .expect("poll");
        assert_eq!(token, "gho_ok");
        let polls = server.received_requests().await.expect("recorded");
        assert_eq!(polls.len(), 4, "start + 3 polls");
    }

    #[tokio::test]
    async fn poll_honors_slow_down_and_continues() {
        let server = MockServer::start().await;
        mock_token_sequence(
            &server,
            vec![
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"error": "slow_down", "interval": 0})),
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"access_token": "gho_slow"})),
            ],
        )
        .await;

        let token = DeviceFlow::with_base_url(server.uri())
            .poll(&test_auth(0, 899), CancellationToken::new())
            .await
            .expect("poll");
        assert_eq!(token, "gho_slow");
        let polls = server.received_requests().await.expect("recorded");
        assert_eq!(polls.len(), 2, "slow_down poll then success poll");
    }

    #[tokio::test]
    async fn poll_surfaces_access_denied() {
        let server = MockServer::start().await;
        mock_token_sequence(
            &server,
            vec![
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"error": "access_denied"})),
            ],
        )
        .await;

        let err = DeviceFlow::with_base_url(server.uri())
            .poll(&test_auth(0, 899), CancellationToken::new())
            .await
            .expect_err("must fail");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("denied on github.com"), "{err}");
    }

    #[tokio::test]
    async fn poll_cancels_during_the_sleep() {
        // Never contacted: the cancel trips during the initial interval wait.
        let flow = DeviceFlow::with_base_url("http://127.0.0.1:9");
        let cancel = CancellationToken::new();
        let trip = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            trip.cancel();
        });

        let err = flow
            .poll(&test_auth(30, 899), cancel)
            .await
            .expect_err("must cancel");
        assert!(matches!(err, ProviderError::Cancelled { .. }), "{err}");
    }

    #[tokio::test]
    async fn poll_gives_up_once_the_code_expires() {
        // expires_in 0: the deadline passes before the first request is sent.
        let flow = DeviceFlow::with_base_url("http://127.0.0.1:9");
        let err = flow
            .poll(&test_auth(0, 0), CancellationToken::new())
            .await
            .expect_err("must expire");
        assert!(matches!(err, ProviderError::AuthRejected { .. }), "{err}");
        assert!(err.to_string().contains("expired"), "{err}");
    }
}
