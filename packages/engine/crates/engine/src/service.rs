use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, ModeSwitchId};
use agentloop_core::{
    Agent, BackgroundProcessRegistry, DemoteRegistry, PendingMap, ProviderRegistry, SessionStore,
};
use agentloop_prompts::CommandRegistry;

use crate::options::OutputVerbosity;

#[derive(Clone)]
pub struct EngineService {
    pub(crate) agent: Arc<dyn Agent>,
    pub(crate) store: Arc<dyn SessionStore>,
    pub(crate) commands: Arc<CommandRegistry>,
    pub(crate) providers: ProviderRegistry,
    pub(crate) workspace: Option<Arc<dyn agentloop_core::Workspaces>>,
    pub(crate) isolation_default: IsolationPolicy,
    pub(crate) verify_command: Option<String>,
    pub(crate) verbosity: OutputVerbosity,
    pub(crate) background_processes: Option<Arc<BackgroundProcessRegistry>>,
    pub(crate) demote_processes: Option<Arc<DemoteRegistry>>,
    pub(crate) pending_mode_switches: Option<Arc<PendingMap<ModeSwitchId, bool>>>,
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
            pending_mode_switches: None,
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
            pending_mode_switches: None,
        }
    }

    pub fn provider_registry(&self) -> &ProviderRegistry {
        &self.providers
    }

    pub fn verbosity(&self) -> OutputVerbosity {
        self.verbosity
    }
}
