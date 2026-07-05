//! `EventSink`: how work-in-progress (tools, stream pumps, subagent relays)
//! emits events into a session's pipeline.
//!
//! The sink is deliberately dumb: it forwards payloads to the session's owner
//! (the agent implementation), which appends persistent events to the store
//! *before* broadcasting and stamps the envelope (seq/ts). Dropping the
//! receiver simply makes emission a no-op — a tool racing turn teardown must
//! not error out because nobody is listening anymore.

use tokio::sync::mpsc;

use agentloop_contracts::AgentEvent;

/// A cloneable handle for emitting events into a session's pipeline.
#[derive(Clone)]
pub struct EventSink {
    tx: mpsc::UnboundedSender<AgentEvent>,
}

impl EventSink {
    /// Create a sink and the receiving half the session owner drains.
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<AgentEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Emit an event. Infallible by design: if the session pipeline is gone,
    /// the event is dropped.
    pub fn emit(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    /// A sink wired to nothing — for tests and detached contexts.
    pub fn disconnected() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel();
        Self { tx }
    }
}
