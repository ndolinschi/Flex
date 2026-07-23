use agentloop_contracts::{AgentEvent, SessionEvent, SessionId, Transcript, TurnId, reduce};
use agentloop_core::StoredEvent;

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    pub async fn replay(
        &self,
        session: &SessionId,
        from_seq: u64,
    ) -> EngineResult<Vec<SessionEvent>> {
        let events = self.store.read(session, 0).await?;
        let mut current_turn: Option<TurnId> = None;
        let mut replay = Vec::new();
        for StoredEvent { seq, ts_ms, event } in events {
            if let AgentEvent::TurnStarted { turn_id } = &event {
                current_turn = Some(turn_id.clone());
            }
            if seq >= from_seq {
                replay.push(SessionEvent {
                    session_id: session.clone(),
                    seq,
                    turn_id: current_turn.clone(),
                    ts_ms,
                    payload: event.clone(),
                });
            }
            if matches!(event, AgentEvent::TurnCompleted { .. }) {
                current_turn = None;
            }
        }
        Ok(replay)
    }

    pub async fn session_items(&self, session: &SessionId) -> EngineResult<Transcript> {
        let events = self.store.read(session, 0).await?;
        let payloads = events
            .iter()
            .map(|stored| &stored.event)
            .collect::<Vec<_>>();
        Ok(reduce(payloads))
    }
}
