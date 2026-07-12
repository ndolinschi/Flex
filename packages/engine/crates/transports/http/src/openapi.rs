//! `GET /openapi.json`: the spec, generated from the same `#[utoipa::path]`
//! annotations and `ToSchema` derives that define the routes — docs and code
//! can't silently drift apart the way a hand-maintained spec would.

use utoipa::OpenApi;

use crate::dto::{
    CreateSessionRequest, CreateSessionResponse, ErrorResponse, PermissionResolveDecision,
    PermissionResolveRequest, PromptRequest, SessionSummary,
};
use crate::routes;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Agent-loop engine API",
        description = "Headless HTTP/SSE control surface over EngineService: session \
                        lifecycle, prompting, permission resolution, and event streaming."
    ),
    paths(
        routes::create_session,
        routes::list_sessions,
        routes::get_session,
        routes::prompt,
        routes::events,
        routes::cancel,
        routes::resolve_permission,
    ),
    components(schemas(
        CreateSessionRequest,
        CreateSessionResponse,
        SessionSummary,
        PromptRequest,
        PermissionResolveRequest,
        PermissionResolveDecision,
        ErrorResponse,
    ))
)]
pub(crate) struct ApiDoc;
