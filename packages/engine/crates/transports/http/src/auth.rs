use std::net::{IpAddr, SocketAddr};

use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn generate() -> Self {
        Self(format!(
            "{}{}",
            uuid::Uuid::now_v7().simple(),
            uuid::Uuid::now_v7().simple()
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn matches(&self, presented: &str) -> bool {
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }
}

pub struct BindTarget {
    pub addr: SocketAddr,
}

pub fn validate_bind(addr: SocketAddr, token_was_explicit: bool) -> Result<BindTarget, String> {
    if !is_loopback(addr.ip()) && !token_was_explicit {
        return Err(format!(
            "refusing to bind {addr} (non-loopback) without an explicit auth token — \
             auto-generated tokens are for local-only use. Pass an explicit token if you \
             really want this reachable off this host."
        ));
    }
    Ok(BindTarget { addr })
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

pub async fn require_bearer_token(
    State(token): State<AuthToken>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let presented = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match presented {
        Some(presented) if token.matches(presented) => next.run(request).await,
        _ => (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_exact_token_only() {
        let token = AuthToken::new("secret-value");
        assert!(token.matches("secret-value"));
        assert!(!token.matches("secret-valu"));
        assert!(!token.matches("secret-value-extra"));
        assert!(!token.matches(""));
    }

    #[test]
    fn loopback_bind_is_always_allowed() {
        let addr: SocketAddr = "127.0.0.1:4517".parse().unwrap();
        assert!(validate_bind(addr, false).is_ok());
        let addr_v6: SocketAddr = "[::1]:4517".parse().unwrap();
        assert!(validate_bind(addr_v6, false).is_ok());
    }

    #[test]
    fn non_loopback_bind_requires_an_explicit_token() {
        let addr: SocketAddr = "0.0.0.0:4517".parse().unwrap();
        assert!(validate_bind(addr, false).is_err());
        assert!(validate_bind(addr, true).is_ok());
    }
}
