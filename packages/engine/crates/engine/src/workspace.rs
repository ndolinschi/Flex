use std::path::{Path, PathBuf};

use agentloop_contracts::{AgentEvent, IntegrationOutcome, SessionId, SessionMeta};
use agentloop_core::{Workspace, WorkspaceStatus};

use crate::EngineResult;
use crate::error::EngineServiceError;
use crate::service::EngineService;

impl EngineService {
    pub async fn list_workspaces(&self, base: &Path) -> EngineResult<Vec<Workspace>> {
        match &self.workspace {
            Some(backend) => Ok(backend.list(base).await?),
            None => Ok(Vec::new()),
        }
    }

    pub async fn is_isolated(&self, session: &SessionId) -> EngineResult<bool> {
        Ok(active_workspace(&self.store.get_meta(session).await?).is_some())
    }

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

pub(crate) fn active_workspace(meta: &SessionMeta) -> Option<(String, PathBuf, PathBuf)> {
    let workspace_id = meta.workspace_id.clone()?;
    let base = meta.base_cwd.clone()?;
    if meta.cwd == base {
        return None;
    }
    Some((workspace_id, base, meta.cwd.clone()))
}
