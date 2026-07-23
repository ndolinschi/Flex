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

pub use auth::{AuthToken, require_bearer_token, validate_bind};

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

pub fn build_router(service: Arc<EngineService>, token: AuthToken) -> Router {
    build_router_with_extra(service, token, Router::new())
}

pub fn build_router_with_extra(
    service: Arc<EngineService>,
    token: AuthToken,
    extra: Router,
) -> Router {
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
        .merge(extra)
        .layer(TraceLayer::new_for_http())
}

pub async fn serve_http(
    engine: Arc<EngineService>,
    opts: HttpServeOptions,
) -> Result<(), HttpServeError> {
    serve_http_with_extra(engine, opts, Router::new()).await
}

pub async fn serve_http_with_extra(
    engine: Arc<EngineService>,
    opts: HttpServeOptions,
    extra: Router,
) -> Result<(), HttpServeError> {
    auth::validate_bind(opts.bind, opts.token_was_explicit).map_err(HttpServeError::InvalidBind)?;
    let router = build_router_with_extra(engine, opts.token, extra);
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
