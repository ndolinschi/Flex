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

pub struct BashTool {
    pub(crate) executor: Arc<dyn Executor>,
    pub(crate) network: NetworkPolicy,
    pub(crate) background: Arc<BackgroundProcessRegistry>,
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

    pub fn with_network(mut self, network: NetworkPolicy) -> Self {
        self.network = network;
        self
    }

    pub fn with_background_registry(mut self, registry: Arc<BackgroundProcessRegistry>) -> Self {
        self.background = registry;
        self
    }

    pub fn with_demote_registry(mut self, registry: Arc<DemoteRegistry>) -> Self {
        self.demote = registry;
        self
    }

    pub fn background_registry(&self) -> Arc<BackgroundProcessRegistry> {
        self.background.clone()
    }

    pub fn demote_registry(&self) -> Arc<DemoteRegistry> {
        self.demote.clone()
    }
}
