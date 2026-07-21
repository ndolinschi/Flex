//! Isolated-workspace lifecycle: integrate, discard, revert, status.

use std::path::{Path, PathBuf};

use agentloop_contracts::{AgentEvent, IntegrationOutcome, SessionId, SessionMeta};
use agentloop_core::{Workspace, WorkspaceStatus};

use crate::EngineResult;
use crate::error::EngineServiceError;
use crate::service::EngineService;

impl EngineService {
    /// Enumerate provisioned workspaces that belong to `base` (still on
    /// disk), for the UI's "reuse workspace" picker. `Ok(vec![])` when no
    /// workspace backend is configured, `base` isn't a git repo, or the
    /// backend can't enumerate — never an error for those "nothing there"
    /// cases.
    pub async fn list_workspaces(&self, base: &Path) -> EngineResult<Vec<Workspace>> {
        match &self.workspace {
            Some(backend) => Ok(backend.list(base).await?),
            None => Ok(Vec::new()),
        }
    }

    /// Whether the session currently runs in an isolated workspace (still
    /// pointing at its worktree — false once integrated/discarded).
    pub async fn is_isolated(&self, session: &SessionId) -> EngineResult<bool> {
        Ok(active_workspace(&self.store.get_meta(session).await?).is_some())
    }

    /// Report the pending changes in a session's isolated workspace. `Ok(None)`
    /// when the session isn't isolated or no backend is configured.
    pub async fn workspace_status(
        &self,
        session: &SessionId,
    ) -> EngineResult<Option<WorkspaceStatus>> {
        let meta = self.store.get_meta(session).await?;
        let Some((_, _, root)) = active_workspace(&meta) else {
            return Ok(None);
        };
        match &self.workspace {
            Some(backend) => Ok(Some(backend.status(&root).await?)),
            None => Ok(None),
        }
    }

    /// Verify and integrate a session's isolated workspace back into its base
    /// tree. On a clean merge the workspace is removed and the session's cwd is
    /// repointed to the base directory; the outcome is returned to the caller.
    pub async fn integrate_session(&self, session: &SessionId) -> EngineResult<IntegrationOutcome> {
        let meta = self.store.get_meta(session).await?;
        let Some((_workspace_id, base, root)) = active_workspace(&meta) else {
            return Err(EngineServiceError::NotIsolated(session.clone()));
        };
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        let outcome = backend
            .integrate(&root, &base, self.verify_command.as_deref())
            .await?;
        if matches!(
            outcome,
            IntegrationOutcome::Merged { .. } | IntegrationOutcome::Empty
        ) {
            self.repoint_to_base(session, base).await?;
        }
        Ok(outcome)
    }

    /// Discard a session's isolated workspace without integrating, repointing
    /// the session to its base directory.
    pub async fn discard_session(&self, session: &SessionId) -> EngineResult<()> {
        let meta = self.store.get_meta(session).await?;
        let Some((_workspace_id, base, root)) = active_workspace(&meta) else {
            return Err(EngineServiceError::NotIsolated(session.clone()));
        };
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        backend.discard(&root, &base).await?;
        self.repoint_to_base(session, base).await?;
        Ok(())
    }

    /// Rewind a session's working tree to a prior per-turn snapshot (backs
    /// `/undo` and `/redo`). Restores files under the session's `cwd` without
    /// moving any git branch, then records a [`AgentEvent::SnapshotRestored`]
    /// audit marker (the append-only log is retained). Works whether or not the
    /// session is isolated. Errors if no workspace backend is configured or the
    /// snapshot id is unknown.
    pub async fn revert(&self, session: &SessionId, snapshot_id: &str) -> EngineResult<()> {
        let meta = self.store.get_meta(session).await?;
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        backend.restore(&meta.cwd, snapshot_id).await?;
        self.store
            .append(
                session,
                &[AgentEvent::SnapshotRestored {
                    snapshot_id: snapshot_id.to_owned(),
                }],
            )
            .await?;
        Ok(())
    }

    /// Point the session's cwd back at the base tree after its workspace is
    /// integrated or discarded (which also makes it read as no longer isolated).
    async fn repoint_to_base(&self, session: &SessionId, base: PathBuf) -> EngineResult<()> {
        self.store
            .update_meta(
                session,
                agentloop_contracts::SessionMetaPatch {
                    cwd: Some(base),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }
}

/// A session's *active* isolated workspace: `Some((workspace_id, base, root))`
/// only while its cwd still points at the worktree. Once integrate/discard
/// repoints cwd back to `base_cwd`, this returns `None` — so a session reads as
/// isolated exactly once, and a second integrate/discard is a clean no-op error
/// rather than operating on the base tree.
pub(crate) fn active_workspace(meta: &SessionMeta) -> Option<(String, PathBuf, PathBuf)> {
    let workspace_id = meta.workspace_id.clone()?;
    let base = meta.base_cwd.clone()?;
    if meta.cwd == base {
        return None;
    }
    Some((workspace_id, base, meta.cwd.clone()))
}
