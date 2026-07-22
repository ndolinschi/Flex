//! In-flight routing-override registry.
//!
//! One [`RoutingTable`] (behind an `Arc`) is shared between `SetRoutingTool`
//! (in `agentloop-tools`) and the loop's `TurnDeps` so the tool can write an
//! override that the loop reads at the start of the next iteration — without
//! either crate depending on the other.

use std::collections::HashMap;
use std::sync::Mutex;

use agentloop_contracts::{Effort, ModelRef, SessionId};

/// One session's in-flight routing override, written by `SetRouting` and
/// consumed (once) by the loop at the start of the next model iteration.
#[derive(Debug, Clone)]
pub struct RoutingOverride {
    pub model: Option<ModelRef>,
    pub effort: Option<Effort>,
}

/// Shared, session-keyed routing-override table.
///
/// Held by both the `SetRoutingTool` and the loop's [`TurnDeps`] via a single
/// `Arc<RoutingTable>` so the tool's write is immediately visible to the next
/// loop iteration without a callback or channel.
#[derive(Debug, Default)]
pub struct RoutingTable {
    inner: Mutex<HashMap<SessionId, RoutingOverride>>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrite the override for `session`.
    pub fn set(&self, session: &SessionId, ov: RoutingOverride) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.insert(session.clone(), ov);
    }

    /// Return a copy of the current override for `session`, if any.
    pub fn get(&self, session: &SessionId) -> Option<RoutingOverride> {
        let guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.get(session).cloned()
    }

    /// Remove any override for `session` (called at turn start so each turn
    /// begins fresh in Auto mode).
    pub fn clear(&self, session: &SessionId) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(session);
    }
}
