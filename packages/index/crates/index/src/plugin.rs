use std::sync::Arc;

use agentloop_core::{Hook, Plugin, Tool};

use crate::auto_context::{AutoContextHook, env_auto_context_enabled};
use crate::tools::shared::{IndexOpenMode, env_auto_update_enabled};
use crate::tools::{FindSymbolTool, RepoMapTool, SearchCodeTool};

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_auto_context(mut self, on: bool) -> Self {
        self.auto_context = on;
        self
    }

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
             Prefer `SearchCode` for natural-language / keyword code search and \
             `FindSymbol` for exact identifier lookup. Call `RepoMap` at most \
             once per project when you need a high-level orientation map of an \
             unfamiliar repository — it is cached across chats until the index \
             changes. Skip `RepoMap` when you already know the area to edit or \
             a prior turn already mapped this workspace."
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
