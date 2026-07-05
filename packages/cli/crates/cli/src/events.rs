//! The event and effect vocabulary of the reducer.
//!
//! Everything the app reacts to arrives as one [`AppEvent`]; everything the
//! app wants done leaves as [`Effect`]s executed by
//! [`crate::runtime::EffectExecutor`]. Keeping both as plain data is what
//! makes the reducer testable without a terminal or a tokio runtime.

use agentloop_cli_core::{
    AgentKind, CatalogEntry, LoginEvent, OpenAiOAuthEvent, OpenAiOAuthMethod,
};
use agentloop_contracts::{
    Answer, CompactionSummary, Hello, ModelRef, PermissionDecision, PermissionRequestId,
    PromptInput, QuestionId, SessionEvent, SessionId, Transcript, TurnOptions, TurnSummary,
};

/// One unit of input to [`crate::app::App::update`].
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppEvent {
    /// Terminal input: keys, paste, resize, mouse.
    Term(crossterm::event::Event),
    /// One event from the engine's session stream.
    Engine(Box<SessionEvent>),
    /// Completion of spawned async work.
    Task(TaskResult),
    /// Progress of a running device-flow login.
    Login(LoginEvent),
    /// Progress of a running OpenAI OAuth sign-in.
    OpenAiOAuth(OpenAiOAuthEvent),
    /// 100ms heartbeat: spinner animation and toast expiry.
    Tick,
    /// OS interrupt (Ctrl+C / SIGINT) delivered outside the key stream.
    Interrupt,
}

/// Engine hello + providers without an active session (home screen).
#[derive(Debug, Clone)]
pub struct EngineBootstrap {
    pub kind: AgentKind,
    pub hello: Hello,
    pub providers: Vec<String>,
    pub trace: Vec<String>,
    pub mcp_enabled: usize,
}

/// Everything the reducer needs to (re)configure itself around a session:
/// produced at startup and after every engine/agent switch.
#[derive(Debug)]
pub struct SessionBootstrap {
    /// Which agent implementation serves the session.
    pub kind: AgentKind,
    /// The agent's handshake (capabilities, commands, identity).
    pub hello: Hello,
    /// The live session.
    pub session: SessionId,
    /// Registered provider ids (empty for delegated agents).
    pub providers: Vec<String>,
    /// Initial model preference carried into `TurnOptions.model`.
    pub model: Option<ModelRef>,
    /// Present when an earlier session was resumed: its materialized history.
    pub transcript: Option<Transcript>,
    /// Human-readable resolution trace (logged, shown on `/agent` switches).
    pub trace: Vec<String>,
    /// Initial permission mode from session creation.
    pub permission_mode: Option<agentloop_contracts::PermissionMode>,
    /// Enabled MCP servers connected in this native session.
    pub mcp_enabled: usize,
    /// Set when reload could not resume and opened a fresh session instead.
    pub session_restarted: bool,
    /// Whether this session runs in an isolated workspace (git worktree).
    pub isolated: bool,
    /// Session title when set by the engine.
    pub session_title: Option<String>,
    /// Session creation time (epoch ms) for the sidebar.
    pub session_created_at_ms: Option<u64>,
}

/// Completion of spawned async work, reported back into the reducer.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TaskResult {
    /// The `prompt()` future resolved (the turn ended, for any reason).
    TurnFinished(Result<TurnSummary, String>),
    /// Model catalog fetch finished.
    Models(Result<Vec<CatalogEntry>, String>),
    /// An engine/agent switch finished.
    EngineSwitched(Box<Result<SessionBootstrap, String>>),
    /// `/new` finished: a fresh session on the current service.
    SessionReset(Result<SessionId, String>),
    /// `/clear` finished: chat wiped and a fresh session on the current service.
    SessionCleared(Result<SessionId, String>),
    /// Transcript re-fetch after a `Gap` finished.
    Resynced(Result<Transcript, String>),
    /// The device-flow login task finished.
    LoginFinished(Result<(), String>),
    /// A `/command` shell invocation finished.
    ShellCommand {
        command: String,
        outcome: ShellCommandOutcome,
    },
    /// `/compact` finished.
    CompactFinished(Result<CompactionSummary, String>),
    /// `/connect` validation finished. On success `config.models` is filled
    /// from the endpoint when the user supplied none; the payload is the
    /// discovered model count.
    ProviderValidated {
        id: String,
        config: agentloop_cli_core::ProviderConfig,
        result: Result<usize, String>,
    },
    /// `/mcp-install` finished.
    McpInstallFinished(Result<String, String>),
    /// MCP explorer listed tools for a server.
    McpToolsListed {
        server: String,
        result: Result<Vec<agentloop_mcp::McpRemoteTool>, String>,
    },
    /// MCP explorer manual tool call finished.
    McpToolCalled {
        server: String,
        tool: String,
        result: Result<String, String>,
    },
    /// Native engine reload finished (MCP toggle/install).
    EngineReloaded(Box<Result<SessionBootstrap, String>>),
    /// Permission response could not be delivered to the engine.
    PermissionRespondFailed { message: String },
    /// `/merge` finished: on success the (display) message; the session is no
    /// longer isolated.
    WorkspaceIntegrated(Result<String, String>),
    /// `/discard` finished.
    WorkspaceDiscarded(Result<String, String>),
    /// `/isolation` status finished.
    WorkspaceStatusReported(Result<String, String>),
    /// `/sessions` list finished.
    SessionsListed(Vec<agentloop_cli_core::SessionSummary>),
    /// In-TUI session resume finished.
    SessionResumed(Box<Result<SessionBootstrap, String>>),
    /// First prompt on the home screen opened a session.
    SessionOpened(Result<SessionBootstrap, String>),
    /// OpenAI OAuth via `/connect` finished.
    OpenAiOAuthFinished(Result<(), String>),
}

/// Result of a `/command` shell invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommandOutcome {
    /// Process exited (stdout and stderr combined).
    Completed {
        output: String,
        exit_code: Option<i32>,
    },
    /// User cancelled via Esc while the process was running.
    Cancelled { partial_output: String },
    /// Failed to spawn or wait on the process.
    Failed { message: String },
}

/// One side effect requested by the reducer.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Run a turn (spawned; never awaited on the render path).
    SubmitPrompt {
        input: PromptInput,
        opts: TurnOptions,
    },
    /// Gracefully interrupt the running turn.
    CancelTurn,
    /// Answer a pending permission request.
    RespondPermission {
        id: PermissionRequestId,
        decision: PermissionDecision,
        /// Child session for relayed subagent prompts; `None` = current.
        session: Option<SessionId>,
    },
    /// Answer a pending question round-trip.
    RespondQuestion {
        id: QuestionId,
        answers: Vec<Answer>,
        /// Child session for relayed subagent prompts; `None` = current.
        session: Option<SessionId>,
    },
    /// Fetch the model catalog across all registered providers.
    ListModels,
    /// Switch to (or rebuild) an agent service; opens or resumes a session.
    SwitchAgent {
        kind: AgentKind,
        /// Drop the cached service first (post-login refresh).
        invalidate: bool,
    },
    /// Start a fresh session on the current service.
    NewSession,
    /// Cancel the turn, wipe chat, and start a blank session (`/clear`).
    ClearSession,
    /// Start the GitHub Copilot device-flow login.
    StartLogin,
    /// Cancel a login in progress.
    CancelLogin,
    /// Run a shell command in the session working directory.
    RunShellCommand { command: String },
    /// Cancel a running `/command` invocation.
    CancelShellCommand,
    /// Rebuild history from the store after a `Gap` event.
    Resync { from_seq: u64 },
    /// Open a URL in the system browser (login verification page).
    OpenBrowser { url: String },
    /// Exit the application.
    Quit,
    /// Enable or disable SGR mouse capture (wheel scroll vs native select).
    SetMouseCapture(bool),
    /// Copy text to the system clipboard.
    CopyToClipboard { text: String },
    /// Persist the last selected model for the next launch.
    SaveLastModel(ModelRef),
    /// Probe a candidate custom provider (`/connect`) by listing its models.
    ValidateProvider {
        id: String,
        config: agentloop_cli_core::ProviderConfig,
    },
    /// Summarize conversation history and record a compaction boundary.
    CompactSession { opts: TurnOptions },
    /// Rebuild the native engine (MCP config changed).
    ReloadEngine {
        /// Drop the cached native service first.
        invalidate: bool,
    },
    /// Install an MCP server (blocking work in the runtime).
    McpInstall {
        target: agentloop_cli_core::InstallTarget,
        registry_id: Option<String>,
        import_path: Option<std::path::PathBuf>,
    },
    /// List tools for the MCP explorer overlay.
    McpListTools { server: String },
    /// Call a tool from the MCP explorer overlay.
    McpCallTool {
        server: String,
        tool: String,
        args_json: String,
    },
    /// Sync permission mode into an in-flight native turn.
    SetTurnPermissionMode {
        mode: Option<agentloop_contracts::PermissionMode>,
    },
    /// Verify and integrate the current session's isolated workspace.
    IntegrateWorkspace,
    /// Discard the current session's isolated workspace.
    DiscardWorkspace,
    /// Report isolation status and pending diff for the current session.
    WorkspaceStatus,
    /// Change the workspace-isolation policy for future sessions, then rebuild
    /// the native service so it takes effect (current session is unaffected).
    SetIsolation {
        policy: agentloop_contracts::IsolationPolicy,
    },
    /// List recent sessions for the working directory (`/sessions`).
    ListSessions,
    /// Switch to a persisted session by id.
    ResumeSession { id: SessionId },
    /// Open the first session after the home screen (lazy session creation).
    OpenSession { model: Option<ModelRef> },
    /// Start OpenAI ChatGPT OAuth from the connect wizard.
    StartOpenAiOAuth { method: OpenAiOAuthMethod },
    /// Cancel an in-flight OpenAI OAuth sign-in.
    CancelOpenAiOAuth,
}
