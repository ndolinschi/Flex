//! [`EngineService`] — the ready runtime front door.

use std::sync::Arc;

use agentloop_contracts::IsolationPolicy;
use agentloop_core::{
    Agent, BackgroundProcessRegistry, DemoteRegistry, ProviderRegistry, SessionStore,
};
use agentloop_prompts::CommandRegistry;

use crate::options::OutputVerbosity;

#[derive(Clone)]
pub struct EngineService {
    pub(crate) agent: Arc<dyn Agent>,
    pub(crate) store: Arc<dyn SessionStore>,
    pub(crate) commands: Arc<CommandRegistry>,
    /// Providers backing a native service; empty for delegated agents.
    pub(crate) providers: ProviderRegistry,
    /// Isolation backend, for integrating/discarding a session's workspace.
    pub(crate) workspace: Option<Arc<dyn agentloop_core::Workspaces>>,
    /// Run-level default isolation for sessions that don't request their own.
    pub(crate) isolation_default: IsolationPolicy,
    /// Verify command run before integrating a workspace back.
    pub(crate) verify_command: Option<String>,
    /// NDJSON output verbosity level.
    pub(crate) verbosity: OutputVerbosity,
    /// Background processes started via `Bash`'s `run_in_background`, keyed
    /// by session. `None` for a headless (`cwd`-less) service, which never
    /// registers `Bash` at all. Killed per-session on `delete_session`, and
    /// entirely via [`EngineService::shutdown`] — cancel deliberately does
    /// *not* touch this: cancelling a turn aborts the turn, not servers it
    /// started. Because [`EngineService`] is [`Clone`], there is no `Drop`
    /// teardown; callers that own the last handle should call `shutdown`.
    pub(crate) background_processes: Option<Arc<BackgroundProcessRegistry>>,
    /// Demote signals for still-running foreground `Bash` calls (see
    /// `MOVE-TO-BACKGROUND`). `None` for a headless service, same as
    /// `background_processes` — `Bash` was never registered so there is
    /// nothing to demote. No teardown concerns here (unlike
    /// `background_processes`): a demote registration only lives for the
    /// duration of one in-flight call and self-cleans either way, so there
    /// is nothing to kill on session delete or service shutdown.
    pub(crate) demote_processes: Option<Arc<DemoteRegistry>>,
}

impl EngineService {
    pub fn new(agent: Arc<dyn Agent>, store: Arc<dyn SessionStore>) -> Self {
        Self {
            agent,
            store,
            commands: Arc::new(CommandRegistry::builtins()),
            providers: ProviderRegistry::new(),
            workspace: None,
            isolation_default: IsolationPolicy::Never,
            verify_command: None,
            verbosity: OutputVerbosity::default(),
            background_processes: None,
            demote_processes: None,
        }
    }

    pub fn with_commands(
        agent: Arc<dyn Agent>,
        store: Arc<dyn SessionStore>,
        commands: CommandRegistry,
    ) -> Self {
        Self {
            agent,
            store,
            commands: Arc::new(commands),
            providers: ProviderRegistry::new(),
            workspace: None,
            isolation_default: IsolationPolicy::Never,
            verify_command: None,
            verbosity: OutputVerbosity::default(),
            background_processes: None,
            demote_processes: None,
        }
    }

    /// The registry backing a native service; empty for delegated agents.
    /// Lets clients enumerate providers and list models for pickers.
    pub fn provider_registry(&self) -> &ProviderRegistry {
        &self.providers
    }

    /// The configured NDJSON output verbosity level.
    pub fn verbosity(&self) -> OutputVerbosity {
        self.verbosity
    }
}
