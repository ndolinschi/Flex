//! Registry composition for the M1 base tools.

use std::sync::Arc;

use agentloop_contracts::{Answer, QuestionId};
use agentloop_core::{Executor, NetworkPolicy, PendingMap, ToolRegistry};

use crate::{
    AskQuestionTool, BashTool, EditTool, ExitPlanModeTool, FsState, GlobTool, GrepTool, PlanTool,
    ReadTool, WebFetchTool, WriteTool,
};

/// The full M1 tool bundle plus shared state needed by the native loop.
pub struct BaseTools {
    pub registry: ToolRegistry,
    pub fs_state: Arc<FsState>,
    pub pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
}

/// Build the read-only subset of the M1 base tools (no Write, Edit, Bash)
/// with fresh filesystem and question state. Used for headless/research mode
/// where there is no project directory to modify.
pub fn base_tools_read_only() -> BaseTools {
    let fs_state = Arc::new(FsState::new());
    let pending_questions = Arc::new(PendingMap::new());
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadTool::new(fs_state.clone())));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(WebFetchTool::new()));
    registry.register(Arc::new(PlanTool));
    registry.register(Arc::new(ExitPlanModeTool));
    registry.register(Arc::new(AskQuestionTool::new(pending_questions.clone())));
    BaseTools {
        registry,
        fs_state,
        pending_questions,
    }
}

/// Build the default M1 base tools with fresh filesystem and question state.
/// Shell commands run through `executor` (local process by default at the
/// composition root; container/remote backends when configured).
pub fn base_tools(executor: Arc<dyn Executor>, network: NetworkPolicy) -> BaseTools {
    let fs_state = Arc::new(FsState::new());
    let pending_questions = Arc::new(PendingMap::new());
    let registry = registry_with_questions(
        fs_state.clone(),
        pending_questions.clone(),
        executor,
        network,
    );
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
    executor: Arc<dyn Executor>,
    network: NetworkPolicy,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadTool::new(fs_state.clone())));
    registry.register(Arc::new(WriteTool::new(fs_state.clone())));
    registry.register(Arc::new(EditTool::new(fs_state)));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(BashTool::new(executor).with_network(network)));
    registry.register(Arc::new(WebFetchTool::new()));
    registry.register(Arc::new(PlanTool));
    registry.register(Arc::new(ExitPlanModeTool));
    registry.register(Arc::new(AskQuestionTool::new(pending_questions)));
    registry
}
