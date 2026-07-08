//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and native-loop composition
//! over a *prebuilt* [`ProviderRegistry`]. It is provider-agnostic: it never
//! constructs concrete providers or delegators — the `providers` facade
//! resolves those and hands the registry (plus a default model) to
//! [`EngineService::native`].

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, Answer, CompactionSummary, Hello, IntegrationOutcome, IsolationPolicy, ModelRef,
    NewSessionParams, PermissionDecision, PermissionMode, PermissionRequestId, PromptInput,
    QuestionId, SessionEvent, SessionId, SessionMeta, Transcript, TurnId, TurnOptions, TurnSummary,
    now_ms, reduce,
};
use agentloop_core::{
    Agent, EventStream, Hook, PluginRegistry, PluginRole, PluginRoleTools, ProviderRegistry,
    SessionStore, WorkspaceStatus, Workspaces,
};
pub use agentloop_hooks::{CheckSpec, DiagnosticsConfig, FormatterSpec};
use agentloop_hooks::{DiagnosticsHook, FormatOnEditHook};
pub use agentloop_loop::roles::{RoleError, RoleRegistry, RoleSpec, RoleToolProfile, valid_name};
use agentloop_loop::{LoopLimits, NativeAgentBuilder};
use agentloop_mcp::McpManager;
use agentloop_prompts::{
    CommandDiscoveryConfig, CommandRegistry, SkillDiscoveryConfig, SkillRegistry,
    SystemPromptAssembler, SystemPromptConfig, Vars,
};
use agentloop_session::MemoryStore;
use agentloop_tools::BaseTools;

mod error;
mod options;

pub use error::{EngineResult, EngineServiceError};
pub use options::{EngineConfig, OutputVerbosity};

#[derive(Clone)]
pub struct EngineService {
    agent: Arc<dyn Agent>,
    store: Arc<dyn SessionStore>,
    commands: Arc<CommandRegistry>,
    /// Providers backing a native service; empty for delegated agents.
    providers: ProviderRegistry,
    /// Isolation backend, for integrating/discarding a session's workspace.
    workspace: Option<Arc<dyn Workspaces>>,
    /// Run-level default isolation for sessions that don't request their own.
    isolation_default: IsolationPolicy,
    /// Verify command run before integrating a workspace back.
    verify_command: Option<String>,
    /// NDJSON output verbosity level.
    verbosity: OutputVerbosity,
}

/// Map a loop-independent [`PluginRole`] onto the loop's [`RoleSpec`].
fn plugin_role_to_spec(role: PluginRole) -> RoleSpec {
    let tools = match role.tools {
        PluginRoleTools::ReadOnly => RoleToolProfile::ReadOnly,
        PluginRoleTools::Full => RoleToolProfile::Full,
        PluginRoleTools::Allow(list) => RoleToolProfile::Allow(list),
    };
    RoleSpec {
        models: role.models,
        tools,
        prompt: role.prompt,
        isolation: role.isolation,
        ..RoleSpec::new(role.name)
    }
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

    /// Compose the native loop over a prebuilt [`ProviderRegistry`] and an
    /// optional default [`ModelRef`], plus the engine-scoped [`EngineConfig`].
    ///
    /// Provider *selection and construction* happen outside the engine (the
    /// `providers` facade); this constructor is provider-agnostic. Enabled
    /// plugins from `config.plugins` contribute tools, prompt fragments, and
    /// roles, folded in deterministically. An empty registry is valid — the
    /// service opens with no default model and the failure defers to turn time.
    pub fn native(
        providers: ProviderRegistry,
        default_model: Option<ModelRef>,
        mut config: EngineConfig,
    ) -> EngineResult<Self> {
        let plugins = PluginRegistry::from_plugins(std::mem::take(&mut config.plugins));

        let BaseTools {
            registry: mut tools,
            pending_questions,
            ..
        } = if config.cwd.is_some() {
            agentloop_tools::base_tools()
        } else {
            agentloop_tools::base_tools_read_only()
        };
        for tool in plugins.tools() {
            tools.register(tool);
        }

        let mut roles = config.roles.clone();
        roles.extend(plugins.roles().into_iter().map(plugin_role_to_spec));

        let role_registry = RoleRegistry::with_defaults(roles.clone())?;
        tools.register(agentloop_tools::subagent_tool(&role_registry.spawnable()));

        let cwd_display = config
            .cwd
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "headless".to_string());
        let system_prompt = SystemPromptAssembler::new(SystemPromptConfig {
            appends: plugins.prompt_fragments(),
            ..SystemPromptConfig::default()
        })
        .assemble(&Vars {
            cwd: cwd_display,
            date: config.date.clone(),
        })?;

        let project_cmd_dir = config
            .cwd
            .as_ref()
            .map(|cwd| cwd.join(".agent").join("commands"));
        let commands = CommandRegistry::discover(CommandDiscoveryConfig {
            user_dir: default_user_command_dir(),
            project_dir: project_cmd_dir,
        })?;

        let project_skill_dir = config
            .cwd
            .as_ref()
            .map(|cwd| cwd.join(".agent").join("skills"));
        let skills = Arc::new(SkillRegistry::discover(SkillDiscoveryConfig {
            user_dir: default_user_skill_dir(),
            project_dir: project_skill_dir,
        })?);
        if let Some(tool) = agentloop_tools::skill_tool(&skills.model_visible(), {
            let skills = skills.clone();
            Arc::new(move |name: &str| skills.load_body(name).ok())
        }) {
            tools.register(tool);
        }

        let mcp_manager = match config.mcp_manager.take() {
            Some(manager) => Some(manager),
            None if config.mcp.servers.is_empty() => None,
            None => Some(Arc::new(McpManager::from_config_blocking_default(
                config.mcp.clone(),
            )?)),
        };

        let store: Arc<dyn SessionStore> = config
            .session_store
            .take()
            .unwrap_or_else(|| Arc::new(MemoryStore::new()));
        let limits = LoopLimits {
            max_iterations: resolve_max_iterations(config.max_iterations),
            ..LoopLimits::default()
        };
        let mut builder = NativeAgentBuilder::new(store.clone())
            .providers(providers.clone())
            .tools(tools)
            .questions(pending_questions)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .roles(roles)
            .limits(limits);
        if let Some(model) = default_model {
            builder = builder.default_model(model);
        }
        if let Some(manager) = mcp_manager {
            builder = builder.mcp(manager);
        }
        let mut hooks: Vec<Arc<dyn Hook>> = Vec::new();
        let formatter = FormatOnEditHook::new(config.formatters.clone());
        if formatter.is_active() {
            hooks.push(Arc::new(formatter));
        }
        let diagnostics = DiagnosticsHook::new(config.diagnostics.clone());
        if diagnostics.is_active() {
            hooks.push(Arc::new(diagnostics));
        }
        if !hooks.is_empty() {
            builder = builder.hooks(hooks);
        }
        if let Some(workspace) = &config.workspace {
            builder = builder.workspace(workspace.clone());
        }
        let agent = builder.build();
        let mut service = Self::with_commands(agent, store, commands);
        service.providers = providers;
        service.workspace = config.workspace;
        service.isolation_default = config.isolation_default;
        service.verify_command = config.verify_command;
        service.verbosity = config.verbosity;
        Ok(service)
    }

    pub fn hello(&self) -> Hello {
        let mut caps = self.agent.capabilities();
        if caps.commands.is_empty() {
            caps.commands = self.commands.infos();
        }
        Hello::new(caps)
    }

    pub async fn create_session(&self, mut params: NewSessionParams) -> EngineResult<SessionId> {
        if params.isolation.is_none() && self.isolation_default.wants_isolation() {
            params.isolation = Some(self.isolation_default);
        }
        Ok(self.agent.create_session(params).await?)
    }

    /// Whether the session currently runs in an isolated workspace (still
    /// pointing at its worktree — false once integrated/discarded).
    pub async fn is_isolated(&self, session: &SessionId) -> EngineResult<bool> {
        Ok(active_workspace(&self.store.get_meta(session).await?).is_some())
    }

    /// Report the pending changes in a session's isolated workspace. `Ok(None)`
    /// when the session isn't isolated or no backend is configured.
    pub async fn workspace_status(
        &self,
        session: &SessionId,
    ) -> EngineResult<Option<WorkspaceStatus>> {
        let meta = self.store.get_meta(session).await?;
        let Some((_, _, root)) = active_workspace(&meta) else {
            return Ok(None);
        };
        match &self.workspace {
            Some(backend) => Ok(Some(backend.status(&root).await?)),
            None => Ok(None),
        }
    }

    /// Verify and integrate a session's isolated workspace back into its base
    /// tree. On a clean merge the workspace is removed and the session's cwd is
    /// repointed to the base directory; the outcome is returned to the caller.
    pub async fn integrate_session(&self, session: &SessionId) -> EngineResult<IntegrationOutcome> {
        let meta = self.store.get_meta(session).await?;
        let Some((_workspace_id, base, root)) = active_workspace(&meta) else {
            return Err(EngineServiceError::NotIsolated(session.clone()));
        };
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        let outcome = backend
            .integrate(&root, &base, self.verify_command.as_deref())
            .await?;
        if matches!(
            outcome,
            IntegrationOutcome::Merged { .. } | IntegrationOutcome::Empty
        ) {
            self.repoint_to_base(session, base).await?;
        }
        Ok(outcome)
    }

    /// Discard a session's isolated workspace without integrating, repointing
    /// the session to its base directory.
    pub async fn discard_session(&self, session: &SessionId) -> EngineResult<()> {
        let meta = self.store.get_meta(session).await?;
        let Some((_workspace_id, base, root)) = active_workspace(&meta) else {
            return Err(EngineServiceError::NotIsolated(session.clone()));
        };
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        backend.discard(&root, &base).await?;
        self.repoint_to_base(session, base).await?;
        Ok(())
    }

    /// Rewind a session's working tree to a prior per-turn snapshot (backs
    /// `/undo` and `/redo`). Restores files under the session's `cwd` without
    /// moving any git branch, then records a [`AgentEvent::SnapshotRestored`]
    /// audit marker (the append-only log is retained). Works whether or not the
    /// session is isolated. Errors if no workspace backend is configured or the
    /// snapshot id is unknown.
    pub async fn revert(&self, session: &SessionId, snapshot_id: &str) -> EngineResult<()> {
        let meta = self.store.get_meta(session).await?;
        let backend = self
            .workspace
            .as_ref()
            .ok_or(EngineServiceError::NoWorkspaceBackend)?;
        backend.restore(&meta.cwd, snapshot_id).await?;
        self.store
            .append(
                session,
                &[AgentEvent::SnapshotRestored {
                    snapshot_id: snapshot_id.to_owned(),
                }],
            )
            .await?;
        Ok(())
    }

    /// Point the session's cwd back at the base tree after its workspace is
    /// integrated or discarded (which also makes it read as no longer isolated).
    async fn repoint_to_base(&self, session: &SessionId, base: PathBuf) -> EngineResult<()> {
        self.store
            .update_meta(
                session,
                agentloop_contracts::SessionMetaPatch {
                    cwd: Some(base),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    pub async fn resume_session(&self, id: &SessionId) -> EngineResult<()> {
        Ok(self.agent.resume_session(id).await?)
    }

    /// Load persisted metadata for a session.
    pub async fn session_meta(&self, session: &SessionId) -> EngineResult<SessionMeta> {
        Ok(self.store.get_meta(session).await?)
    }

    pub async fn list_sessions(&self) -> EngineResult<Vec<SessionMeta>> {
        Ok(self.agent.list_sessions().await?)
    }

    pub fn subscribe(&self, session: &SessionId) -> EngineResult<EventStream> {
        Ok(self.agent.events(session)?)
    }

    pub async fn prompt(
        &self,
        session: &SessionId,
        input: PromptInput,
        opts: TurnOptions,
    ) -> EngineResult<TurnSummary> {
        let input = self.commands.expand_input(input);
        Ok(self.agent.prompt(session, input, opts).await?)
    }

    /// Summarize conversation history and record a compaction boundary.
    pub async fn compact(
        &self,
        session: &SessionId,
        opts: TurnOptions,
    ) -> EngineResult<CompactionSummary> {
        Ok(self.agent.compact(session, opts).await?)
    }

    pub async fn cancel(&self, session: &SessionId) -> EngineResult<()> {
        Ok(self.agent.cancel(session).await?)
    }

    /// Push a permission-mode change into an in-flight native turn.
    pub fn set_turn_permission_mode(
        &self,
        session: &SessionId,
        mode: Option<PermissionMode>,
    ) -> EngineResult<()> {
        Ok(self.agent.set_turn_permission_mode(session, mode)?)
    }

    pub async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> EngineResult<()> {
        Ok(self.agent.respond_permission(session, id, decision).await?)
    }

    pub async fn respond_question(
        &self,
        session: &SessionId,
        id: QuestionId,
        answers: Vec<Answer>,
    ) -> EngineResult<()> {
        Ok(self.agent.respond_question(session, id, answers).await?)
    }

    pub async fn replay(
        &self,
        session: &SessionId,
        from_seq: u64,
    ) -> EngineResult<Vec<SessionEvent>> {
        let events = self.store.read(session, 0).await?;
        let mut current_turn: Option<TurnId> = None;
        let mut replay = Vec::new();
        for (seq, payload) in events {
            if let AgentEvent::TurnStarted { turn_id } = &payload {
                current_turn = Some(turn_id.clone());
            }
            if seq >= from_seq {
                replay.push(SessionEvent {
                    session_id: session.clone(),
                    seq,
                    turn_id: current_turn.clone(),
                    ts_ms: now_ms(),
                    payload: payload.clone(),
                });
            }
            if matches!(payload, AgentEvent::TurnCompleted { .. }) {
                current_turn = None;
            }
        }
        Ok(replay)
    }

    pub async fn session_items(&self, session: &SessionId) -> EngineResult<Transcript> {
        let events = self.store.read(session, 0).await?;
        let payloads = events.iter().map(|(_, event)| event).collect::<Vec<_>>();
        Ok(reduce(payloads))
    }
}

/// A session's *active* isolated workspace: `Some((workspace_id, base, root))`
/// only while its cwd still points at the worktree. Once integrate/discard
/// repoints cwd back to `base_cwd`, this returns `None` — so a session reads as
/// isolated exactly once, and a second integrate/discard is a clean no-op error
/// rather than operating on the base tree.
fn active_workspace(meta: &SessionMeta) -> Option<(String, PathBuf, PathBuf)> {
    let workspace_id = meta.workspace_id.clone()?;
    let base = meta.base_cwd.clone()?;
    if meta.cwd == base {
        return None;
    }
    Some((workspace_id, base, meta.cwd.clone()))
}

fn default_user_command_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("commands")
    })
}

fn default_user_skill_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("skills")
    })
}

/// A configured `max_iterations` overrides the loop's own default; `None`
/// keeps it.
fn resolve_max_iterations(configured: Option<u32>) -> u32 {
    configured.unwrap_or(LoopLimits::default().max_iterations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_max_iterations_uses_configured_value_or_falls_back_to_loop_default() {
        assert_eq!(resolve_max_iterations(Some(2_000)), 2_000);
        assert_eq!(
            resolve_max_iterations(None),
            LoopLimits::default().max_iterations
        );
    }

    use agentloop_loop::NativeAgentBuilder;
    use agentloop_session::MemoryStore;
    use agentloop_testkit::MockWorkspaces;

    /// An `EngineService` wired to a mock isolation backend that provisions
    /// successfully, over a real (empty) native agent and an in-memory store.
    fn isolated_service(
        store: std::sync::Arc<MemoryStore>,
    ) -> (EngineService, std::sync::Arc<MockWorkspaces>) {
        let mock = std::sync::Arc::new(MockWorkspaces::new());
        let agent = NativeAgentBuilder::new(store.clone())
            .workspace(mock.clone())
            .build();
        let mut service = EngineService::new(agent, store);
        service.workspace = Some(mock.clone());
        service.isolation_default = IsolationPolicy::Required;
        (service, mock)
    }

    async fn open_isolated(service: &EngineService) -> SessionId {
        service
            .create_session(NewSessionParams {
                cwd: Some(PathBuf::from("/repo")),
                ..NewSessionParams::default()
            })
            .await
            .expect("isolated session opens")
    }

    #[tokio::test]
    async fn integrate_repoints_cwd_and_records_outcome() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let (service, mock) = isolated_service(store.clone());
        let id = open_isolated(&service).await;
        assert_ne!(
            store.get_meta(&id).await.expect("meta").cwd,
            PathBuf::from("/repo")
        );

        let outcome = service.integrate_session(&id).await.expect("integrate");
        assert!(matches!(outcome, IntegrationOutcome::Merged { .. }));
        assert_eq!(mock.integrate_calls(), 1);

        let meta = store.get_meta(&id).await.expect("meta");
        assert_eq!(
            meta.cwd,
            PathBuf::from("/repo"),
            "cwd repointed to base after merge"
        );
        assert!(!service.is_isolated(&id).await.expect("meta"));
    }

    #[tokio::test]
    async fn discard_repoints_cwd_to_base() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let (service, mock) = isolated_service(store.clone());
        let id = open_isolated(&service).await;

        service.discard_session(&id).await.expect("discard");
        assert_eq!(mock.discard_calls(), 1);
        let meta = store.get_meta(&id).await.expect("meta");
        assert_eq!(meta.cwd, PathBuf::from("/repo"));
        assert!(!service.is_isolated(&id).await.expect("meta"));
    }

    #[tokio::test]
    async fn status_reports_for_isolated_only() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let (service, _mock) = isolated_service(store.clone());
        let id = open_isolated(&service).await;
        assert!(
            service
                .workspace_status(&id)
                .await
                .expect("status")
                .is_some()
        );
    }

    #[tokio::test]
    async fn integrate_on_a_non_isolated_session_errors() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let mock = std::sync::Arc::new(MockWorkspaces::new());
        let agent = NativeAgentBuilder::new(store.clone()).build();
        let service = EngineService::new(agent, store.clone());
        let id = service
            .create_session(NewSessionParams {
                cwd: Some(PathBuf::from("/repo")),
                ..NewSessionParams::default()
            })
            .await
            .expect("plain session");
        assert!(matches!(
            service.integrate_session(&id).await,
            Err(EngineServiceError::NotIsolated(_))
        ));
        assert_eq!(mock.integrate_calls(), 0);
    }

    #[tokio::test]
    async fn revert_restores_workspace_and_records_marker() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let (service, mock) = isolated_service(store.clone());
        let id = open_isolated(&service).await;

        service.revert(&id, "snap-abc").await.expect("revert ok");
        assert_eq!(mock.restore_calls(), 1, "workspace restored once");

        let events = store.read(&id, 0).await.expect("events");
        assert!(
            events.iter().any(|(_, e)| matches!(
                e,
                AgentEvent::SnapshotRestored { snapshot_id } if snapshot_id == "snap-abc"
            )),
            "a SnapshotRestored audit marker was appended to the log"
        );
    }

    #[tokio::test]
    async fn revert_without_a_workspace_backend_errors() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let agent = NativeAgentBuilder::new(store.clone()).build();
        let service = EngineService::new(agent, store.clone());
        let id = service
            .create_session(NewSessionParams {
                cwd: Some(PathBuf::from("/repo")),
                ..NewSessionParams::default()
            })
            .await
            .expect("plain session");
        assert!(matches!(
            service.revert(&id, "snap-x").await,
            Err(EngineServiceError::NoWorkspaceBackend)
        ));
    }
}
