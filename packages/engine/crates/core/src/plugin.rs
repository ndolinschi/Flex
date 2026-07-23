use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, ModelRef};

use crate::hook::Hook;
use crate::tool::Tool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRoleTools {
    ReadOnly,
    Full,
    Allow(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct PluginRole {
    pub name: String,
    pub models: Vec<ModelRef>,
    pub tools: PluginRoleTools,
    pub prompt: Option<String>,
    pub isolation: IsolationPolicy,
}

impl PluginRole {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            models: Vec::new(),
            tools: PluginRoleTools::ReadOnly,
            prompt: None,
            isolation: IsolationPolicy::Never,
        }
    }
}

pub trait Plugin: Send + Sync {
    fn id(&self) -> &'static str;

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        Vec::new()
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        None
    }

    fn roles(&self) -> Vec<PluginRole> {
        Vec::new()
    }

    fn hooks(&self) -> Vec<Arc<dyn Hook>> {
        Vec::new()
    }

    fn force_ask_tools(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Default, Clone)]
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_plugins(plugins: Vec<Arc<dyn Plugin>>) -> Self {
        Self { plugins }
    }

    pub fn register(&mut self, plugin: Arc<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn plugins(&self) -> &[Arc<dyn Plugin>] {
        &self.plugins
    }

    pub fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.tools())
            .collect()
    }

    pub fn prompt_fragments(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter_map(|plugin| plugin.system_prompt_fragment())
            .collect()
    }

    pub fn hooks(&self) -> Vec<Arc<dyn Hook>> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.hooks())
            .collect()
    }

    pub fn roles(&self) -> Vec<PluginRole> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.roles())
            .collect()
    }

    pub fn force_ask_tools(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .plugins
            .iter()
            .flat_map(|plugin| plugin.force_ask_tools())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
