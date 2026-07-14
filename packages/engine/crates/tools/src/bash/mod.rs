//! `Bash`: run a shell command in the session cwd through the composed
//! execution backend.

mod background;
mod chunk_sink;
mod input;
mod run;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use agentloop_core::{BackgroundProcessRegistry, DemoteRegistry, Executor, NetworkPolicy};

pub(super) const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub(super) const MAX_TIMEOUT_MS: u64 = 600_000;
pub(super) const MAX_OUTPUT_CHARS: usize = 120_000;

/// Execute a shell command with `sh -lc` semantics through the injected
/// [`Executor`] backend (local process by default; container/remote backends
/// at composition time). Also fields `background_action`/`process_id` for
/// checking on or stopping a process a prior `run_in_background: true` call
/// started — kept on the same tool (rather than a separate one) since both
/// paths share the executor, the process id namespace, and the model's
/// mental model of "the shell tool."
pub struct BashTool {
    pub(crate) executor: Arc<dyn Executor>,
    pub(crate) network: NetworkPolicy,
    pub(crate) background: Arc<BackgroundProcessRegistry>,
    /// Per-call demote signals for still-running **foreground** calls (see
    /// `MOVE-TO-BACKGROUND`): registered for the duration of one blocking
    /// `exec_demotable` call and unregistered the instant it returns, either
    /// way. Shared with the composition root the same way `background` is,
    /// so a `background_demote` Tauri command (via `EngineService`) can reach
    /// it without holding a second `BashTool`.
    pub(crate) demote: Arc<DemoteRegistry>,
}

impl BashTool {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self {
            executor,
            network: NetworkPolicy::Allowed,
            background: Arc::new(BackgroundProcessRegistry::new()),
            demote: Arc::new(DemoteRegistry::new()),
        }
    }

    /// Set the network posture every command runs under.
    pub fn with_network(mut self, network: NetworkPolicy) -> Self {
        self.network = network;
        self
    }

    /// Share a background-process registry rather than owning a private one
    /// — lets the composition root (session teardown, engine drop) reach the
    /// same table this tool registers into.
    pub fn with_background_registry(mut self, registry: Arc<BackgroundProcessRegistry>) -> Self {
        self.background = registry;
        self
    }

    /// Share a demote registry rather than owning a private one — lets the
    /// composition root's `background_demote` reach the same table this
    /// tool's foreground exec path registers into.
    pub fn with_demote_registry(mut self, registry: Arc<DemoteRegistry>) -> Self {
        self.demote = registry;
        self
    }

    /// The shared registry, for composition roots that need to kill sessions'
    /// background processes on teardown without holding a second `BashTool`.
    pub fn background_registry(&self) -> Arc<BackgroundProcessRegistry> {
        self.background.clone()
    }

    /// The shared demote registry, for composition roots wiring up
    /// `background_demote` without holding a second `BashTool`.
    pub fn demote_registry(&self) -> Arc<DemoteRegistry> {
        self.demote.clone()
    }
}
