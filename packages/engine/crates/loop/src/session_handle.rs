use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, Effort, PermissionMode, SessionEvent, SessionId, TurnId, now_ms,
};
use agentloop_core::{SessionStore, StoreError};

use crate::actor::{SessionActorHandle, spawn_session_actor};

pub(crate) struct SessionHandle {
    pub(crate) id: SessionId,
    actor: SessionActorHandle,
    pub(crate) broadcast: broadcast::Sender<SessionEvent>,
    next_seq: AtomicU64,
    pub(crate) turn_gate: tokio::sync::Mutex<()>,
    pub(crate) current_cancel: Mutex<Option<CancellationToken>>,
    turn_permission_mode: Mutex<Option<PermissionMode>>,
    turn_disable_tools: Mutex<bool>,
    turn_effort: Mutex<Option<Effort>>,
}

impl SessionHandle {
    pub(crate) fn new(
        id: SessionId,
        agent_id: String,
        store: Arc<dyn SessionStore>,
        next_seq: u64,
    ) -> Self {
        let (broadcast, _) = broadcast::channel(1024);
        let actor = spawn_session_actor(id.clone(), store, agent_id, broadcast.clone());
        Self {
            id,
            actor,
            broadcast,
            next_seq: AtomicU64::new(next_seq),
            turn_gate: tokio::sync::Mutex::new(()),
            current_cancel: Mutex::new(None),
            turn_permission_mode: Mutex::new(None),
            turn_disable_tools: Mutex::new(false),
            turn_effort: Mutex::new(None),
        }
    }

    pub(crate) fn request_cancel(&self) {
        if let Some(token) = self
            .current_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            token.cancel();
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

    pub(crate) fn set_turn_disable_tools(&self, disable: bool) {
        *self
            .turn_disable_tools
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = disable;
    }

    pub(crate) fn turn_disable_tools(&self) -> bool {
        *self
            .turn_disable_tools
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn set_turn_effort(&self, effort: Option<Effort>) {
        *self.turn_effort.lock().unwrap_or_else(|p| p.into_inner()) = effort;
    }

    pub(crate) fn turn_effort(&self) -> Option<Effort> {
        *self.turn_effort.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) async fn emit_persistent(
        &self,
        turn_id: Option<&TurnId>,
        payload: AgentEvent,
    ) -> Result<u64, StoreError> {
        let seq = self.actor.append(turn_id.cloned(), payload).await?;
        self.next_seq.store(seq + 1, Ordering::Relaxed);
        Ok(seq)
    }

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
