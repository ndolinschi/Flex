//! Registry composition for the M1 base tools.

use std::sync::Arc;

use agentloop_contracts::{Answer, ModeSwitchId, QuestionId};
use agentloop_core::{
    BackgroundProcessRegistry, DemoteRegistry, Executor, NetworkPolicy, PendingMap, ToolRegistry,
};

use crate::{
    AskQuestionTool, BashTool, EditTool, ExitPlanModeTool, FsState, GlobTool, GrepTool, PlanTool,
    ReadTool, WebFetchTool, WriteTool,
};

/// The full M1 tool bundle plus shared state needed by the native loop.
pub struct BaseTools {
    pub registry: ToolRegistry,
    pub fs_state: Arc<FsState>,
    pub pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    /// Pending mode-switch proposals waiting for the client to accept or
    /// veto. Always created; only populated when the `SwitchMode` tool is
    /// registered (i.e. `EngineConfig::enable_switch_mode` is true). The
    /// composition root threads this through to `NativeAgentBuilder` so
    /// `respond_mode_switch` can reach the same map the tool waits on.
    pub pending_mode_switches: Arc<PendingMap<ModeSwitchId, bool>>,
    /// Background processes started via `Bash`'s `run_in_background`,
    /// keyed by session. `None` when the read-only tool set is used (no
    /// `Bash`, so nothing ever populates it). The composition root threads
    /// this through to session teardown so a deleted session's dev
    /// servers/watchers get killed rather than orphaned.
    pub background_processes: Option<Arc<BackgroundProcessRegistry>>,
    /// Demote signals for still-running foreground `Bash` calls (see
    /// `MOVE-TO-BACKGROUND`). `None` when the read-only tool set is used (no
    /// `Bash`). The composition root threads this through to
    /// `background_demote` so it can reach the same table `Bash`'s
    /// foreground exec path registers into.
    pub demote_processes: Option<Arc<DemoteRegistry>>,
}

/// Build the read-only subset of the M1 base tools (no Write, Edit, Bash)
/// with fresh filesystem and question state. Used for headless/research mode
/// where there is no project directory to modify.
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

/// Build the default M1 base tools with fresh filesystem and question state.
/// Shell commands run through `executor` (local process by default at the
/// composition root; container/remote backends when configured).
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

/// Build a registry around caller-owned shared state, with a private
/// background-process registry (nothing outside `Bash` itself can reach it —
/// fine for callers that don't need session-scoped teardown, e.g. tests).
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

/// Same as [`registry_with_questions`], but shares caller-owned
/// background-process and demote registries with `Bash` (so the caller can
/// enumerate/kill a session's background processes, or demote a running
/// foreground call, from outside the tool).
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
