//! In-memory [`SessionStore`] backed by a mutex-guarded map.
//!
//! Intended for tests and short-lived embedded use; nothing survives the
//! process. The JSONL store (M2) shares the same trait semantics, so callers
//! can swap implementations without behavioral drift.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use agentloop_contracts::{
    AgentEvent, CheckpointRef, CompactionSummary, SessionId, SessionMeta, SessionMetaPatch, now_ms,
};
use agentloop_core::{SessionStore, StoreError, StoredEvent};

/// Everything stored for one session: its metadata plus the append-only
/// event log. An event's sequence number is its index in `events`, so seqs
/// are gapless and start at 0 by construction. Each entry pairs the event
/// with the wall-clock `ts_ms` captured at append, mirroring the JSONL store
/// so conformance parity holds.
#[derive(Debug)]
struct Record {
    meta: SessionMeta,
    events: Vec<(u64, AgentEvent)>,
    checkpoints: Vec<CheckpointRef>,
}

/// In-memory [`SessionStore`].
///
/// All state lives in a single `std::sync::Mutex<HashMap<..>>`. The lock is
/// never held across an `.await` (every method completes synchronously after
/// taking it), so the store is safe to call from any async context. A
/// poisoned lock is recovered by taking the inner value: every method leaves
/// the map in a consistent state before any point that could panic, so the
/// data is still coherent after a poison.
#[derive(Debug, Default)]
pub struct MemoryStore {
    sessions: Mutex<HashMap<SessionId, Record>>,
}

impl MemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lock the session map, recovering from poisoning.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Record>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl SessionStore for MemoryStore {
    async fn create(&self, meta: SessionMeta) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        if sessions.contains_key(&meta.id) {
            return Err(StoreError::SessionExists(meta.id));
        }
        sessions.insert(
            meta.id.clone(),
            Record {
                meta,
                events: Vec::new(),
                checkpoints: Vec::new(),
            },
        );
        Ok(())
    }

    async fn append(&self, id: &SessionId, events: &[AgentEvent]) -> Result<u64, StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        let first_seq = record.events.len() as u64;
        if !events.is_empty() {
            // Stamp emit time once per batch, matching the JSONL store so the
            // two implementations stay behavior-identical under conformance.
            let ts_ms = now_ms();
            record
                .events
                .extend(events.iter().map(|event| (ts_ms, event.clone())));
            record.meta.updated_at_ms = ts_ms;
        }
        Ok(first_seq)
    }

    async fn read(&self, id: &SessionId, from_seq: u64) -> Result<Vec<StoredEvent>, StoreError> {
        let sessions = self.lock();
        let record = sessions
            .get(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        Ok(record
            .events
            .iter()
            .enumerate()
            .filter(|(seq, _)| *seq as u64 >= from_seq)
            .map(|(seq, (ts_ms, event))| StoredEvent {
                seq: seq as u64,
                ts_ms: *ts_ms,
                event: event.clone(),
            })
            .collect())
    }

    async fn list(&self) -> Result<Vec<SessionMeta>, StoreError> {
        let sessions = self.lock();
        let mut metas: Vec<SessionMeta> = sessions.values().map(|r| r.meta.clone()).collect();
        metas.sort_by_key(|m| std::cmp::Reverse(m.updated_at_ms));
        Ok(metas)
    }

    async fn get_meta(&self, id: &SessionId) -> Result<SessionMeta, StoreError> {
        let sessions = self.lock();
        sessions
            .get(id)
            .map(|r| r.meta.clone())
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))
    }

    async fn update_meta(&self, id: &SessionId, patch: SessionMetaPatch) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        let SessionMetaPatch {
            title,
            provider_session_id,
            model,
            mode,
            cwd,
        } = patch;
        if let Some(title) = title {
            record.meta.title = Some(title);
        }
        if let Some(provider_session_id) = provider_session_id {
            record.meta.provider_session_id = Some(provider_session_id);
        }
        if let Some(model) = model {
            record.meta.model = Some(model);
        }
        if let Some(cwd) = cwd {
            record.meta.cwd = cwd;
        }
        if let Some(mode) = mode {
            record.meta.mode = Some(mode);
        }
        record.meta.updated_at_ms = now_ms();
        Ok(())
    }

    async fn delete(&self, id: &SessionId) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        sessions
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))
    }

    /// Records the compaction as an [`AgentEvent::CompactionBoundary`]
    /// appended through [`SessionStore::append`] — a single mechanism, so the
    /// boundary gets a sequence number, bumps `updated_at_ms`, and replays
    /// exactly like every other persisted event.
    async fn record_compaction(
        &self,
        id: &SessionId,
        compaction: CompactionSummary,
    ) -> Result<(), StoreError> {
        self.append(
            id,
            &[AgentEvent::CompactionBoundary {
                summary: compaction,
            }],
        )
        .await
        .map(|_| ())
    }

    async fn record_checkpoint(
        &self,
        id: &SessionId,
        checkpoint: CheckpointRef,
    ) -> Result<(), StoreError> {
        let mut sessions = self.lock();
        let record = sessions
            .get_mut(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        record.checkpoints.push(checkpoint);
        Ok(())
    }

    async fn list_checkpoints(&self, id: &SessionId) -> Result<Vec<CheckpointRef>, StoreError> {
        let sessions = self.lock();
        let record = sessions
            .get(id)
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        Ok(record.checkpoints.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentloop_contracts::{ModelRef, TurnId};
    use pretty_assertions::assert_eq;

    use super::*;

    fn meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: SessionId::from(id),
            title: None,
            agent_id: "native".to_owned(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: PathBuf::from("/workspace"),
            model: None,
            fallback_models: Vec::new(),
            mode: None,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn event(turn: &str) -> AgentEvent {
        AgentEvent::TurnStarted {
            turn_id: TurnId::from(turn),
        }
    }

    async fn store_with(id: &str) -> MemoryStore {
        let store = MemoryStore::new();
        store.create(meta(id)).await.unwrap();
        store
    }

    #[tokio::test]
    async fn append_assigns_consecutive_seqs_across_batches() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");

        let first = store
            .append(&id, &[event("t0"), event("t1")])
            .await
            .unwrap();
        assert_eq!(first, 0);
        let second = store.append(&id, &[event("t2")]).await.unwrap();
        assert_eq!(second, 2);
        let third = store
            .append(&id, &[event("t3"), event("t4")])
            .await
            .unwrap();
        assert_eq!(third, 3);

        let seqs: Vec<u64> = store
            .read(&id, 0)
            .await
            .unwrap()
            .into_iter()
            .map(|stored| stored.seq)
            .collect();
        assert_eq!(seqs, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn empty_batch_returns_next_seq_without_storing() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");

        assert_eq!(store.append(&id, &[]).await.unwrap(), 0);
        store.append(&id, &[event("t0")]).await.unwrap();
        assert_eq!(store.append(&id, &[]).await.unwrap(), 1);
        assert_eq!(store.read(&id, 0).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn read_filters_by_from_seq_in_order() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        store
            .append(&id, &[event("t0"), event("t1"), event("t2")])
            .await
            .unwrap();

        let tail: Vec<(u64, AgentEvent)> = store
            .read(&id, 1)
            .await
            .unwrap()
            .into_iter()
            .map(|stored| (stored.seq, stored.event))
            .collect();
        assert_eq!(tail, vec![(1, event("t1")), (2, event("t2"))],);
        assert!(store.read(&id, 3).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn append_stamps_real_ts_and_keeps_distinct_non_decreasing() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        store.append(&id, &[event("t0")]).await.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.append(&id, &[event("t1")]).await.unwrap();

        let events = store.read(&id, 0).await.unwrap();
        assert!(events[0].ts_ms > 0, "append stamps a real ts");
        assert!(
            events[1].ts_ms > events[0].ts_ms,
            "distinct appends keep distinct, non-decreasing ts"
        );
    }

    #[tokio::test]
    async fn create_duplicate_errors() {
        let store = store_with("s1").await;
        let err = store.create(meta("s1")).await.unwrap_err();
        assert!(matches!(err, StoreError::SessionExists(id) if id == SessionId::from("s1")));
    }

    #[tokio::test]
    async fn unknown_session_errors() {
        let store = MemoryStore::new();
        let id = SessionId::from("missing");

        assert!(matches!(
            store.append(&id, &[event("t0")]).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(matches!(
            store.read(&id, 0).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(matches!(
            store.get_meta(&id).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(matches!(
            store
                .update_meta(&id, SessionMetaPatch::default())
                .await
                .unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(matches!(
            store.delete(&id).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(matches!(
            store
                .record_compaction(
                    &id,
                    CompactionSummary {
                        summary_markdown: "s".to_owned(),
                        strategy: "truncate".to_owned(),
                        tokens_before: None,
                        tokens_after: None,
                    },
                )
                .await
                .unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn list_sorts_by_updated_at_descending() {
        let store = MemoryStore::new();
        let mut old = meta("old");
        old.updated_at_ms = 100;
        let mut newer = meta("newer");
        newer.updated_at_ms = 300;
        let mut middle = meta("middle");
        middle.updated_at_ms = 200;
        store.create(old).await.unwrap();
        store.create(newer).await.unwrap();
        store.create(middle).await.unwrap();

        let ids: Vec<String> = store
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.id.as_str().to_owned())
            .collect();
        assert_eq!(ids, vec!["newer", "middle", "old"]);
    }

    #[tokio::test]
    async fn update_meta_applies_only_some_fields() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        store
            .update_meta(
                &id,
                SessionMetaPatch {
                    title: Some("hello".to_owned()),
                    provider_session_id: None,
                    model: Some(ModelRef::from("anthropic/model-x")),
                    mode: None,
                    cwd: None,
                },
            )
            .await
            .unwrap();

        let updated = store.get_meta(&id).await.unwrap();
        assert_eq!(updated.title.as_deref(), Some("hello"));
        assert_eq!(updated.model, Some(ModelRef::from("anthropic/model-x")));
        assert_eq!(updated.provider_session_id, None);
        assert_eq!(updated.mode, None);
        assert!(
            updated.updated_at_ms > 1,
            "update_meta must bump updated_at_ms"
        );

        store
            .update_meta(&id, SessionMetaPatch::default())
            .await
            .unwrap();
        let unchanged = store.get_meta(&id).await.unwrap();
        assert_eq!(unchanged.title.as_deref(), Some("hello"));
        assert_eq!(unchanged.model, Some(ModelRef::from("anthropic/model-x")));
    }

    #[tokio::test]
    async fn record_compaction_appends_boundary_event() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        store.append(&id, &[event("t0")]).await.unwrap();

        let summary = CompactionSummary {
            summary_markdown: "condensed history".to_owned(),
            strategy: "summarize_oldest".to_owned(),
            tokens_before: Some(1000),
            tokens_after: Some(100),
        };
        store.record_compaction(&id, summary.clone()).await.unwrap();

        let events: Vec<(u64, AgentEvent)> = store
            .read(&id, 1)
            .await
            .unwrap()
            .into_iter()
            .map(|stored| (stored.seq, stored.event))
            .collect();
        assert_eq!(
            events,
            vec![(1, AgentEvent::CompactionBoundary { summary })],
        );
        assert_eq!(store.append(&id, &[]).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn append_bumps_updated_at() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        assert_eq!(store.get_meta(&id).await.unwrap().updated_at_ms, 1);

        store.append(&id, &[event("t0")]).await.unwrap();
        let bumped = store.get_meta(&id).await.unwrap().updated_at_ms;
        assert!(bumped > 1, "append must bump updated_at_ms");
    }

    #[tokio::test]
    async fn satisfies_the_session_store_conformance_suite() {
        agentloop_testkit::assert_store_conformance(MemoryStore::new())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_removes_session() {
        let store = store_with("s1").await;
        let id = SessionId::from("s1");
        store.delete(&id).await.unwrap();
        assert!(matches!(
            store.get_meta(&id).await.unwrap_err(),
            StoreError::SessionNotFound(_)
        ));
        assert!(store.list().await.unwrap().is_empty());
    }
}
