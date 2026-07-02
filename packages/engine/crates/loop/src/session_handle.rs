//! Live per-session state for the native loop.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{AgentEvent, PermissionMode, SessionEvent, SessionId, TurnId, now_ms};
use agentloop_core::{SessionStore, StoreError};

/// Per-session live state: the broadcast bus, the turn gate, and the current
/// turn's cancellation token. The handle owns everything event emission needs
/// so background tasks (sink drains) can emit without borrowing the agent.
pub(crate) struct SessionHandle {
    pub(crate) id: SessionId,
    agent_id: String,
    store: Arc<dyn SessionStore>,
    pub(crate) broadcast: broadcast::Sender<SessionEvent>,
    /// Next persisted seq (mirror of the store's counter, for stamping
    /// ephemeral events between persisted ones).
    next_seq: AtomicU64,
    /// One turn at a time per session.
    pub(crate) turn_gate: tokio::sync::Mutex<()>,
    pub(crate) current_cancel: Mutex<Option<CancellationToken>>,
    /// Live permission mode for the in-flight turn; updated when the client
    /// changes `/permissions` mid-turn.
    turn_permission_mode: Mutex<Option<PermissionMode>>,
}

impl SessionHandle {
    pub(crate) fn new(
        id: SessionId,
        agent_id: String,
        store: Arc<dyn SessionStore>,
        next_seq: u64,
    ) -> Self {
        let (broadcast, _) = broadcast::channel(1024);
        Self {
            id,
            agent_id,
            store,
            broadcast,
            next_seq: AtomicU64::new(next_seq),
            turn_gate: tokio::sync::Mutex::new(()),
            current_cancel: Mutex::new(None),
            turn_permission_mode: Mutex::new(None),
        }
    }

    pub(crate) fn set_turn_permission_mode(&self, mode: Option<PermissionMode>) {
        *self
            .turn_permission_mode
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = mode;
    }

    pub(crate) fn turn_permission_mode(&self) -> Option<PermissionMode> {
        *self
            .turn_permission_mode
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// Append to the store (assigning seq), record metrics, then broadcast.
    /// Persistence happens *before* broadcast: subscribers can always re-sync
    /// from the store.
    pub(crate) async fn emit_persistent(
        &self,
        turn_id: Option<&TurnId>,
        payload: AgentEvent,
    ) -> Result<u64, StoreError> {
        let seq = self
            .store
            .append(&self.id, std::slice::from_ref(&payload))
            .await?;
        self.next_seq.store(seq + 1, Ordering::Relaxed);
        agentloop_core::observe::record_event_metrics(&self.agent_id, &payload);
        let _ = self.broadcast.send(SessionEvent {
            session_id: self.id.clone(),
            seq,
            turn_id: turn_id.cloned(),
            ts_ms: now_ms(),
            payload,
        });
        Ok(seq)
    }

    /// Broadcast without persisting (streaming deltas). Stamped with the seq
    /// the *next* persisted event will get, so ordering against the log is
    /// unambiguous.
    pub(crate) fn emit_ephemeral(&self, turn_id: Option<&TurnId>, payload: AgentEvent) {
        let _ = self.broadcast.send(SessionEvent {
            session_id: self.id.clone(),
            seq: self.next_seq.load(Ordering::Relaxed),
            turn_id: turn_id.cloned(),
            ts_ms: now_ms(),
            payload,
        });
    }
}
