//! Headless HTTP/SSE transport boundary for `EngineService`, with OpenAPI.
//!
//! Security posture (non-negotiable, not a knob to relax later): binds
//! `127.0.0.1` unless the caller opts into a wider bind *and* supplies an
//! explicit token; every route but `/health` requires a bearer token; no
//! CORS layer, so browser-origin requests are refused by the browser itself
//! (nothing here whitelists an `Access-Control-Allow-Origin`).

mod auth;
mod dto;
mod openapi;
mod routes;
mod sse;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::routing::get;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

use agentloop_engine::EngineService;

pub use auth::{AuthToken, validate_bind};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HttpServeError {
    #[error("invalid bind configuration: {0}")]
    InvalidBind(String),
    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("server error: {0}")]
    Server(std::io::Error),
}

pub struct HttpServeOptions {
    pub bind: SocketAddr,
    pub token: AuthToken,
    /// Whether `token` was explicitly provided (flag/env) vs auto-generated.
    /// Re-validated here (not just at CLI parse time) so embedders calling
    /// `serve_http` directly get the same non-loopback guard.
    pub token_was_explicit: bool,
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn openapi_json(State(_service): State<Arc<EngineService>>) -> axum::Json<serde_json::Value> {
    axum::Json(openapi::ApiDoc::openapi().to_json().map_or_else(
        |_| serde_json::json!({}),
        |json| serde_json::from_str(&json).unwrap_or_default(),
    ))
}

/// Build the full router: `/health` unauthenticated, everything else behind
/// the bearer-token middleware.
pub fn build_router(service: Arc<EngineService>, token: AuthToken) -> Router {
    let authenticated = routes::router()
        .route("/openapi.json", get(openapi_json))
        .layer(middleware::from_fn_with_state(
            token,
            auth::require_bearer_token,
        ))
        .with_state(service);

    Router::new()
        .route("/health", get(health))
        .merge(authenticated)
        .layer(TraceLayer::new_for_http())
}

/// Serve `engine` over HTTP until the process is signaled to stop (this
/// function runs the accept loop forever on success — callers that want a
/// shutdown hook should race this future against their own signal).
pub async fn serve_http(
    engine: Arc<EngineService>,
    opts: HttpServeOptions,
) -> Result<(), HttpServeError> {
    auth::validate_bind(opts.bind, opts.token_was_explicit).map_err(HttpServeError::InvalidBind)?;
    let router = build_router(engine, opts.token);
    let listener = tokio::net::TcpListener::bind(opts.bind)
        .await
        .map_err(|source| HttpServeError::Bind {
            addr: opts.bind,
            source,
        })?;
    axum::serve(listener, router)
        .await
        .map_err(HttpServeError::Server)
}
