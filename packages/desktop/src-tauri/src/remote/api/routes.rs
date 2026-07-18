//! Route handlers for the desktop Remote Access `/v1` API.
//!
//! **Least privilege (non-negotiable):** a remote client may only
//! - list / get root session titles,
//! - read chat message events (filtered SSE),
//! - send a text prompt (`disable_tools` + `DontAsk`).
//!
//! No session create/delete/update, no MCP, no providers, no HITL resolve,
//! no permission_mode override, no cancel, no subagent sessions, no tools.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use agentloop_contracts::{ErrorCode, PermissionMode, PromptInput, SessionId, TurnOptions};
use agentloop_engine::EngineServiceError;

use crate::remote::api::dto::{
    ErrorResponse, InfoResponse, PromptRequest, SessionSummary, MAX_PROMPT_CHARS,
};
use crate::remote::api::openapi::openapi_json;
use crate::remote::api::sse::session_events_stream;
use crate::remote::config::RemoteAccessConfig;
use crate::remote::pairing::{CAPABILITIES, PROTOCOL_VERSION};
use crate::state::AppState;

/// Shared axum state: Tauri app handle + a snapshot of remote config for `/v1/info`.
#[derive(Clone)]
pub struct RemoteApiState {
    pub app: AppHandle,
    pub config: Arc<tokio::sync::RwLock<RemoteAccessConfig>>,
}

struct ApiError(StatusCode, ErrorResponse);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl From<EngineServiceError> for ApiError {
    fn from(err: EngineServiceError) -> Self {
        let engine_error = err.to_engine_error();
        let status = status_for(engine_error.code);
        ApiError(status, ErrorResponse::new(engine_error.message))
    }
}

impl From<(StatusCode, String)> for ApiError {
    fn from((status, message): (StatusCode, String)) -> Self {
        ApiError(status, ErrorResponse::new(message))
    }
}

fn status_for(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::AuthMissing | ErrorCode::AuthExpired => StatusCode::UNAUTHORIZED,
        ErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
        ErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Timeout => StatusCode::GATEWAY_TIMEOUT,
        ErrorCode::Cancelled => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn require_service(app: &AppHandle) -> Result<agentloop_sdk::EngineService, ApiError> {
    let state = app.state::<AppState>();
    let guard = state.service.lock().await;
    guard.clone().ok_or_else(|| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorResponse::new("engine is not configured — save a provider first"),
        )
    })
}

pub fn v1_router() -> Router<RemoteApiState> {
    Router::new()
        .route("/info", get(info))
        .route("/openapi.json", get(openapi))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/prompt", post(prompt))
        .route("/sessions/{id}/events", get(events))
}

async fn info(State(state): State<RemoteApiState>) -> Result<Json<InfoResponse>, ApiError> {
    let cfg = state.config.read().await;
    Ok(Json(InfoResponse {
        protocol_version: PROTOCOL_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        device_name: cfg.device_name.clone(),
        device_id: cfg.device_id.clone(),
        capabilities: CAPABILITIES.iter().map(|s| (*s).to_owned()).collect(),
        openapi_url: "/v1/openapi.json".into(),
    }))
}

async fn openapi() -> Json<serde_json::Value> {
    Json(openapi_json())
}

async fn list_sessions(
    State(state): State<RemoteApiState>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let service = require_service(&state.app).await?;
    let sessions = service.list_sessions().await?;
    // Root chats only — subagent sessions are not a remote chat target.
    Ok(Json(
        sessions
            .into_iter()
            .filter(|m| m.depth == 0 && m.parent_id.is_none())
            .map(SessionSummary::from)
            .collect(),
    ))
}

async fn get_session(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<Json<SessionSummary>, ApiError> {
    let service = require_service(&state.app).await?;
    let meta = service.session_meta(&SessionId::from(id)).await?;
    if meta.depth != 0 || meta.parent_id.is_some() {
        return Err(ApiError::from((
            StatusCode::FORBIDDEN,
            "remote chat may only target root sessions".into(),
        )));
    }
    Ok(Json(SessionSummary::from(meta)))
}

async fn prompt(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
    Json(body): Json<PromptRequest>,
) -> Result<StatusCode, ApiError> {
    let text = body.prompt.trim();
    if text.is_empty() {
        return Err(ApiError::from((
            StatusCode::BAD_REQUEST,
            "prompt must not be empty".into(),
        )));
    }
    if text.chars().count() > MAX_PROMPT_CHARS {
        return Err(ApiError::from((
            StatusCode::BAD_REQUEST,
            format!("prompt exceeds {MAX_PROMPT_CHARS} character limit"),
        )));
    }
    let prompt = text.to_owned();
    let service = require_service(&state.app).await?;
    let session = SessionId::from(id);
    // Verify the target is a root chat session — never a subagent child.
    let meta = service.session_meta(&session).await?;
    if meta.depth != 0 || meta.parent_id.is_some() {
        return Err(ApiError::from((
            StatusCode::FORBIDDEN,
            "remote chat may only target root sessions".into(),
        )));
    }
    // Isolation: no tools offered to the model, and any tool call is denied.
    // DontAsk alone is insufficient — PermissionHint::Never tools (Read/Grep)
    // would still run and could exfiltrate files into the assistant reply.
    tokio::spawn(async move {
        let _ = service
            .prompt(
                &session,
                PromptInput::text(prompt),
                TurnOptions {
                    permission_mode: Some(PermissionMode::DontAsk),
                    disable_tools: true,
                    system_append: Some(
                        "You are responding to a remote chat companion. Reply in text only. \
                         You have no tools on this turn — do not attempt tool calls."
                            .into(),
                    ),
                    ..TurnOptions::default()
                },
            )
            .await;
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    from_seq: u64,
}

async fn events(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let service = require_service(&state.app).await?;
    let session = SessionId::from(id);
    let meta = service.session_meta(&session).await?;
    if meta.depth != 0 || meta.parent_id.is_some() {
        return Err(ApiError::from((
            StatusCode::FORBIDDEN,
            "remote chat may only target root sessions".into(),
        )));
    }
    let stream = session_events_stream(&service, session, query.from_seq).await?;
    Ok(stream)
}
