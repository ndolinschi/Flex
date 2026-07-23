use std::sync::Arc;

use agentloop_contracts::{Answer, ModeSwitchId, QuestionId};
use agentloop_core::{
    BackgroundProcessRegistry, DemoteRegistry, Executor, NetworkPolicy, PendingMap, ToolRegistry,
};

use crate::{
    AskQuestionTool, BashTool, EditTool, ExitPlanModeTool, FsState, GlobTool, GrepTool, PlanTool,
    ReadTool, WebFetchTool, WriteTool,
};

pub struct BaseTools {
    pub registry: ToolRegistry,
    pub fs_state: Arc<FsState>,
    pub pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    pub pending_mode_switches: Arc<PendingMap<ModeSwitchId, bool>>,
    pub background_processes: Option<Arc<BackgroundProcessRegistry>>,
    pub demote_processes: Option<Arc<DemoteRegistry>>,
}

pub fn base_tools_read_only() -> BaseTools {
    let fs_state = Arc::new(FsState::new());
    let pending_questions = Arc::new(PendingMap::new());
    let pending_mode_switches = Arc::new(PendingMap::new());
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
        pending_mode_switches,
        background_processes: None,
        demote_processes: None,
    }
}

pub fn base_tools(executor: Arc<dyn Executor>, network: NetworkPolicy) -> BaseTools {
    let fs_state = Arc::new(FsState::new());
    let pending_questions = Arc::new(PendingMap::new());
    let pending_mode_switches = Arc::new(PendingMap::new());
    let background_processes = Arc::new(BackgroundProcessRegistry::new());
    let demote_processes = Arc::new(DemoteRegistry::new());
    let registry = registry_with_questions_and_background(
        fs_state.clone(),
        pending_questions.clone(),
        executor,
        network,
        background_processes.clone(),
        demote_processes.clone(),
    );
    BaseTools {
        registry,
        fs_state,
        pending_questions,
        pending_mode_switches,
        background_processes: Some(background_processes),
        demote_processes: Some(demote_processes),
    }
}

pub fn registry_with_questions(
    fs_state: Arc<FsState>,
    pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    executor: Arc<dyn Executor>,
    network: NetworkPolicy,
) -> ToolRegistry {
    registry_with_questions_and_background(
        fs_state,
        pending_questions,
        executor,
        network,
        Arc::new(BackgroundProcessRegistry::new()),
        Arc::new(DemoteRegistry::new()),
    )
}

pub fn registry_with_questions_and_background(
    fs_state: Arc<FsState>,
    pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    executor: Arc<dyn Executor>,
    network: NetworkPolicy,
    background_processes: Arc<BackgroundProcessRegistry>,
    demote_processes: Arc<DemoteRegistry>,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadTool::new(fs_state.clone())));
    registry.register(Arc::new(WriteTool::new(fs_state.clone())));
    registry.register(Arc::new(EditTool::new(fs_state)));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(
        BashTool::new(executor)
            .with_network(network)
            .with_background_registry(background_processes)
            .with_demote_registry(demote_processes),
    ));
    registry.register(Arc::new(WebFetchTool::new()));
    registry.register(Arc::new(PlanTool));
    registry.register(Arc::new(ExitPlanModeTool));
    registry.register(Arc::new(AskQuestionTool::new(pending_questions)));
    registry
}
