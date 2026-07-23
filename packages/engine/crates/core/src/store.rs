use async_trait::async_trait;

use agentloop_contracts::{
    AgentEvent, CheckpointRef, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch,
};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub seq: u64,
    pub ts_ms: u64,
    pub event: AgentEvent,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error("session {0} not found")]
    SessionNotFound(SessionId),
    #[error("session {0} already exists")]
    SessionExists(SessionId),
    #[error("storage I/O failure: {0}")]
    Io(String),
    #[error("corrupt session data: {0}")]
    Corrupt(String),
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, meta: SessionMeta) -> Result<(), StoreError>;

    async fn append(&self, id: &SessionId, events: &[AgentEvent]) -> Result<u64, StoreError>;

    async fn read(&self, id: &SessionId, from_seq: u64) -> Result<Vec<StoredEvent>, StoreError>;

    async fn list(&self) -> Result<Vec<SessionMeta>, StoreError>;

    async fn get_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError>;

    async fn update_meta(&self, id: &SessionId, patch: SessionMetaPatch) -> Result<(), StoreError>;

    async fn delete(&self, id: &SessionId) -> Result<(), StoreError>;

    async fn record_compaction(
        &self,
        id: &SessionId,
        compaction: CompactionSummary,
    ) -> Result<(), StoreError>;

    async fn record_checkpoint(
        &self,
        _id: &SessionId,
        _checkpoint: CheckpointRef,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    async fn list_checkpoints(&self, _id: &SessionId) -> Result<Vec<CheckpointRef>, StoreError> {
        Ok(Vec::new())
    }
}
