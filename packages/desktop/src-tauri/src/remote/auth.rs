
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

    pub fn matches(&self, presented: &str) -> bool {
        if self.0.len() != presented.len() {
            return false;
        }
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }
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
}
