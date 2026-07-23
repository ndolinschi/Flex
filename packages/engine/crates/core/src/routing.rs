use std::collections::HashMap;
use std::sync::Mutex;

use agentloop_contracts::{Effort, ModelRef, SessionId};

#[derive(Debug, Clone)]
pub struct RoutingOverride {
    pub model: Option<ModelRef>,
    pub effort: Option<Effort>,
}

#[derive(Debug, Default)]
pub struct RoutingTable {
    inner: Mutex<HashMap<SessionId, RoutingOverride>>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, session: &SessionId, ov: RoutingOverride) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.insert(session.clone(), ov);
    }

    pub fn get(&self, session: &SessionId) -> Option<RoutingOverride> {
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.get(session).cloned()
    }

    pub fn clear(&self, session: &SessionId) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(session);
    }
}
