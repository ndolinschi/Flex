//! The `Plugin` trait and registry — the extension seam for optional
//! capabilities composed into the engine at build time.
//!
//! A plugin contributes any of: executable [`Tool`]s, a system-prompt
//! fragment, and role definitions. Plugins are instantiated and enabled by the
//! composition root (e.g. the SDK builder) and handed to the engine, which
//! folds their contributions into the tool registry, prompt assembler, and
//! role registry. Roles are expressed with a loop-independent [`PluginRole`]
//! (the `loop` crate's `RoleSpec` lives above `core`), mapped at composition.

use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, ModelRef};

use crate::tool::Tool;

/// How a plugin-declared role derives its tool set — mirrors the loop's
/// `RoleToolProfile` without depending on it (that type lives in `loop`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRoleTools {
    /// Every registry tool whose descriptor says `read_only`.
    ReadOnly,
    /// Every registry tool.
    Full,
    /// An explicit allow-list of tool names.
    Allow(Vec<String>),
}

/// A role a plugin contributes to the engine's role registry. The engine maps
/// this to the loop's `RoleSpec` at composition time.
#[derive(Debug, Clone)]
pub struct PluginRole {
    /// Role name: `^[a-z0-9][a-z0-9_-]{0,31}$`.
    pub name: String,
    /// Ordered model preference chain; empty = inherit the spawning session's
    /// effective model.
    pub models: Vec<ModelRef>,
    /// Which tools the role may use.
    pub tools: PluginRoleTools,
    /// System-prompt addition delivered to the role's subagent turns.
    pub prompt: Option<String>,
    /// Whether a root session serving this role runs in an isolated workspace.
    pub isolation: IsolationPolicy,
}

impl PluginRole {
    /// A role with conservative defaults: inherit model, read-only tools, no
    /// isolation.
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

/// An optional capability composed into the engine at build time.
///
/// All contribution methods default to empty, so a plugin implements only what
/// it provides. Implementations must be cheap to clone-share (`Arc`).
pub trait Plugin: Send + Sync {
    /// Stable, unique id (`^[a-z0-9][a-z0-9_-]*$`), used for enable/disable and
    /// diagnostics.
    fn id(&self) -> &'static str;

    /// Executable tools the plugin adds to the registry.
    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        Vec::new()
    }

    /// A system-prompt fragment appended to the assembled base prompt.
    fn system_prompt_fragment(&self) -> Option<String> {
        None
    }

    /// Roles the plugin registers (dispatchable via the `Task` tool).
    fn roles(&self) -> Vec<PluginRole> {
        Vec::new()
    }
}

/// An ordered set of enabled plugins, consulted during engine composition.
///
/// Insertion order is preserved so tool/prompt/role contributions are
/// deterministic. A later plugin registering a tool or role of the same name
/// wins at the registry level (last-write), matching `ToolRegistry`.
#[derive(Default, Clone)]
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from an ordered list of enabled plugins.
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

    /// All tools contributed by every plugin, in registration order.
    pub fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.tools())
            .collect()
    }

    /// All system-prompt fragments, in registration order.
    pub fn prompt_fragments(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter_map(|plugin| plugin.system_prompt_fragment())
            .collect()
    }

    /// All roles contributed by every plugin, in registration order.
    pub fn roles(&self) -> Vec<PluginRole> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.roles())
            .collect()
    }
}
