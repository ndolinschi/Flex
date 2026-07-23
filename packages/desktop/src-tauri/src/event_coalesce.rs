//! Coalesce high-frequency session streaming deltas before emitting to the FE.
//!
//! Merges consecutive `MarkdownDelta` / `ThinkingDelta` / `ToolArgsDelta` /
//! `ExecChunk` events that share the same identity keys, keeping the **max**
//! `seq` so FE gap detection (`useSessionEvents` lastSeq) stays correct.

use agentloop_contracts::{AgentEvent, SessionEvent};

/// Flush pending when accumulated text/json reaches this many bytes.
pub const MAX_PENDING_BYTES: usize = 4_096;

/// Default flush window for coalesced deltas (≈ one frame at 60 Hz).
pub const FLUSH_INTERVAL_MS: u64 = 16;

/// Holds at most one pending coalescable [`SessionEvent`].
#[derive(Debug, Default)]
pub struct EventCoalescer {
    pending: Option<SessionEvent>,
}

impl EventCoalescer {
    pub fn new() -> Self {
        Self { pending: None }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Ingest one event.
    ///
    /// Returns zero, one, or two events ready to emit immediately:
    /// - empty when the event was absorbed into `pending`
    /// - flushed previous + current when the new event cannot merge
    /// - a size-forced flush of the (possibly just-merged) pending buffer
    pub fn push(&mut self, event: SessionEvent) -> Vec<SessionEvent> {
        if let Some(prev) = self.pending.take() {
            if can_merge(&prev, &event) {
                let merged = merge(prev, event);
                if coalescable_bytes(&merged) >= MAX_PENDING_BYTES {
                    return vec![merged];
                }
                self.pending = Some(merged);
                return Vec::new();
            }
            // Cannot merge: emit previous, then handle the new event.
            let mut out = vec![prev];
            if is_coalescable(&event.payload) {
                self.pending = Some(event);
            } else {
                out.push(event);
            }
            return out;
        }

        if is_coalescable(&event.payload) {
            // Single chunk already oversized — emit immediately.
            if coalescable_bytes(&event) >= MAX_PENDING_BYTES {
                return vec![event];
            }
            self.pending = Some(event);
            Vec::new()
        } else {
            vec![event]
        }
    }

    /// Take any buffered event (stream end / timer flush).
    pub fn flush(&mut self) -> Option<SessionEvent> {
        self.pending.take()
    }
}

fn is_coalescable(payload: &AgentEvent) -> bool {
    matches!(
        payload,
        AgentEvent::MarkdownDelta { .. }
            | AgentEvent::ThinkingDelta { .. }
            | AgentEvent::ToolArgsDelta { .. }
            | AgentEvent::ExecChunk { .. }
    )
}

fn can_merge(a: &SessionEvent, b: &SessionEvent) -> bool {
    if a.session_id != b.session_id {
        return false;
    }
    match (&a.payload, &b.payload) {
        (
            AgentEvent::MarkdownDelta {
                message_id: m1, ..
            },
            AgentEvent::MarkdownDelta {
                message_id: m2, ..
            },
        ) => m1 == m2,
        (
            AgentEvent::ThinkingDelta {
                message_id: m1, ..
            },
            AgentEvent::ThinkingDelta {
                message_id: m2, ..
            },
        ) => m1 == m2,
        (
            AgentEvent::ToolArgsDelta { call_id: c1, .. },
            AgentEvent::ToolArgsDelta { call_id: c2, .. },
        ) => c1 == c2,
        (
            AgentEvent::ExecChunk {
                call_id: c1,
                stream: s1,
                ..
            },
            AgentEvent::ExecChunk {
                call_id: c2,
                stream: s2,
                ..
            },
        ) => c1 == c2 && s1 == s2,
        _ => false,
    }
}

fn merge(mut a: SessionEvent, b: SessionEvent) -> SessionEvent {
    // Highest seq so FE lastSeq advances past every merged delta.
    a.seq = a.seq.max(b.seq);
    a.ts_ms = a.ts_ms.max(b.ts_ms);
    if b.turn_id.is_some() {
        a.turn_id = b.turn_id;
    }
    match (&mut a.payload, b.payload) {
        (
            AgentEvent::MarkdownDelta { text, .. },
            AgentEvent::MarkdownDelta { text: t2, .. },
        ) => text.push_str(&t2),
        (
            AgentEvent::ThinkingDelta { text, .. },
            AgentEvent::ThinkingDelta { text: t2, .. },
        ) => text.push_str(&t2),
        (
            AgentEvent::ToolArgsDelta { json_fragment, .. },
            AgentEvent::ToolArgsDelta {
                json_fragment: t2, ..
            },
        ) => json_fragment.push_str(&t2),
        (AgentEvent::ExecChunk { text, .. }, AgentEvent::ExecChunk { text: t2, .. }) => {
            text.push_str(&t2)
        }
        _ => {
            // can_merge already verified shape; leave payload as `a`.
        }
    }
    a
}

fn coalescable_bytes(event: &SessionEvent) -> usize {
    match &event.payload {
        AgentEvent::MarkdownDelta { text, .. } | AgentEvent::ThinkingDelta { text, .. } => {
            text.len()
        }
        AgentEvent::ToolArgsDelta { json_fragment, .. } => json_fragment.len(),
        AgentEvent::ExecChunk { text, .. } => text.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{
        AgentEvent, ExecStream, MessageId, SessionEvent, SessionId, ToolCallId,
    };

    fn session() -> SessionId {
        SessionId::from("s1".to_string())
    }

    fn markdown(seq: u64, mid: &str, text: &str) -> SessionEvent {
        SessionEvent {
            session_id: session(),
            seq,
            turn_id: None,
            ts_ms: seq * 10,
            payload: AgentEvent::MarkdownDelta {
                message_id: MessageId::from(mid.to_string()),
                text: text.to_string(),
            },
        }
    }

    fn turn_started(seq: u64) -> SessionEvent {
        SessionEvent {
            session_id: session(),
            seq,
            turn_id: None,
            ts_ms: seq * 10,
            payload: AgentEvent::TurnStarted {
                turn_id: agentloop_contracts::TurnId::from("t1".to_string()),
            },
        }
    }

    #[test]
    fn merges_same_message_markdown_deltas() {
        let mut c = EventCoalescer::new();
        assert!(c.push(markdown(1, "m1", "hel")).is_empty());
        assert!(c.push(markdown(2, "m1", "lo")).is_empty());
        let flushed = c.flush().expect("pending");
        assert_eq!(flushed.seq, 2);
        assert_eq!(flushed.ts_ms, 20);
        match flushed.payload {
            AgentEvent::MarkdownDelta { text, .. } => assert_eq!(text, "hello"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn flushes_before_non_coalescable() {
        let mut c = EventCoalescer::new();
        assert!(c.push(markdown(1, "m1", "a")).is_empty());
        let ready = c.push(turn_started(2));
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].seq, 1);
        assert_eq!(ready[1].seq, 2);
        assert!(!c.has_pending());
    }

    #[test]
    fn does_not_merge_different_message_ids() {
        let mut c = EventCoalescer::new();
        assert!(c.push(markdown(1, "m1", "a")).is_empty());
        let ready = c.push(markdown(2, "m2", "b"));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].seq, 1);
        assert!(c.has_pending());
        let flushed = c.flush().unwrap();
        assert_eq!(flushed.seq, 2);
    }

    #[test]
    fn merges_exec_chunk_same_stream_only() {
        let mut c = EventCoalescer::new();
        let a = SessionEvent {
            session_id: session(),
            seq: 1,
            turn_id: None,
            ts_ms: 1,
            payload: AgentEvent::ExecChunk {
                call_id: ToolCallId::from("c1".to_string()),
                stream: ExecStream::Stdout,
                text: "out1".into(),
            },
        };
        let b = SessionEvent {
            session_id: session(),
            seq: 2,
            turn_id: None,
            ts_ms: 2,
            payload: AgentEvent::ExecChunk {
                call_id: ToolCallId::from("c1".to_string()),
                stream: ExecStream::Stderr,
                text: "err".into(),
            },
        };
        assert!(c.push(a).is_empty());
        let ready = c.push(b);
        assert_eq!(ready.len(), 1);
        assert!(matches!(
            ready[0].payload,
            AgentEvent::ExecChunk {
                stream: ExecStream::Stdout,
                ..
            }
        ));
    }
}
