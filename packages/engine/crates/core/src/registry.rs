//! Instance-based registries (no globals — composition happens in the
//! runner, and tests build their own).

use std::collections::BTreeMap;
use std::sync::Arc;

use agentloop_contracts::{ModelRef, ProviderId};

use crate::provider::{Provider, ToolSpec};
use crate::tool::Tool;

/// Which tools a session/turn may see. Deny wins over allow.
#[derive(Debug, Clone, Default)]
pub struct ToolFilter {
    /// If non-empty, only these tool names are visible.
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

impl ToolFilter {
    pub fn permits(&self, name: &str) -> bool {
        if self.deny.iter().any(|d| d == name) {
            return false;
        }
        self.allow.is_empty() || self.allow.iter().any(|a| a == name)
    }
}

/// Registry of executable tools. Ordered (BTreeMap) so tool listings are
/// deterministic across runs — stable prompts cache better.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Re-registering a name replaces the previous tool.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.descriptor().name, tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Agent-facing specs for all tools passing the filter.
    pub fn specs(&self, filter: &ToolFilter) -> Vec<ToolSpec> {
        self.tools
            .values()
            .filter(|tool| filter.permits(&tool.descriptor().name))
            .map(|tool| tool.descriptor().spec())
            .collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Names of tools whose descriptor declares them read-only.
    pub fn read_only_names(&self) -> Vec<String> {
        self.tools
            .values()
            .filter(|tool| tool.descriptor().read_only)
            .map(|tool| tool.descriptor().name)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Registry of provider clients, with model-reference resolution.
#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, Arc<dyn Provider>>,
    /// Order in which providers are preferred for unqualified model refs.
    priority: Vec<ProviderId>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        let id = provider.id();
        if !self.priority.contains(&id) {
            self.priority.push(id.clone());
        }
        self.providers.insert(id, provider);
    }

    /// Set explicit preference order (unlisted providers keep insertion order
    /// after the listed ones).
    pub fn set_priority(&mut self, priority: Vec<ProviderId>) {
        let mut ordered = priority;
        for id in self.providers.keys() {
            if !ordered.contains(id) {
                ordered.push(id.clone());
            }
        }
        self.priority = ordered;
    }

    pub fn get(&self, id: &ProviderId) -> Option<Arc<dyn Provider>> {
        self.providers.get(id).cloned()
    }

    /// Resolve a model reference to `(provider, model_id)`.
    ///
    /// `"anthropic/claude-x"` selects the named provider; a bare `"claude-x"`
    /// selects the highest-priority registered provider.
    pub fn resolve(&self, model: &ModelRef) -> Option<(Arc<dyn Provider>, String)> {
        let (provider_part, model_part) = model.split();
        match provider_part {
            Some(provider) => {
                let id = ProviderId::from(provider);
                self.get(&id).map(|p| (p, model_part.to_owned()))
            }
            None => self
                .priority
                .first()
                .and_then(|id| self.get(id))
                .map(|p| (p, model_part.to_owned())),
        }
    }

    pub fn ids(&self) -> Vec<ProviderId> {
        self.priority.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
