use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use agentloop_contracts::{
    ErrorCode, NewSessionParams, PermissionRequestId, PromptInput, SessionId, TurnOptions,
};
use agentloop_engine::{EngineService, EngineServiceError};

use crate::dto::{
    CreateSessionRequest, CreateSessionResponse, ErrorResponse, PermissionResolveRequest,
    PromptRequest, SessionSummary, parse_permission_mode,
};
use crate::sse::session_events_stream;

pub(crate) type AppState = Arc<EngineService>;

pub(crate) struct AppError(StatusCode, ErrorResponse);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl From<EngineServiceError> for AppError {
    fn from(err: EngineServiceError) -> Self {
        let engine_error = err.to_engine_error();
        let status = status_for(engine_error.code);
        AppError(status, ErrorResponse::new(engine_error.message))
    }
}

impl From<(StatusCode, String)> for AppError {
    fn from((status, message): (StatusCode, String)) -> Self {
        AppError(status, ErrorResponse::new(message))
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
        ErrorCode::ModelUnavailable
        | ErrorCode::ProcessCrashed
        | ErrorCode::ProtocolViolation
        | ErrorCode::NotInstalled
        | ErrorCode::ContextOverflow
        | ErrorCode::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/sessions", post(create_session).get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/prompt", post(prompt))
        .route("/sessions/{id}/events", get(events))
        .route("/sessions/{id}/cancel", post(cancel))
        .route(
            "/sessions/{id}/permissions/{request_id}/resolve",
            post(resolve_permission),
        )
}

#[utoipa::path(
    post,
    path = "/sessions",
    request_body = CreateSessionRequest,
    responses((status = 200, body = CreateSessionResponse)),
    tag = "sessions"
)]
pub(crate) async fn create_session(
    State(service): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, AppError> {
    let params = NewSessionParams {
        title: body.title,
        cwd: body.cwd.map(PathBuf::from),
        role: body.role,
        model: body.model.map(agentloop_contracts::ModelRef),
        fallback_models: body
            .fallback_models
            .into_iter()
            .map(agentloop_contracts::ModelRef)
            .collect(),
        ..NewSessionParams::default()
    };
    let session_id = service.create_session(params).await?;
    Ok(Json(CreateSessionResponse {
        session_id: session_id.as_str().to_owned(),
    }))
}

#[utoipa::path(
    get,
    path = "/sessions",
    responses((status = 200, body = Vec<SessionSummary>)),
    tag = "sessions"
)]
pub(crate) async fn list_sessions(
    State(service): State<AppState>,
) -> Result<Json<Vec<SessionSummary>>, AppError> {
    let sessions = service.list_sessions().await?;
    Ok(Json(
        sessions.into_iter().map(SessionSummary::from).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/sessions/{id}",
    params(("id" = String, Path, description = "Session id")),
    responses((status = 200, body = SessionSummary)),
    tag = "sessions"
)]
pub(crate) async fn get_session(
    State(service): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionSummary>, AppError> {
    let meta = service.session_meta(&SessionId::from(id)).await?;
    Ok(Json(SessionSummary::from(meta)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/prompt",
    params(("id" = String, Path, description = "Session id")),
    request_body = PromptRequest,
    responses((status = 202, description = "Turn admitted; watch /events for progress")),
    tag = "sessions"
)]
pub(crate) async fn prompt(
    State(service): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PromptRequest>,
) -> Result<StatusCode, AppError> {
    let session = SessionId::from(id);
    let permission_mode = match body.permission_mode {
        Some(raw) => Some(
            parse_permission_mode(&raw)
                .map_err(|message| AppError::from((StatusCode::BAD_REQUEST, message)))?,
        ),
        None => None,
    };
    let service = service.clone();
    tokio::spawn(async move {
        let _ = service
            .prompt(
                &session,
                PromptInput::text(body.prompt),
                TurnOptions {
                    permission_mode,
                    ..TurnOptions::default()
                },
            )
            .await;
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    #[serde(default)]
    from_seq: u64,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/events",
    params(
        ("id" = String, Path, description = "Session id"),
        ("from_seq" = Option<u64>, Query, description = "Replay from this seq, then tail live"),
    ),
    responses((status = 200, description = "text/event-stream of SessionEvent JSON")),
    tag = "sessions"
)]
pub(crate) async fn events(
    State(service): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<Response, AppError> {
    let session = SessionId::from(id);
    let sse = session_events_stream(&service, session, query.from_seq).await?;
    Ok(sse.into_response())
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/cancel",
    params(("id" = String, Path, description = "Session id")),
    responses((status = 204, description = "Cancel requested")),
    tag = "sessions"
)]
pub(crate) async fn cancel(
    State(service): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    service.cancel(&SessionId::from(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/permissions/{request_id}/resolve",
    params(
        ("id" = String, Path, description = "Session id"),
        ("request_id" = String, Path, description = "Permission request id"),
    ),
    request_body = PermissionResolveRequest,
    responses((status = 204, description = "Decision recorded")),
    tag = "sessions"
)]
pub(crate) async fn resolve_permission(
    State(service): State<AppState>,
    Path((id, request_id)): Path<(String, String)>,
    Json(body): Json<PermissionResolveRequest>,
) -> Result<StatusCode, AppError> {
    service
        .respond_permission(
            &SessionId::from(id),
            PermissionRequestId::from(request_id),
            body.into(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
