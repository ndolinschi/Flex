//! The `SessionStore` trait: an append-only event log per session
//! (the managed-agents pattern — the log, not the process, is the ground
//! truth, so any implementation can be resumed from its store alone).

use async_trait::async_trait;

use agentloop_contracts::{
    AgentEvent, CheckpointRef, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch,
};

/// One persisted event as returned by [`SessionStore::read`]: its per-session
/// sequence number, the wall-clock time it was appended (`ts_ms`), and the
/// event itself. `ts_ms` is captured once at append and is stable across
/// reopen — replaying a session yields the same per-event `ts_ms` it had when
/// live, so historical timestamps are never rewritten to "now".
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub seq: u64,
    pub ts_ms: u64,
    pub event: AgentEvent,
}

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

    /// Read events with `seq >= from_seq`, in order. Each [`StoredEvent`]
    /// carries the wall-clock `ts_ms` captured when the event was appended.
    async fn read(&self, id: &SessionId, from_seq: u64) -> Result<Vec<StoredEvent>, StoreError>;

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

    /// Record a named pointer at a `seq` the log already contains. Not
    /// separate storage — restoring one is `reduce()` over `read(0)`
    /// truncated at `checkpoint.seq`. Default no-op so existing
    /// implementations keep compiling; real stores override both this and
    /// [`Self::list_checkpoints`] together.
    async fn record_checkpoint(
        &self,
        _id: &SessionId,
        _checkpoint: CheckpointRef,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Checkpoints recorded for `id`, oldest first. Default empty.
    async fn list_checkpoints(&self, _id: &SessionId) -> Result<Vec<CheckpointRef>, StoreError> {
        Ok(Vec::new())
    }
}
