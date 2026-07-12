//! `ToolCallManager`: owns every tool call of a turn, enforces the status
//! machine, stamps timing, and yields the updated record for each transition
//! so the loop can emit `ToolCallUpdated` events.

use std::collections::HashMap;

use agentloop_contracts::{
    MessageId, SessionId, ToolCall, ToolCallId, ToolCallOrigin, ToolCallStatus, ToolCallTiming,
    ToolOutput, TurnId, now_ms,
};

/// Attempted an illegal status transition (a loop bug, not a user error).
#[derive(Debug, thiserror::Error)]
#[error("illegal tool-call transition for {call_id}: {from:?} -> {to:?}")]
pub struct InvalidTransition {
    pub call_id: ToolCallId,
    pub from: ToolCallStatus,
    pub to: ToolCallStatus,
}

/// In-flight tool calls of one turn.
#[derive(Default)]
pub struct ToolCallManager {
    calls: HashMap<ToolCallId, ToolCall>,
}

impl ToolCallManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit a parsed call as `Pending`. Returns the record to emit.
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        &mut self,
        id: ToolCallId,
        session_id: SessionId,
        turn_id: TurnId,
        message_id: MessageId,
        tool_name: String,
        input: serde_json::Value,
        read_only: bool,
        origin: ToolCallOrigin,
    ) -> ToolCall {
        let call = ToolCall {
            id: id.clone(),
            session_id,
            turn_id,
            message_id,
            tool_name,
            input,
            read_only,
            origin,
            status: ToolCallStatus::Pending,
            timing: ToolCallTiming {
                queued_at_ms: now_ms(),
                ..ToolCallTiming::default()
            },
            result: None,
        };
        self.calls.insert(id, call.clone());
        call
    }

    /// Transition a call, stamping timing. Returns the updated record.
    pub fn transition(
        &mut self,
        id: &ToolCallId,
        to: ToolCallStatus,
        result: Option<ToolOutput>,
    ) -> Result<ToolCall, InvalidTransition> {
        let Some(call) = self.calls.get_mut(id) else {
            return Err(InvalidTransition {
                call_id: id.clone(),
                from: ToolCallStatus::Pending,
                to,
            });
        };
        if !call.status.can_transition_to(&to) {
            return Err(InvalidTransition {
                call_id: id.clone(),
                from: call.status.clone(),
                to,
            });
        }

        let now = now_ms();
        if matches!(call.status, ToolCallStatus::AwaitingPermission { .. }) {
            let waited_since = call
                .timing
                .started_at_ms
                .unwrap_or(call.timing.queued_at_ms);
            call.timing.permission_wait_ms = Some(now.saturating_sub(waited_since));
        }
        match &to {
            ToolCallStatus::Running => call.timing.started_at_ms = Some(now),
            status if status.is_terminal() => call.timing.finished_at_ms = Some(now),
            _ => {}
        }
        if result.is_some() {
            call.result = result;
        }
        call.status = to;
        Ok(call.clone())
    }

    /// Cancel every non-terminal call; returns the updated records to emit.
    pub fn cancel_in_flight(&mut self) -> Vec<ToolCall> {
        let ids: Vec<ToolCallId> = self
            .calls
            .iter()
            .filter(|(_, call)| !call.status.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| self.transition(&id, ToolCallStatus::Cancelled, None).ok())
            .collect()
    }

    pub fn get(&self, id: &ToolCallId) -> Option<&ToolCall> {
        self.calls.get(id)
    }

    pub fn get_mut(&mut self, id: &ToolCallId) -> Option<&mut ToolCall> {
        self.calls.get_mut(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admit(manager: &mut ToolCallManager) -> ToolCall {
        manager.admit(
            ToolCallId::from("c1"),
            SessionId::from("s1"),
            TurnId::from("t1"),
            MessageId::from("m1"),
            "echo".to_owned(),
            serde_json::json!({}),
            true,
            ToolCallOrigin::Model,
        )
    }

    #[test]
    fn lifecycle_stamps_timing() {
        let mut manager = ToolCallManager::new();
        let call = admit(&mut manager);
        assert_eq!(call.status, ToolCallStatus::Pending);
        assert!(call.timing.queued_at_ms > 0);

        let running = manager
            .transition(&call.id, ToolCallStatus::Running, None)
            .expect("pending -> running");
        assert!(running.timing.started_at_ms.is_some());

        let done = manager
            .transition(
                &call.id,
                ToolCallStatus::Completed,
                Some(ToolOutput::text("ok")),
            )
            .expect("running -> completed");
        assert!(done.timing.finished_at_ms.is_some());
        assert!(done.result.is_some());
    }

    #[test]
    fn illegal_transition_is_rejected() {
        let mut manager = ToolCallManager::new();
        let call = admit(&mut manager);
        let err = manager
            .transition(&call.id, ToolCallStatus::Completed, None)
            .expect_err("pending -> completed is illegal");
        assert!(err.to_string().contains("illegal"));
    }

    #[test]
    fn cancel_in_flight_skips_terminal() {
        let mut manager = ToolCallManager::new();
        let call = admit(&mut manager);
        manager
            .transition(&call.id, ToolCallStatus::Running, None)
            .expect("running");
        manager
            .transition(
                &call.id,
                ToolCallStatus::Completed,
                Some(ToolOutput::text("x")),
            )
            .expect("completed");

        let other = manager.admit(
            ToolCallId::from("c2"),
            SessionId::from("s1"),
            TurnId::from("t1"),
            MessageId::from("m1"),
            "slow".to_owned(),
            serde_json::json!({}),
            false,
            ToolCallOrigin::Model,
        );

        let cancelled = manager.cancel_in_flight();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].id, other.id);
        assert_eq!(cancelled[0].status, ToolCallStatus::Cancelled);
    }
}
