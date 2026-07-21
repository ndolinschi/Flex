//! Single-writer front for one session's persisted append + broadcast.
//!
//! `SessionHandle::emit_persistent` used to call `store.append().await` then
//! `broadcast.send()` directly. Two concurrent callers on the same session
//! (e.g. a subagent relay racing the parent's own turn) could have their
//! `append`s serialized correctly by the store's internal lock, yet their
//! *broadcasts* land in the opposite order — a live subscriber could observe
//! seq 6 before seq 5 even though the log itself is correctly ordered. This
//! actor closes that window: every append for a session goes through one
//! mailbox, processed strictly one at a time, so append-then-broadcast is
//! atomic relative to every other append on the same session.
//!
//! Subscribing is unaffected and untouched — `broadcast::Sender::subscribe()`
//! may still be called directly at any time; a receiver only misses messages
//! sent before it existed, which `next_seq`-stamped ephemeral events and the
//! session log (always readable from `seq` onward) already cover.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use agentloop_contracts::{
    AgentEvent, CheckpointLabel, CheckpointRef, SessionEvent, SessionId, TurnId, now_ms,
};
use agentloop_core::{SessionStore, StoreError};

struct AppendJob {
    turn_id: Option<TurnId>,
    payload: AgentEvent,
    reply: oneshot::Sender<Result<u64, StoreError>>,
}

/// Handle to a session's single-writer append task. Cheap to clone; cloning
/// shares the same mailbox.
#[derive(Clone)]
pub(crate) struct SessionActorHandle {
    tx: mpsc::Sender<AppendJob>,
}

impl SessionActorHandle {
    /// Append one event and broadcast it, as a single ordered step relative
    /// to every other call on this handle. Returns the assigned seq.
    pub(crate) async fn append(
        &self,
        turn_id: Option<TurnId>,
        payload: AgentEvent,
    ) -> Result<u64, StoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppendJob {
                turn_id,
                payload,
                reply,
            })
            .await
            .map_err(|_| StoreError::Io("session actor task is gone".to_owned()))?;
        rx.await
            .map_err(|_| StoreError::Io("session actor dropped the reply".to_owned()))?
    }
}

/// Spawn the single-writer task owning `id`'s append+broadcast(+checkpoint)
/// sequencing. `broadcast_tx` is the same sender `SessionHandle` hands out
/// for `.subscribe()` — this task is the only writer, but not the only
/// reader/subscriber path.
pub(crate) fn spawn_session_actor(
    id: SessionId,
    store: Arc<dyn SessionStore>,
    agent_id: String,
    broadcast_tx: tokio::sync::broadcast::Sender<SessionEvent>,
) -> SessionActorHandle {
    let (tx, mut rx) = mpsc::channel::<AppendJob>(256);
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let result = store.append(&id, std::slice::from_ref(&job.payload)).await;
            if let Ok(seq) = result {
                agentloop_core::observe::record_event_metrics(&agent_id, &job.payload);
                let _ = broadcast_tx.send(SessionEvent {
                    session_id: id.clone(),
                    seq,
                    turn_id: job.turn_id.clone(),
                    ts_ms: now_ms(),
                    payload: job.payload.clone(),
                });
                if let Some(label) = checkpoint_label(&job.payload) {
                    let _ = store
                        .record_checkpoint(
                            &id,
                            CheckpointRef {
                                session_id: id.clone(),
                                seq,
                                turn_id: job.turn_id.clone(),
                                ts_ms: now_ms(),
                                label,
                            },
                        )
                        .await;
                }
            }
            let _ = job.reply.send(result);
        }
    });
    SessionActorHandle { tx }
}

fn checkpoint_label(event: &AgentEvent) -> Option<CheckpointLabel> {
    match event {
        AgentEvent::TurnCompleted { .. } => Some(CheckpointLabel::TurnCompleted),
        AgentEvent::CompactionBoundary { .. } => Some(CheckpointLabel::Compaction),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentloop_contracts::{SessionMeta, TurnId};
    use agentloop_session::MemoryStore;

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
            reuse_workspace_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn turn_event(turn: &str) -> AgentEvent {
        AgentEvent::TurnStarted {
            turn_id: TurnId::from(turn),
        }
    }

    /// The regression test for the fixed race: many concurrent callers append
    /// through the same actor handle; every live subscriber must observe
    /// seqs in strictly increasing order with no gaps or duplicates, matching
    /// the persisted log exactly.
    #[tokio::test]
    async fn concurrent_appends_broadcast_in_seq_order() {
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let id = SessionId::from("actor-race");
        store.create(meta(id.as_str())).await.unwrap();
        let (broadcast_tx, mut rx) = tokio::sync::broadcast::channel(1024);
        let actor =
            spawn_session_actor(id.clone(), store.clone(), "native".to_owned(), broadcast_tx);

        const WRITERS: usize = 16;
        let mut handles = Vec::with_capacity(WRITERS);
        for writer in 0..WRITERS {
            let actor = actor.clone();
            handles.push(tokio::spawn(async move {
                actor
                    .append(None, turn_event(&format!("w{writer}")))
                    .await
                    .unwrap()
            }));
        }
        let mut appended_seqs = Vec::with_capacity(WRITERS);
        for handle in handles {
            appended_seqs.push(handle.await.unwrap());
        }
        appended_seqs.sort_unstable();
        assert_eq!(
            appended_seqs,
            (0..WRITERS as u64).collect::<Vec<_>>(),
            "every append gets a distinct, gapless seq"
        );

        let mut observed = Vec::with_capacity(WRITERS);
        for _ in 0..WRITERS {
            observed.push(rx.recv().await.unwrap().seq);
        }
        assert_eq!(
            observed,
            (0..WRITERS as u64).collect::<Vec<_>>(),
            "the live broadcast stream observes seqs in strictly increasing order, \
             matching the persisted log — no inversion between concurrent callers"
        );
    }

    #[tokio::test]
    async fn turn_completed_records_a_checkpoint() {
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let id = SessionId::from("actor-checkpoint");
        store.create(meta(id.as_str())).await.unwrap();
        let (broadcast_tx, _rx) = tokio::sync::broadcast::channel(16);
        let actor =
            spawn_session_actor(id.clone(), store.clone(), "native".to_owned(), broadcast_tx);

        let turn_id = TurnId::from("t0");
        let seq = actor
            .append(
                Some(turn_id.clone()),
                AgentEvent::TurnCompleted {
                    turn_id: turn_id.clone(),
                    summary: agentloop_contracts::TurnSummary {
                        turn_id: turn_id.clone(),
                        stop_reason: agentloop_contracts::TurnStopReason::EndTurn,
                        usage: agentloop_contracts::TokenUsage::default(),
                        cost_usd: None,
                        num_model_calls: 1,
                        num_tool_calls: 0,
                        duration_ms: 1,
                    },
                },
            )
            .await
            .unwrap();

        let checkpoints = store.list_checkpoints(&id).await.unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].seq, seq);
        assert!(matches!(
            checkpoints[0].label,
            CheckpointLabel::TurnCompleted
        ));
    }
}
