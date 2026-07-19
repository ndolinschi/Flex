//! Workspace isolation integrate/discard/revert.

use super::common::require_service;
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatusDto {
    pub files_changed: u32,
    pub summary: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn is_isolated(state: State<'_, AppState>, session_id: String) -> DesktopResult<bool> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.is_isolated(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn workspace_status(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<Option<WorkspaceStatusDto>> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let status: Option<WorkspaceStatus> = service.workspace_status(&id).await?;
    Ok(status.map(|s| WorkspaceStatusDto {
        files_changed: s.files_changed,
        summary: s.summary,
    }))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn integrate_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<IntegrationOutcome> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.integrate_session(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn discard_session(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.discard_session(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn revert(
    state: State<'_, AppState>,
    session_id: String,
    snapshot_id: String,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.revert(&id, &snapshot_id).await?)
}
