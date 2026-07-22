//! `IndexPlugin` — the agentic code-index capability.
//!
//! Contributes `SearchCode`, `FindSymbol`, and `RepoMap`, backed by a
//! per-repo lexical (BM25) + symbol (+ optional embedding) index built
//! lazily on first use. Optionally installs an [`AutoContextHook`] that
//! injects top-k chunks into the first user message of a turn — **off by
//! default**; enable via [`IndexPlugin::with_auto_context`] or
//! `AGENTLOOP_AUTO_CONTEXT=1`.
//!
//! Index refresh on tool use is also opt-in ([`IndexPlugin::with_auto_update`]
//! / `AGENTLOOP_INDEX_AUTO_UPDATE=1`): when off (default), a warm on-disk
//! index is reused across chats; Settings → Rebuild (or turning auto-update
//! on) refreshes it.

use std::sync::Arc;

use agentloop_core::{Hook, Plugin, Tool};

use crate::auto_context::{AutoContextHook, env_auto_context_enabled};
use crate::tools::shared::{IndexOpenMode, env_auto_update_enabled};
use crate::tools::{FindSymbolTool, RepoMapTool, SearchCodeTool};

/// The agentic code-index plugin.
///
/// Enabled via `AgentBuilder::enable_plugin("index")` (env-gated auto-context
/// / auto-update) or composed explicitly with
/// [`IndexPlugin::with_auto_context`] / [`IndexPlugin::with_auto_update`].
#[derive(Debug, Clone, Copy)]
pub struct IndexPlugin {
    auto_context: bool,
    auto_update: bool,
}

impl Default for IndexPlugin {
    fn default() -> Self {
        Self {
            auto_context: env_auto_context_enabled(),
            auto_update: env_auto_update_enabled(),
        }
    }
}

impl IndexPlugin {
    /// Construct with auto-context / auto-update from their env vars.
    pub fn new() -> Self {
        Self::default()
    }

    /// Force auto-context on or off (overrides the env default).
    pub fn with_auto_context(mut self, on: bool) -> Self {
        self.auto_context = on;
        self
    }

    /// Force index auto-update on tool use on or off (overrides the env default).
    ///
    /// When off, SearchCode / FindSymbol / RepoMap reuse a warm on-disk index
    /// and only build when the index is empty. When on, each tool call
    /// incrementally rescans for changed files (previous always-on behavior).
    pub fn with_auto_update(mut self, on: bool) -> Self {
        self.auto_update = on;
        self
    }

    pub fn auto_context(&self) -> bool {
        self.auto_context
    }

    pub fn auto_update(&self) -> bool {
        self.auto_update
    }

    fn open_mode(&self) -> IndexOpenMode {
        IndexOpenMode::from_auto_update(self.auto_update)
    }
}

impl Plugin for IndexPlugin {
    fn id(&self) -> &'static str {
        "index"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let mode = self.open_mode();
        vec![
            Arc::new(SearchCodeTool::new(mode)),
            Arc::new(FindSymbolTool::new(mode)),
            Arc::new(RepoMapTool::new(mode)),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "# Code index\n\
             Prefer `SearchCode` for natural-language / keyword code search, \
             `FindSymbol` for exact identifier lookup, and `RepoMap` when you \
             need a high-level map of an unfamiliar repository."
                .to_owned(),
        )
    }

    fn hooks(&self) -> Vec<Arc<dyn Hook>> {
        if self.auto_context {
            vec![Arc::new(
                AutoContextHook::new(true).with_open_mode(self.open_mode()),
            )]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_three_tools() {
        let plugin = IndexPlugin::new()
            .with_auto_context(false)
            .with_auto_update(false);
        let tools = plugin.tools();
        let names: Vec<_> = tools.iter().map(|t| t.descriptor().name).collect();
        assert!(names.contains(&"SearchCode".to_owned()), "{names:?}");
        assert!(names.contains(&"FindSymbol".to_owned()), "{names:?}");
        assert!(names.contains(&"RepoMap".to_owned()), "{names:?}");
        assert!(plugin.hooks().is_empty());
        assert!(!plugin.auto_update());
    }

    #[test]
    fn auto_context_installs_hook() {
        let plugin = IndexPlugin::new().with_auto_context(true);
        assert_eq!(plugin.hooks().len(), 1);
    }
}
