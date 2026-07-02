//! HTTP helpers shared by provider clients.

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use reqwest::{Client, Method, RequestBuilder, StatusCode};

/// Build an authenticated request using bearer-token auth.
pub fn authenticated_request(
    client: &Client,
    method: Method,
    url: &str,
    api_key: &str,
) -> RequestBuilder {
    client
        .request(method, url)
        .bearer_auth(api_key)
        .header("accept", "application/json")
}

/// Convert a non-success HTTP response into a canonical provider error.
pub fn status_to_provider_error(
    provider: &ProviderId,
    status: StatusCode,
    body: String,
    model: Option<&str>,
) -> ProviderError {
    let message = if body.trim().is_empty() {
        status.to_string()
    } else {
        body
    };
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::AuthRejected {
            provider: provider.clone(),
            message,
        },
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited {
            provider: provider.clone(),
            retry_after_ms: None,
        },
        StatusCode::PAYLOAD_TOO_LARGE => ProviderError::ContextOverflow {
            provider: provider.clone(),
            message,
        },
        StatusCode::NOT_FOUND => ProviderError::ModelUnavailable {
            provider: provider.clone(),
            model: model.unwrap_or("unknown").to_owned(),
            message,
        },
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            ProviderError::InvalidRequest {
                provider: provider.clone(),
                message,
            }
        }
        _ => ProviderError::Http {
            provider: provider.clone(),
            message: format!("HTTP {status}: {message}"),
        },
    }
}
