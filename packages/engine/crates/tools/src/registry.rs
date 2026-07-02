//! Registry composition for the M1 base tools.

use std::sync::Arc;

use agentloop_contracts::{Answer, QuestionId};
use agentloop_core::{PendingMap, ToolRegistry};

use crate::{
    AskQuestionTool, BashTool, EditTool, FsState, GlobTool, GrepTool, ReadTool, TaskListTool,
    WebFetchTool, WriteTool,
};

/// The full M1 tool bundle plus shared state needed by the native loop.
pub struct BaseTools {
    pub registry: ToolRegistry,
    pub fs_state: Arc<FsState>,
    pub pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
}

/// Build the default M1 base tools with fresh filesystem and question state.
pub fn base_tools() -> BaseTools {
    let fs_state = Arc::new(FsState::new());
    let pending_questions = Arc::new(PendingMap::new());
    let registry = registry_with_questions(fs_state.clone(), pending_questions.clone());
    BaseTools {
        registry,
        fs_state,
        pending_questions,
    }
}

/// Build a registry around caller-owned shared state.
pub fn registry_with_questions(
    fs_state: Arc<FsState>,
    pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadTool::new(fs_state.clone())));
    registry.register(Arc::new(WriteTool::new(fs_state.clone())));
    registry.register(Arc::new(EditTool::new(fs_state)));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(BashTool));
    registry.register(Arc::new(WebFetchTool::new()));
    registry.register(Arc::new(TaskListTool));
    registry.register(Arc::new(AskQuestionTool::new(pending_questions)));
    registry
}
