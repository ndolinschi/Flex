use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use reqwest::header::HeaderMap;
use reqwest::{Client, Method, RequestBuilder, StatusCode};

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

pub fn retry_after_ms_from_headers(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    Some(seconds.saturating_mul(1000))
}

pub fn is_retryable_transport_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect()
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

    #[test]
    fn retry_after_parses_delay_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "20".parse().unwrap());
        assert_eq!(retry_after_ms_from_headers(&headers), Some(20_000));
    }

    #[test]
    fn retry_after_missing_header_is_none() {
        let headers = HeaderMap::new();
        assert_eq!(retry_after_ms_from_headers(&headers), None);
    }

    #[test]
    fn retry_after_unparsable_value_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Fri, 31 Dec 2021 23:59:59 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after_ms_from_headers(&headers), None);
    }
}
