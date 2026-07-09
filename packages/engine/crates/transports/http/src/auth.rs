//! Bearer-token auth: constant-time comparison, and a hard refusal to bind
//! non-loopback without an explicit token. Three competing agent runtimes
//! shipped unauthenticated-by-default HTTP servers and paid for it (an
//! unauthenticated RCE, ~1000 exposed instances found via Shodan, an
//! unauthenticated API) — this transport does not repeat that mistake.

use std::net::{IpAddr, SocketAddr};

use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

/// The bearer token guarding every route except `/health`.
#[derive(Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// A fresh, randomly generated token (32 bytes, hex-encoded).
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
        // Constant-time so response latency can't leak how many leading
        // bytes matched. Length must match first (ct_eq requires equal-
        // length inputs); a length mismatch is already a safe, non-secret
        // signal (any real token has a fixed known length).
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }
}

/// Where the server will listen. Refuses to construct a non-loopback bind
/// unless a token was explicitly provided (not auto-generated) — an
/// auto-generated token is meant for local-only use and printed once to
/// stderr, never for a bind reachable off the host.
pub struct BindTarget {
    pub addr: SocketAddr,
}

/// Validate a requested bind address against the token's provenance.
/// `token_was_explicit` is `true` when the caller passed `--token` or set
/// `FLEX_SERVE_TOKEN`; `false` when the token was auto-generated.
pub fn validate_bind(addr: SocketAddr, token_was_explicit: bool) -> Result<BindTarget, String> {
    if !is_loopback(addr.ip()) && !token_was_explicit {
        return Err(format!(
            "refusing to bind {addr} (non-loopback) without an explicit auth token — \
             auto-generated tokens are for local-only use. Pass --token or set \
             FLEX_SERVE_TOKEN if you really want this reachable off this host."
        ));
    }
    Ok(BindTarget { addr })
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

/// Axum middleware: every request must carry `Authorization: Bearer <token>`
/// matching the configured token, or gets `401`. Applied to every route
/// except `/health` (mounted outside this layer).
pub(crate) async fn require_bearer_token(
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
