use tokio::sync::mpsc;

use agentloop_contracts::AgentEvent;

#[derive(Clone)]
pub struct EventSink {
    tx: mpsc::UnboundedSender<AgentEvent>,
}

impl EventSink {
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<AgentEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn emit(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    pub fn disconnected() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel();
        Self { tx }
    }
}
