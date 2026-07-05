//! The `SessionStore` trait: an append-only event log per session
//! (the managed-agents pattern — the log, not the process, is the ground
//! truth, so any implementation can be resumed from its store alone).

use async_trait::async_trait;

use agentloop_contracts::{
    AgentEvent, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch,
};

/// Storage failures.
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

/// Append-only session storage. `append` assigns monotonic per-session
/// sequence numbers; implementations must persist an event before it is
/// considered appended (the caller broadcasts only after `append` returns).
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, meta: SessionMeta) -> Result<(), StoreError>;

    /// Append persisted events, returning the sequence number assigned to the
    /// *first* event of the batch (consecutive numbers follow).
    async fn append(&self, id: &SessionId, events: &[AgentEvent]) -> Result<u64, StoreError>;

    /// Read events with `seq >= from_seq`, in order.
    async fn read(
        &self,
        id: &SessionId,
        from_seq: u64,
    ) -> Result<Vec<(u64, AgentEvent)>, StoreError>;

    async fn list(&self) -> Result<Vec<SessionMeta>, StoreError>;

    async fn get_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError>;

    async fn update_meta(&self, id: &SessionId, patch: SessionMetaPatch) -> Result<(), StoreError>;

    async fn delete(&self, id: &SessionId) -> Result<(), StoreError>;

    /// Record a compaction. The raw log is retained; the compaction event in
    /// the log is what shapes future context building.
    async fn record_compaction(
        &self,
        id: &SessionId,
        compaction: CompactionSummary,
    ) -> Result<(), StoreError>;
}
