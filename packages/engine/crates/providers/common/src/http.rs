//! HTTP helpers shared by provider clients.

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use reqwest::{Client, Method, RequestBuilder, StatusCode};

/// Build an authenticated request using bearer-token auth. An empty key
/// sends no Authorization header (keyless local endpoints like LM Studio).
pub fn authenticated_request(
    client: &Client,
    method: Method,
    url: &str,
    api_key: &str,
) -> RequestBuilder {
    let request = client
        .request(method, url)
        .header("accept", "application/json");
    if api_key.is_empty() {
        request
    } else {
        request.bearer_auth(api_key)
    }
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
    if looks_like_context_overflow(&message) {
        return ProviderError::ContextOverflow {
            provider: provider.clone(),
            message,
        };
    }
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

/// True when a provider body reports the prompt exceeded its context window.
///
/// Copilot returns HTTP 400 with text like
/// `prompt token count of 383156 exceeds the limit of 136000`.
pub fn looks_like_context_overflow(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let tokenish = lower.contains("token")
        || lower.contains("context")
        || lower.contains("prompt")
        || lower.contains("maximum");
    if !tokenish {
        return false;
    }
    lower.contains("exceed")
        || lower.contains("too large")
        || lower.contains("too long")
        || lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("maximum context")
        || lower.contains("max context")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::ProviderId;

    #[test]
    fn copilot_token_limit_maps_to_context_overflow() {
        let provider = ProviderId::from("copilot");
        let body = "prompt token count of 383156 exceeds the limit of 136000";
        let err = status_to_provider_error(
            &provider,
            StatusCode::BAD_REQUEST,
            body.to_owned(),
            Some("claude-haiku-4.5"),
        );
        assert!(matches!(err, ProviderError::ContextOverflow { .. }));
    }

    #[test]
    fn unrelated_bad_request_stays_invalid() {
        let provider = ProviderId::from("copilot");
        let err = status_to_provider_error(
            &provider,
            StatusCode::BAD_REQUEST,
            "unknown tool name".to_owned(),
            None,
        );
        assert!(matches!(err, ProviderError::InvalidRequest { .. }));
    }

    #[test]
    fn empty_key_sends_no_authorization_header() {
        let client = Client::new();
        let with_key = authenticated_request(&client, Method::GET, "http://localhost/v1", "sk-x")
            .build()
            .expect("request builds");
        assert!(
            with_key
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION)
        );

        let keyless = authenticated_request(&client, Method::GET, "http://localhost/v1", "")
            .build()
            .expect("request builds");
        assert!(
            !keyless
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION)
        );
    }
}
