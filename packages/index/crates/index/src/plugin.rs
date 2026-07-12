//! `IndexPlugin` — the agentic code-index capability.
//!
//! Contributes `SearchCode`, `FindSymbol`, and `RepoMap`, backed by a
//! per-repo lexical (BM25) + symbol (+ optional embedding) index built
//! lazily on first use. Optionally installs an [`AutoContextHook`] that
//! injects top-k chunks into the first user message of a turn — **off by
//! default**; enable via [`IndexPlugin::with_auto_context`] or
//! `AGENTLOOP_AUTO_CONTEXT=1`.

use std::sync::Arc;

use agentloop_core::{Hook, Plugin, Tool};

use crate::auto_context::{AutoContextHook, env_auto_context_enabled};
use crate::tools::{FindSymbolTool, RepoMapTool, SearchCodeTool};

/// The agentic code-index plugin.
///
/// Enabled via `AgentBuilder::enable_plugin("index")` (env-gated auto-context)
/// or composed explicitly with [`IndexPlugin::with_auto_context`].
#[derive(Debug, Clone, Copy)]
pub struct IndexPlugin {
    auto_context: bool,
}

impl Default for IndexPlugin {
    fn default() -> Self {
        Self {
            auto_context: env_auto_context_enabled(),
        }
    }
}

impl IndexPlugin {
    /// Construct with auto-context from [`crate::auto_context::AUTO_CONTEXT_ENV`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Force auto-context on or off (overrides the env default).
    pub fn with_auto_context(mut self, on: bool) -> Self {
        self.auto_context = on;
        self
    }

    pub fn auto_context(&self) -> bool {
        self.auto_context
    }
}

impl Plugin for IndexPlugin {
    fn id(&self) -> &'static str {
        "index"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(SearchCodeTool),
            Arc::new(FindSymbolTool),
            Arc::new(RepoMapTool),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "# Code index\n\
             Prefer `SearchCode` for natural-language / keyword code search, \
             `FindSymbol` for exact identifier lookup, and `RepoMap` to orient \
             in an unfamiliar repository before diving into files."
                .to_owned(),
        )
    }

    fn hooks(&self) -> Vec<Arc<dyn Hook>> {
        if self.auto_context {
            vec![Arc::new(AutoContextHook::new(true))]
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
        let plugin = IndexPlugin::new().with_auto_context(false);
        let tools = plugin.tools();
        let names: Vec<_> = tools.iter().map(|t| t.descriptor().name).collect();
        assert!(names.contains(&"SearchCode".to_owned()), "{names:?}");
        assert!(names.contains(&"FindSymbol".to_owned()), "{names:?}");
        assert!(names.contains(&"RepoMap".to_owned()), "{names:?}");
        assert!(plugin.hooks().is_empty());
    }

    #[test]
    fn auto_context_installs_hook() {
        let plugin = IndexPlugin::new().with_auto_context(true);
        assert_eq!(plugin.hooks().len(), 1);
    }
}
