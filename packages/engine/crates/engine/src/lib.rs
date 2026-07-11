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
    AgentEvent, Answer, CompactionSummary, GoalOutcome, GoalSpec, GoalStopReason, Hello,
    IntegrationOutcome, IsolationPolicy, ModelRef, NewSessionParams, PermissionDecision,
    PermissionMode, PermissionRequestId, PromptInput, QuestionId, SessionEvent, SessionId,
    SessionMeta, SessionMetaPatch, TokenUsage, ToolCallStatus, Transcript, TurnId, TurnOptions,
    TurnStopReason, TurnSummary, VerdictOutcome, VerificationVerdict, now_ms, reduce,
};
use agentloop_core::{
    Agent, BackgroundEntrySummary, BackgroundProcessRegistry, DemoteRegistry, EventStream, Hook,
    PluginRegistry, PluginRole, PluginRoleTools, ProviderRegistry, SessionStore, WorkspaceStatus,
    Workspaces,
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
    /// Background processes started via `Bash`'s `run_in_background`, keyed
    /// by session. `None` for a headless (`cwd`-less) service, which never
    /// registers `Bash` at all. Killed per-session on `delete_session`, and
    /// entirely on `EngineService` drop (see the `Drop` impl below) — cancel
    /// deliberately does *not* touch this: cancelling a turn aborts the
    /// turn, not servers it started.
    background_processes: Option<Arc<BackgroundProcessRegistry>>,
    /// Demote signals for still-running foreground `Bash` calls (see
    /// `MOVE-TO-BACKGROUND`). `None` for a headless service, same as
    /// `background_processes` — `Bash` was never registered so there is
    /// nothing to demote. No teardown concerns here (unlike
    /// `background_processes`): a demote registration only lives for the
    /// duration of one in-flight call and self-cleans either way, so there
    /// is nothing to kill on session delete or service shutdown.
    demote_processes: Option<Arc<DemoteRegistry>>,
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

        let executor = config.executor.take();
        let executor_id = executor.as_ref().map(|backend| backend.id().to_owned());
        let executor = executor.unwrap_or_else(|| Arc::new(agentloop_executors::LocalExecutor));
        let BaseTools {
            registry: mut tools,
            pending_questions,
            background_processes,
            demote_processes,
            ..
        } = if config.cwd.is_some() {
            agentloop_tools::base_tools(executor, config.network)
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
        if config.enable_workflow_tool {
            tools.register(agentloop_tools::workflow_tool(&role_registry.spawnable()));
        }

        let cwd_display = config
            .cwd
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "headless".to_string());
        let mut appends = plugins.prompt_fragments();
        if let Some(memory) =
            agentloop_prompts::load_memory_section(&agentloop_prompts::MemoryConfig {
                dir: default_user_memory_dir(),
                budget_chars: 0,
            })
        {
            appends.push(memory);
        }
        let system_prompt = SystemPromptAssembler::new(SystemPromptConfig {
            appends,
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

        if let Some(user_skill_dir) = default_user_skill_dir() {
            match agentloop_prompts::install_bundled_skills(&user_skill_dir) {
                Ok(installed) if !installed.is_empty() => {
                    tracing::debug!(?installed, "seeded bundled skills into user skill dir");
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(%err, "failed to seed bundled skills; continuing without them");
                }
            }
        }

        let project_skill_dir = config
            .cwd
            .as_ref()
            .map(|cwd| cwd.join(".agent").join("skills"));
        let skills = Arc::new(SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: default_user_skill_dir().map(|dir| dir.join("learned")),
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
            retry: config.retry_policy.clone().unwrap_or_default(),
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
        if !config.default_fallback_models.is_empty() {
            builder = builder.default_fallback_models(config.default_fallback_models.clone());
        }
        let force_ask_tools = plugins.force_ask_tools();
        if !force_ask_tools.is_empty() {
            builder = builder.policy(
                agentloop_loop::PermissionPolicy::new(PermissionMode::Default)
                    .with_force_ask(force_ask_tools),
            );
        }
        if let Some(id) = executor_id {
            builder = builder.executor_id(id);
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
        if config.injection_scan {
            hooks.insert(0, Arc::new(agentloop_hooks::InjectionScanHook::new()));
        }
        hooks.extend(plugins.hooks());
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
        service.background_processes = background_processes;
        service.demote_processes = demote_processes;
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

    /// Apply a partial update to a session's persisted metadata (e.g. rename
    /// via `title`, or patch the default `model`).
    pub async fn update_session(
        &self,
        session: &SessionId,
        patch: SessionMetaPatch,
    ) -> EngineResult<SessionMeta> {
        self.store.update_meta(session, patch).await?;
        Ok(self.store.get_meta(session).await?)
    }

    /// Cancel any in-flight turn, kill any background processes the session
    /// started via `Bash`'s `run_in_background` (a dev server left running
    /// on a deleted session would otherwise leak forever), then delete the
    /// session's event log and metadata from the store.
    pub async fn delete_session(&self, session: &SessionId) -> EngineResult<()> {
        let _ = self.agent.cancel(session).await;
        if let Some(registry) = &self.background_processes {
            registry.kill_session(session).await;
        }
        Ok(self.store.delete(session).await?)
    }

    /// Kill every background process started by any session through
    /// `Bash`'s `run_in_background`, across the whole service. Call this
    /// during process shutdown (the runner binary's signal handler, the
    /// headless HTTP transport's graceful-shutdown path, `EOF` on stdio) —
    /// a spawned child process is owned by a detached task that outlives
    /// any `Arc` clone being dropped (background processes must keep
    /// streaming after their starting tool call returns, by design), so
    /// there is no `Drop` impl that can kill them for you; this must be
    /// called explicitly. No-op for a headless service (no `cwd`, so
    /// `Bash` was never registered).
    pub async fn shutdown(&self) {
        if let Some(registry) = &self.background_processes {
            registry.kill_all().await;
        }
    }

    /// List background processes (started via `Bash`'s `run_in_background`)
    /// tracked for `session`, for a "background processes" panel. Empty for
    /// a headless service (no registry — `Bash` was never registered) or a
    /// session with none running.
    pub fn background_list(&self, session: &SessionId) -> Vec<BackgroundEntrySummary> {
        match &self.background_processes {
            Some(registry) => registry.list(session),
            None => Vec::new(),
        }
    }

    /// Kill one background process by id. Returns `false` if `id` is unknown
    /// for `session` (already reaped or never existed) or if this service
    /// has no background-process registry at all.
    pub async fn background_kill(&self, session: &SessionId, id: &str) -> EngineResult<bool> {
        match &self.background_processes {
            Some(registry) => Ok(registry.kill(session, id).await?),
            None => Ok(false),
        }
    }

    /// Ask a still-running **foreground** `Bash` call to move to the
    /// background (see `MOVE-TO-BACKGROUND`): the tool call returns early
    /// ("moved to background…") and the process keeps running as a tracked
    /// background entry, reachable afterward through
    /// [`Self::background_list`]/[`Self::background_kill`] under the same
    /// `id`. Returns `false` — not an error — when there's nothing to do:
    /// unknown id, the call already finished naturally, or the session's
    /// execution backend doesn't support demote at all (docker, ssh, …;
    /// only the local backend does). Callers (the desktop UI) should treat
    /// `false` as "no visible effect," not surface it as a failure.
    pub fn background_demote(&self, session: &SessionId, id: &str) -> bool {
        match &self.demote_processes {
            Some(registry) => registry.request_demote(session, id),
            None => false,
        }
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

    /// Drive repeated turns on `session` toward `goal`, stopping at the
    /// first applicable rule (see `agentloop_contracts::goal` for the
    /// stop-reason vocabulary — `Parked` is reserved and never returned
    /// here). Each iteration after the first re-states the goal rather than
    /// repeating the original prompt verbatim, since a single-turn "continue"
    /// nudge is what actually drives repeated turns forward.
    pub async fn run_goal(&self, session: &SessionId, goal: GoalSpec) -> EngineResult<GoalOutcome> {
        let mut turns = Vec::new();
        let mut total_usage = TokenUsage::default();
        let mut failures = FailureCounts::default();
        let mut next_prompt = goal.prompt.clone();
        let mut iterations = 0u32;

        loop {
            if iterations >= goal.max_iterations {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::MaxIterations,
                    iterations,
                    total_usage,
                    turns,
                });
            }
            if let Some(budget) = goal.token_budget {
                if total_usage.input + total_usage.output >= budget {
                    return Ok(GoalOutcome {
                        stop_reason: GoalStopReason::TokenBudgetExceeded,
                        iterations,
                        total_usage,
                        turns,
                    });
                }
            }

            let summary = self
                .prompt(
                    session,
                    PromptInput::text(next_prompt.clone()),
                    TurnOptions::default(),
                )
                .await?;
            iterations += 1;
            total_usage.add(&summary.usage);
            turns.push(summary.clone());

            if summary.stop_reason == TurnStopReason::Cancelled {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::Cancelled,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            if failures.record(summary.stop_reason) >= goal.max_identical_failures {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::IdenticalFailureCeiling,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            if goal.require_verification {
                match self.verify_goal_progress(session, &goal.prompt).await? {
                    Some(verdict) if verdict.outcome == VerdictOutcome::Pass => {
                        return Ok(GoalOutcome {
                            stop_reason: GoalStopReason::Achieved,
                            iterations,
                            total_usage,
                            turns,
                        });
                    }
                    Some(verdict) => {
                        next_prompt = format!(
                            "An independent verifier checked this against the goal and found \
                             issues:\n{}\n\nAddress them, then continue.",
                            verdict.findings.join("\n")
                        );
                        continue;
                    }
                    // The verifier plugin is disabled, or the model didn't
                    // call Verify — fall through to the weaker signal below
                    // rather than loop forever on a check that can't run.
                    None => {}
                }
            } else if summary.stop_reason == TurnStopReason::EndTurn && summary.num_tool_calls == 0
            {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::Achieved,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            next_prompt = format!(
                "Continue working toward this goal:\n{}\n\nIf you believe it's fully \
                 complete, say so explicitly and stop calling tools.",
                goal.prompt
            );
        }
    }

    /// Prompt the model to call `Verify` against `goal_prompt`, then read the
    /// resulting structured verdict back out of the session's own log (same
    /// extraction the `verifier` plugin's `Verify` tool already performs for
    /// the caller that spawned it — see `agentloop_loop::subagent`). Returns
    /// `None` when no completed `Verify` call is found (tool unavailable, or
    /// the model didn't call it).
    async fn verify_goal_progress(
        &self,
        session: &SessionId,
        goal_prompt: &str,
    ) -> EngineResult<Option<VerificationVerdict>> {
        let verify_prompt = format!(
            "Call the Verify tool now — rubric: \"{goal_prompt}\" is fully and correctly \
             done. List the files you changed (or the relevant output) as artifacts. Call \
             Verify exactly once; do no other work this turn."
        );
        self.prompt(
            session,
            PromptInput::text(verify_prompt),
            TurnOptions::default(),
        )
        .await?;
        let events = self.store.read(session, 0).await?;
        Ok(events.iter().rev().find_map(|(_, event)| {
            let AgentEvent::ToolCallUpdated { call } = event else {
                return None;
            };
            if call.tool_name != agentloop_core::tool::VERIFIER_TOOL_NAME
                || call.status != ToolCallStatus::Completed
            {
                return None;
            }
            call.result
                .as_ref()
                .and_then(|output| output.structured.clone())
                .and_then(|value| serde_json::from_value::<VerificationVerdict>(value).ok())
        }))
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

/// Per-category failure tally for [`EngineService::run_goal`]. Deliberately
/// coarse — grouped by [`TurnStopReason`], not by error message — since a
/// `TurnSummary` carries no error text (that lives in a separate
/// `SessionError` event); this is the signal actually available without
/// scanning the log for prose to fuzzy-match.
#[derive(Debug, Default)]
struct FailureCounts {
    error: u32,
    max_iterations: u32,
    refusal: u32,
}

impl FailureCounts {
    /// Record `stop_reason` if it is failure-like, returning the updated
    /// count for its category (`0` for a non-failure stop reason).
    fn record(&mut self, stop_reason: TurnStopReason) -> u32 {
        match stop_reason {
            TurnStopReason::Error => {
                self.error += 1;
                self.error
            }
            TurnStopReason::MaxIterations => {
                self.max_iterations += 1;
                self.max_iterations
            }
            TurnStopReason::Refusal => {
                self.refusal += 1;
                self.refusal
            }
            _ => 0,
        }
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

fn default_user_memory_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("agentloop")
            .join("memory")
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

    mod run_goal {
        use agentloop_testkit::{EchoTool, MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};

        use super::*;

        fn default_model() -> ModelRef {
            ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
        }

        fn goal_service(
            provider: Arc<MockProvider>,
            limits: LoopLimits,
        ) -> (EngineService, Arc<MemoryStore>) {
            let store = Arc::new(MemoryStore::new());
            let mut providers = ProviderRegistry::new();
            providers.register(provider);
            let mut tools = agentloop_core::ToolRegistry::new();
            tools.register(Arc::new(EchoTool));
            let agent = NativeAgentBuilder::new(store.clone())
                .providers(providers)
                .tools(tools)
                .limits(limits)
                .system_prompt("test agent")
                .default_model(default_model())
                .build();
            (EngineService::new(agent, store.clone()), store)
        }

        fn spec(prompt: &str, max_iterations: u32, max_identical_failures: u32) -> GoalSpec {
            GoalSpec {
                prompt: prompt.to_owned(),
                max_iterations,
                max_identical_failures,
                token_budget: None,
                require_verification: false,
            }
        }

        #[tokio::test]
        async fn achieves_when_the_model_stops_calling_tools() {
            let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn(
                "all done, nothing left to do",
            )]));
            let (service, _store) = goal_service(provider, LoopLimits::default());
            let session = service
                .create_session(NewSessionParams::default())
                .await
                .expect("session");

            let outcome = service
                .run_goal(&session, spec("say hello", 5, 3))
                .await
                .expect("goal runs");

            assert_eq!(outcome.stop_reason, GoalStopReason::Achieved);
            assert_eq!(outcome.iterations, 1);
            assert_eq!(outcome.turns.len(), 1);
        }

        #[tokio::test]
        async fn stops_at_max_iterations_when_the_model_keeps_working() {
            // Each run_goal iteration is one full turn: a tool call, then a
            // text reply that ends it — scripted so the loop never sees the
            // weak "no tool calls" achieved signal.
            let mut turns = Vec::new();
            for _ in 0..2 {
                let (tool_turn, _ids) =
                    MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "x"}))]);
                turns.push(tool_turn);
                turns.push(MockProvider::text_turn("still working"));
            }
            let provider = Arc::new(MockProvider::with_turns(turns));
            let (service, _store) = goal_service(provider, LoopLimits::default());
            let session = service
                .create_session(NewSessionParams::default())
                .await
                .expect("session");

            let outcome = service
                .run_goal(&session, spec("keep working", 2, 10))
                .await
                .expect("goal runs");

            assert_eq!(outcome.stop_reason, GoalStopReason::MaxIterations);
            assert_eq!(outcome.iterations, 2);
        }

        #[tokio::test]
        async fn stops_at_identical_failure_ceiling() {
            // A turn-level max_iterations of 1 means any turn whose first
            // model call produces a tool call (needing a second round to
            // close out) is truncated as `TurnStopReason::MaxIterations` —
            // a controllable, repeatable "failure" for this test.
            let (turn_a, _) =
                MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "x"}))]);
            let (turn_b, _) =
                MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "y"}))]);
            let provider = Arc::new(MockProvider::with_turns([turn_a, turn_b]));
            let (service, _store) = goal_service(
                provider,
                LoopLimits {
                    max_iterations: 1,
                    ..LoopLimits::default()
                },
            );
            let session = service
                .create_session(NewSessionParams::default())
                .await
                .expect("session");

            let outcome = service
                .run_goal(&session, spec("do the thing", 10, 2))
                .await
                .expect("goal runs");

            assert_eq!(outcome.stop_reason, GoalStopReason::IdenticalFailureCeiling);
            assert_eq!(outcome.iterations, 2);
            assert!(
                outcome
                    .turns
                    .iter()
                    .all(|turn| turn.stop_reason == TurnStopReason::MaxIterations)
            );
        }
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
    async fn update_session_renames_title() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let agent = NativeAgentBuilder::new(store.clone()).build();
        let service = EngineService::new(agent, store);
        let id = service
            .create_session(NewSessionParams {
                title: Some("old".to_owned()),
                ..NewSessionParams::default()
            })
            .await
            .expect("session");

        let meta = service
            .update_session(
                &id,
                SessionMetaPatch {
                    title: Some("renamed".to_owned()),
                    ..Default::default()
                },
            )
            .await
            .expect("update");
        assert_eq!(meta.title.as_deref(), Some("renamed"));
        assert_eq!(
            service
                .session_meta(&id)
                .await
                .expect("meta")
                .title
                .as_deref(),
            Some("renamed")
        );
    }

    #[tokio::test]
    async fn delete_session_removes_from_store() {
        let store = std::sync::Arc::new(MemoryStore::new());
        let agent = NativeAgentBuilder::new(store.clone()).build();
        let service = EngineService::new(agent, store.clone());
        let id = service
            .create_session(NewSessionParams::default())
            .await
            .expect("session");
        assert_eq!(service.list_sessions().await.expect("list").len(), 1);

        service.delete_session(&id).await.expect("delete");
        assert!(service.list_sessions().await.expect("list").is_empty());
        assert!(store.get_meta(&id).await.is_err());
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
