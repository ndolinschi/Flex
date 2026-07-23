mod gate;
mod hook;
mod memory;
mod save;

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_core::{Hook, Plugin, Tool};

pub use gate::VerifiedMemoryGateHook;
pub use hook::SkillLearningHook;
pub use memory::MemoryWriteTool;
pub use save::SkillSaveTool;

pub fn default_learned_skill_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("skills")
            .join("learned")
    })
}

pub fn default_memory_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("memory")
    })
}

pub struct LearningPlugin {
    learned_dir: PathBuf,
    memory_dir: Option<PathBuf>,
    require_verified_memory: bool,
    require_human_approval: bool,
}

impl LearningPlugin {
    pub fn new(learned_dir: impl Into<PathBuf>) -> Self {
        Self {
            learned_dir: learned_dir.into(),
            memory_dir: default_memory_dir(),
            require_verified_memory: false,
            require_human_approval: false,
        }
    }

    pub fn with_memory_dir(mut self, memory_dir: Option<PathBuf>) -> Self {
        self.memory_dir = memory_dir;
        self
    }

    pub fn require_verified_memory(mut self, on: bool) -> Self {
        self.require_verified_memory = on;
        self
    }

    pub fn require_human_approval(mut self, on: bool) -> Self {
        self.require_human_approval = on;
        self
    }

    pub fn with_default_dir() -> Option<Self> {
        default_learned_skill_dir().map(Self::new)
    }

    pub fn learned_dir(&self) -> &PathBuf {
        &self.learned_dir
    }
}

impl Plugin for LearningPlugin {
    fn id(&self) -> &'static str {
        "learning"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> =
            vec![Arc::new(SkillSaveTool::new(self.learned_dir.clone()))];
        if let Some(memory_dir) = &self.memory_dir {
            tools.push(Arc::new(MemoryWriteTool::new(memory_dir.clone())));
        }
        tools
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "# Learned skills\n\
             You can persist procedural knowledge across sessions. When you \
             complete a task whose *procedure* was non-obvious and verified to \
             work, you may save it as a reusable skill with the `SkillSave` \
             tool. Save procedures, not facts; only what you verified in this \
             session; at most one skill per session."
                .to_owned(),
        )
    }

    fn hooks(&self) -> Vec<Arc<dyn Hook>> {
        let mut hooks: Vec<Arc<dyn Hook>> = vec![Arc::new(SkillLearningHook::new())];
        if self.require_verified_memory {
            hooks.push(Arc::new(VerifiedMemoryGateHook::new()));
        }
        hooks
    }

    fn force_ask_tools(&self) -> Vec<String> {
        if self.require_human_approval {
            vec!["SkillSave".to_owned(), "MemoryWrite".to_owned()]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_ask_tools_is_empty_by_default() {
        let plugin = LearningPlugin::new("/tmp/learned");
        assert!(plugin.force_ask_tools().is_empty());
    }

    #[test]
    fn require_human_approval_force_asks_both_memory_tools() {
        let plugin = LearningPlugin::new("/tmp/learned").require_human_approval(true);
        assert_eq!(
            plugin.force_ask_tools(),
            vec!["SkillSave".to_owned(), "MemoryWrite".to_owned()]
        );
    }

    #[test]
    fn verified_memory_alone_does_not_force_ask() {
        let plugin = LearningPlugin::new("/tmp/learned").require_verified_memory(true);
        assert!(plugin.force_ask_tools().is_empty());
        assert_eq!(plugin.hooks().len(), 2, "verify gate hook is still added");
    }

    #[test]
    fn verify_and_ask_compose() {
        let plugin = LearningPlugin::new("/tmp/learned")
            .require_verified_memory(true)
            .require_human_approval(true);
        assert_eq!(plugin.hooks().len(), 2, "verify gate hook is still added");
        assert_eq!(
            plugin.force_ask_tools(),
            vec!["SkillSave".to_owned(), "MemoryWrite".to_owned()]
        );
    }
}
