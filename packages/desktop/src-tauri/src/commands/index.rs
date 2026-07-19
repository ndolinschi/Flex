//! Code index status and rebuild.

use super::prelude::*;

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn index_status(cwd: String) -> DesktopResult<agentloop_sdk::index::IndexStatus> {
    let path = PathBuf::from(cwd.trim());
    if path.as_os_str().is_empty() {
        return Err(DesktopError::Message("cwd is required".to_owned()));
    }
    tokio::task::spawn_blocking(move || agentloop_sdk::index::status_for(&path))
        .await
        .map_err(|e| DesktopError::Message(format!("index_status worker failed: {e}")))?
        .map_err(|e| DesktopError::Message(e.to_string()))
}

/// Force a (re)build of the code index for `cwd`. Returns status + update
/// stats so Settings can show progress after a rebuild click.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn index_rebuild(cwd: String) -> DesktopResult<IndexRebuildResult> {
    let path = PathBuf::from(cwd.trim());
    if path.as_os_str().is_empty() {
        return Err(DesktopError::Message("cwd is required".to_owned()));
    }
    let (status, stats) =
        tokio::task::spawn_blocking(move || agentloop_sdk::index::rebuild_with_stats(&path))
            .await
            .map_err(|e| DesktopError::Message(format!("index_rebuild worker failed: {e}")))?
            .map_err(DesktopError::Message)?;
    Ok(IndexRebuildResult { status, stats })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRebuildResult {
    pub status: agentloop_sdk::index::IndexStatus,
    pub stats: agentloop_sdk::index::UpdateStats,
}
