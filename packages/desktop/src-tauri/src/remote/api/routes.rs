//! Route handlers for the desktop Remote Access `/v1` API.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use agentloop_contracts::{
    Answer, ErrorCode, ModelRef, NewSessionParams, PermissionRequestId, PromptInput, QuestionId,
    SessionId, SessionMetaPatch, TurnOptions,
};
use agentloop_engine::EngineServiceError;

use crate::commands::{self, McpServerDto};
use crate::remote::api::dto::{
    self, CreateSessionRequest, CreateSessionResponse, ErrorResponse, InfoResponse, McpServerBody,
    PermissionResolveRequest, PromptRequest, QuestionRespondRequest, SessionSummary,
    UpdateSessionRequest,
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
        .route("/sessions", get(list_sessions).post(create_session))
        .route(
            "/sessions/{id}",
            get(get_session)
                .patch(update_session)
                .delete(delete_session),
        )
        .route("/sessions/{id}/resume", post(resume_session))
        .route("/sessions/{id}/prompt", post(prompt))
        .route("/sessions/{id}/cancel", post(cancel))
        .route("/sessions/{id}/events", get(events))
        .route(
            "/sessions/{id}/permissions/{request_id}/resolve",
            post(resolve_permission),
        )
        .route(
            "/sessions/{id}/questions/{request_id}/respond",
            post(respond_question),
        )
        .route("/mcp/servers", get(mcp_list).put(mcp_upsert))
        .route("/mcp/servers/{id}", delete(mcp_remove))
        .route("/mcp/servers/{id}/test", post(mcp_test))
        .route("/providers", get(list_providers))
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

async fn create_session(
    State(state): State<RemoteApiState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    let service = require_service(&state.app).await?;
    let params = NewSessionParams {
        title: body.title,
        cwd: body.cwd.map(PathBuf::from),
        role: body.role,
        model: body.model.map(ModelRef),
        fallback_models: body.fallback_models.into_iter().map(ModelRef).collect(),
        ..NewSessionParams::default()
    };
    let session_id = service.create_session(params).await?;
    Ok(Json(CreateSessionResponse {
        session_id: session_id.as_str().to_owned(),
    }))
}

async fn list_sessions(
    State(state): State<RemoteApiState>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let service = require_service(&state.app).await?;
    let sessions = service.list_sessions().await?;
    Ok(Json(
        sessions.into_iter().map(SessionSummary::from).collect(),
    ))
}

async fn get_session(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<Json<SessionSummary>, ApiError> {
    let service = require_service(&state.app).await?;
    let meta = service.session_meta(&SessionId::from(id)).await?;
    Ok(Json(SessionSummary::from(meta)))
}

async fn update_session(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionRequest>,
) -> Result<Json<SessionSummary>, ApiError> {
    let service = require_service(&state.app).await?;
    let meta = service
        .update_session(
            &SessionId::from(id),
            SessionMetaPatch {
                title: body.title,
                model: body.model.map(ModelRef),
                cwd: body.cwd.map(PathBuf::from),
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(SessionSummary::from(meta)))
}

async fn delete_session(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    service.delete_session(&SessionId::from(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resume_session(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    service.resume_session(&SessionId::from(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn prompt(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
    Json(body): Json<PromptRequest>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    let session = SessionId::from(id);
    let permission_mode = match body.permission_mode {
        Some(raw) => Some(
            dto::parse_permission_mode(&raw)
                .map_err(|message| ApiError::from((StatusCode::BAD_REQUEST, message)))?,
        ),
        None => None,
    };
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

async fn cancel(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    service.cancel(&SessionId::from(id)).await?;
    Ok(StatusCode::NO_CONTENT)
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
    let stream = session_events_stream(&service, SessionId::from(id), query.from_seq).await?;
    Ok(stream)
}

async fn resolve_permission(
    State(state): State<RemoteApiState>,
    Path((id, request_id)): Path<(String, String)>,
    Json(body): Json<PermissionResolveRequest>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    service
        .respond_permission(
            &SessionId::from(id),
            PermissionRequestId::from(request_id),
            body.into(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn respond_question(
    State(state): State<RemoteApiState>,
    Path((id, request_id)): Path<(String, String)>,
    Json(body): Json<QuestionRespondRequest>,
) -> Result<StatusCode, ApiError> {
    let service = require_service(&state.app).await?;
    let answers: Vec<Answer> = body
        .answers
        .into_iter()
        .map(|a| Answer {
            question: a.question,
            selected: a.selected,
        })
        .collect();
    service
        .respond_question(&SessionId::from(id), QuestionId::from(request_id), answers)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_providers(
    State(state): State<RemoteApiState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let service = require_service(&state.app).await?;
    Ok(Json(
        service
            .provider_registry()
            .ids()
            .into_iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
    ))
}

async fn mcp_list(
    State(_state): State<RemoteApiState>,
) -> Result<Json<Vec<McpServerBody>>, ApiError> {
    let servers = commands::mcp_list_internal()
        .await
        .map_err(|e| ApiError::from((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())))?;
    Ok(Json(
        servers
            .into_iter()
            .map(|s| McpServerBody {
                id: s.id,
                command: s.command,
                args: s.args,
                env: s.env,
                secret_env: Default::default(),
                secret_args: None,
                enabled: s.enabled,
                configured_secret_env: s.configured_secret_env,
                has_secret_args: s.has_secret_args,
            })
            .collect(),
    ))
}

async fn mcp_upsert(
    State(state): State<RemoteApiState>,
    Json(body): Json<McpServerBody>,
) -> Result<StatusCode, ApiError> {
    let dto = McpServerDto {
        id: body.id,
        command: body.command,
        args: body.args,
        env: body.env,
        secret_env: body.secret_env,
        secret_args: body.secret_args,
        configured_secret_env: Vec::new(),
        has_secret_args: false,
        enabled: body.enabled,
    };
    let app_state = state.app.state::<AppState>();
    commands::mcp_upsert_internal(&state.app, &app_state, dto)
        .await
        .map_err(|e| ApiError::from((StatusCode::BAD_REQUEST, e.to_string())))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mcp_remove(
    State(state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let app_state = state.app.state::<AppState>();
    commands::mcp_remove_internal(&state.app, &app_state, id)
        .await
        .map_err(|e| ApiError::from((StatusCode::BAD_REQUEST, e.to_string())))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mcp_test(
    State(_state): State<RemoteApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let tools = commands::mcp_test_internal(id)
        .await
        .map_err(|e| ApiError::from((StatusCode::BAD_REQUEST, e.to_string())))?;
    Ok(Json(tools))
}
