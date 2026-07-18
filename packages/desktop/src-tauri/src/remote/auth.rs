//! Bearer-token auth for the desktop Remote Access HTTP surface.

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

    /// A fresh, randomly generated token (two UUIDs, hex-concatenated).
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

    pub fn matches(&self, presented: &str) -> bool {
        // Length mismatch is non-secret (tokens are fixed length) but still
        // avoid short-circuiting on content bytes: only compare when lengths
        // match; otherwise return false.
        if self.0.len() != presented.len() {
            return false;
        }
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }
}

/// Axum middleware: every request must carry `Authorization: Bearer <token>`
/// matching the configured token, or gets `401`.
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
}
