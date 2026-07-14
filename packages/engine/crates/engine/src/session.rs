//! Session CRUD and metadata.

use agentloop_contracts::{NewSessionParams, SessionId, SessionMeta, SessionMetaPatch};

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    pub async fn create_session(&self, mut params: NewSessionParams) -> EngineResult<SessionId> {
        if params.isolation.is_none() && self.isolation_default.wants_isolation() {
            params.isolation = Some(self.isolation_default);
        }
        Ok(self.agent.create_session(params).await?)
    }
    pub async fn resume_session(&self, id: &SessionId) -> EngineResult<()> {
        Ok(self.agent.resume_session(id).await?)
    }

    /// Load persisted metadata for a session.
    pub async fn session_meta(&self, session: &SessionId) -> EngineResult<SessionMeta> {
        Ok(self.store.get_meta(session).await?)
    }

    pub async fn list_sessions(&self) -> EngineResult<Vec<SessionMeta>> {
        Ok(self.agent.list_sessions().await?)
    }

    /// Apply a partial update to a session's persisted metadata (e.g. rename
    /// via `title`, or patch the default `model`).
    pub async fn update_session(
        &self,
        session: &SessionId,
        patch: SessionMetaPatch,
    ) -> EngineResult<SessionMeta> {
        self.store.update_meta(session, patch).await?;
        Ok(self.store.get_meta(session).await?)
    }

    /// Cancel any in-flight turn, kill any background processes the session
    /// started via `Bash`'s `run_in_background` (a dev server left running
    /// on a deleted session would otherwise leak forever), then delete the
    /// session's event log and metadata from the store.
    pub async fn delete_session(&self, session: &SessionId) -> EngineResult<()> {
        let _ = self.agent.cancel(session).await;
        if let Some(registry) = &self.background_processes {
            registry.kill_session(session).await;
        }
        Ok(self.store.delete(session).await?)
    }
}
