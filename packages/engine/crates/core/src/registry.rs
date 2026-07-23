use std::collections::BTreeMap;
use std::sync::Arc;

use agentloop_contracts::{ModelRef, ProviderId};

use crate::provider::{Provider, ToolSpec};
use crate::tool::Tool;

#[derive(Debug, Clone, Default)]
pub struct ToolFilter {
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

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.descriptor().name, tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

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

#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, Arc<dyn Provider>>,
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
