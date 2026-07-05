//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and real-provider native-loop
//! composition. It intentionally does not expose provider wire types or install
//! mock providers at runtime.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, Answer, CompactionSummary, EngineError, ErrorCode, Hello, IntegrationOutcome,
    IsolationPolicy, ModelInfo, ModelRef, NewSessionParams, PermissionDecision, PermissionMode,
    PermissionRequestId, PromptInput, ProviderId, QuestionId, SessionEvent, SessionId, SessionMeta,
    Transcript, TurnId, TurnOptions, TurnSummary, now_ms, reduce,
};
use agentloop_core::{
    Agent, AgentError, EventStream, Hook, ProviderError, ProviderRegistry, SessionStore,
    StoreError, WorkspaceError, WorkspaceStatus, Workspaces,
};
pub use agentloop_hooks::{CheckSpec, DiagnosticsConfig, FormatterSpec};
use agentloop_hooks::{DiagnosticsHook, FormatOnEditHook};
pub use agentloop_loop::roles::{RoleError, RoleRegistry, RoleSpec, RoleToolProfile, valid_name};
use agentloop_loop::{LoopLimits, NativeAgentBuilder};
use agentloop_mcp::{McpBridgeConfig, McpBridgeError, McpManager};
use agentloop_prompts::{
    CommandDiscoveryConfig, CommandError, CommandRegistry, PromptError, SkillDiscoveryConfig,
    SkillRegistry, SystemPromptAssembler, SystemPromptConfig, Vars,
};
use agentloop_provider_anthropic::{ANTHROPIC_PROVIDER_ID, AnthropicProvider};
use agentloop_provider_copilot::{COPILOT_PROVIDER_ID, CopilotConfig, CopilotProvider};
use agentloop_provider_gemini::{GEMINI_PROVIDER_ID, GeminiProvider};
use agentloop_provider_ollama::{OLLAMA_PROVIDER_ID, OllamaProvider};
use agentloop_provider_openai::{OPENAI_PROVIDER_ID, OpenAiConfig, OpenAiProvider};
use agentloop_session::MemoryStore;
use agentloop_tools::BaseTools;

/// One client-configured OpenAI-compatible provider, registered alongside the
/// built-in providers under its own id.
#[derive(Debug, Clone)]
pub struct CustomProviderSpec {
    /// Registry id; must match `^[a-z0-9][a-z0-9_-]*$` (no `/`, which is the
    /// [`ModelRef`] separator) and must not collide with a built-in id.
    pub id: String,
    /// Chat Completions base URL (e.g. `https://api.deepseek.com/v1`).
    pub base_url: String,
    /// API key, already resolved by the caller (never an env reference).
    pub api_key: String,
    /// Default model; falls back to the first entry of `models`, then to the
    /// OpenAI config default as a documented last resort.
    pub default_model: Option<String>,
    /// Static model catalog served without a network call; may be empty for
    /// endpoints that implement `/models`.
    pub models: Vec<ModelInfo>,
    /// Advertise + forward extended-thinking config (DeepSeek-style APIs).
    pub thinking: bool,
}

#[derive(Clone)]
pub struct EngineOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cwd: PathBuf,
    pub date: String,
    /// Client-configured OpenAI-compatible providers, registered after the
    /// built-ins in vec order.
    pub custom: Vec<CustomProviderSpec>,
    /// Role definitions for multi-agent orchestration (built-ins always present).
    pub roles: Vec<RoleSpec>,
    /// MCP servers bridged into the native tool registry.
    pub mcp: McpBridgeConfig,
    /// Pre-built MCP manager; when set, the engine reuses it instead of creating a new one.
    pub mcp_manager: Option<std::sync::Arc<McpManager>>,
    /// Reuse an existing session store across native service rebuilds (CLI MCP reload).
    pub session_store: Option<Arc<dyn SessionStore>>,
    /// Model-call iterations per turn; `None` keeps the engine default
    /// (currently 500 — a backstop against a runaway loop, not a budget for
    /// normal work).
    pub max_iterations: Option<u32>,
    /// Isolation backend. When set, root sessions can run in an isolated
    /// workspace; when `None`, isolation requests degrade or fail per policy.
    pub workspace: Option<Arc<dyn Workspaces>>,
    /// Run-level default isolation applied to a root session that doesn't
    /// request its own (and whose role doesn't force one). `Never` = off.
    pub isolation_default: IsolationPolicy,
    /// Command run inside an isolated workspace before integrating it back
    /// (e.g. `"cargo test"`). `None` skips verification.
    pub verify_command: Option<String>,
    /// Formatters run after `Write`/`Edit` (format-on-edit). Empty = off.
    /// Each spec is availability-gated on its command resolving on `$PATH`.
    pub formatters: Vec<FormatterSpec>,
    /// Diagnostics feedback run after `Write`/`Edit`. Disabled by default;
    /// availability-gated on the check command resolving on `$PATH`.
    pub diagnostics: DiagnosticsConfig,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            cwd: PathBuf::from("."),
            date: String::new(),
            custom: Vec::new(),
            roles: Vec::new(),
            mcp: McpBridgeConfig::default(),
            mcp_manager: None,
            session_store: None,
            max_iterations: None,
            workspace: None,
            isolation_default: IsolationPolicy::Never,
            verify_command: None,
            formatters: Vec::new(),
            diagnostics: DiagnosticsConfig::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineServiceError {
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Prompt(#[from] PromptError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Skill(#[from] agentloop_prompts::SkillError),
    #[error(transparent)]
    Mcp(#[from] McpBridgeError),
    #[error(transparent)]
    Role(#[from] RoleError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("session {0} is not isolated")]
    NotIsolated(SessionId),
    #[error("no workspace backend is configured")]
    NoWorkspaceBackend,
    #[error(
        "provider `{0}` is not available in this build; supported runtime providers: `openai`, `anthropic`, `gemini`, `ollama`, or a provider configured in the client's config"
    )]
    UnsupportedProvider(String),
    #[error("custom provider `{0}` conflicts with a built-in provider id")]
    CustomProviderConflict(String),
    #[error("custom provider `{id}` is invalid: {message}")]
    CustomProviderInvalid { id: String, message: String },
}

impl EngineServiceError {
    pub fn to_engine_error(&self) -> EngineError {
        match self {
            Self::Agent(err) => err.to_engine_error(),
            Self::Provider(err) => err.to_engine_error(),
            Self::Store(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::Prompt(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Command(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Skill(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Mcp(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Role(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Workspace(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::NotIsolated(_) | Self::NoWorkspaceBackend => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
            Self::UnsupportedProvider(_)
            | Self::CustomProviderConflict(_)
            | Self::CustomProviderInvalid { .. } => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
        }
    }
}

pub type EngineResult<T> = Result<T, EngineServiceError>;

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
        }
    }

    /// Native loop with a single provider resolved from `options.provider` or
    /// the environment.
    pub fn native(mut options: EngineOptions) -> EngineResult<Self> {
        let model = options.model.take();
        let (providers, default_model) =
            resolve_real_providers(options.provider.as_deref(), model, &options.custom)?;
        Self::build_native(providers, Some(default_model), options)
    }

    /// Native loop with every provider whose credentials resolve, so
    /// provider-qualified [`ModelRef`]s can switch providers per turn.
    ///
    /// `options.provider` names the preferred provider (it must resolve and
    /// becomes the priority for bare model refs); `options.model` picks the
    /// default model, qualified against the preferred provider unless it
    /// already carries a `provider/` prefix. Succeeds with an empty registry
    /// and no default model when nothing resolves — see
    /// [`resolve_available_providers`].
    pub fn native_all(mut options: EngineOptions) -> EngineResult<Self> {
        let model = options.model.take();
        let (providers, default_model) =
            resolve_available_providers(options.provider.as_deref(), model, &options.custom)?;
        Self::build_native(providers, default_model, options)
    }

    /// The registry backing a native service; empty for delegated agents.
    /// Lets clients enumerate providers and list models for pickers.
    pub fn provider_registry(&self) -> &ProviderRegistry {
        &self.providers
    }

    fn build_native(
        providers: ProviderRegistry,
        default_model: Option<ModelRef>,
        mut options: EngineOptions,
    ) -> EngineResult<Self> {
        let BaseTools {
            registry: mut tools,
            pending_questions,
            ..
        } = agentloop_tools::base_tools();
        // Advertise the spawnable roles on the Task tool, then register it.
        let role_registry = RoleRegistry::with_defaults(options.roles.clone())?;
        tools.register(agentloop_tools::subagent_tool(&role_registry.spawnable()));
        let system_prompt =
            SystemPromptAssembler::new(SystemPromptConfig::default()).assemble(&Vars {
                cwd: options.cwd.display().to_string(),
                date: options.date,
            })?;
        let commands = CommandRegistry::discover(CommandDiscoveryConfig {
            user_dir: default_user_command_dir(),
            project_dir: Some(options.cwd.join(".agent").join("commands")),
        })?;

        // Skills: `~/.config/agentloop/skills/*/SKILL.md` and
        // `<project>/.agent/skills/*/SKILL.md`. Only names + descriptions sit
        // in the `Skill` tool's own description (progressive disclosure); the
        // full body loads into context on invocation. No tool is registered
        // when nothing was discovered.
        let skills = Arc::new(SkillRegistry::discover(SkillDiscoveryConfig {
            user_dir: default_user_skill_dir(),
            project_dir: Some(options.cwd.join(".agent").join("skills")),
        })?);
        if let Some(tool) = agentloop_tools::skill_tool(&skills.model_visible(), {
            let skills = skills.clone();
            Arc::new(move |name: &str| skills.load_body(name).ok())
        }) {
            tools.register(tool);
        }

        let mcp_manager = match options.mcp_manager.take() {
            Some(manager) => Some(manager),
            None if options.mcp.servers.is_empty() => None,
            None => Some(Arc::new(McpManager::from_config_blocking_default(
                options.mcp.clone(),
            )?)),
        };

        let store: Arc<dyn SessionStore> = options
            .session_store
            .take()
            .unwrap_or_else(|| Arc::new(MemoryStore::new()));
        let limits = LoopLimits {
            max_iterations: resolve_max_iterations(options.max_iterations),
            ..LoopLimits::default()
        };
        let mut builder = NativeAgentBuilder::new(store.clone())
            .providers(providers.clone())
            .tools(tools)
            .questions(pending_questions)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .roles(options.roles.clone())
            .limits(limits);
        if let Some(model) = default_model {
            builder = builder.default_model(model);
        }
        if let Some(manager) = mcp_manager {
            builder = builder.mcp(manager);
        }
        // Post-edit hooks (format-on-edit, diagnostics feedback). Each is
        // included only when configured and active; otherwise the loop keeps
        // its default empty hook set and behaves byte-identically.
        let mut hooks: Vec<Arc<dyn Hook>> = Vec::new();
        let formatter = FormatOnEditHook::new(options.formatters.clone());
        if formatter.is_active() {
            hooks.push(Arc::new(formatter));
        }
        let diagnostics = DiagnosticsHook::new(options.diagnostics.clone());
        if diagnostics.is_active() {
            hooks.push(Arc::new(diagnostics));
        }
        if !hooks.is_empty() {
            builder = builder.hooks(hooks);
        }
        if let Some(workspace) = &options.workspace {
            builder = builder.workspace(workspace.clone());
        }
        let agent = builder.build();
        let mut service = Self::with_commands(agent, store, commands);
        service.providers = providers;
        service.workspace = options.workspace;
        service.isolation_default = options.isolation_default;
        service.verify_command = options.verify_command;
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
        // Apply the run-level isolation default when the caller didn't request
        // one (the role's own policy still applies if this default is Never).
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
        // A merged/empty workspace is gone: repoint the session to the base so
        // subsequent turns keep working — and so it no longer reads as isolated.
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
        // Stores persist payloads only today. Reconstruct the turn envelope by
        // replaying from the beginning; timestamps remain synthetic until the
        // store contract grows persisted envelopes.
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

/// The built-in provider ids custom specs may not shadow. `openai` and
/// `deepseek` are deliberately absent: both are OpenAI-compatible endpoints a
/// user can supply credentials for via `/connect <id> <key>`, so a custom spec
/// of either id must resolve (and win over the env built-in) rather than be
/// rejected as a conflict.
const BUILTIN_PROVIDER_IDS: [&str; 4] = [
    ANTHROPIC_PROVIDER_ID,
    GEMINI_PROVIDER_ID,
    COPILOT_PROVIDER_ID,
    OLLAMA_PROVIDER_ID,
];

/// `true` when `id` matches `^[a-z0-9][a-z0-9_-]*$` (which also excludes `/`,
/// the [`ModelRef`] separator).
fn valid_custom_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Build one custom provider plus its default model id.
///
/// Validates the spec (id shape, non-empty base URL and key) so failures are
/// attributed to the custom id instead of surfacing as `openai` errors.
/// Default model precedence: `spec.default_model`, else the first static
/// model, else the OpenAI config default as a documented last resort.
fn build_custom_provider(
    spec: &CustomProviderSpec,
) -> Result<(Arc<dyn agentloop_core::Provider>, String), EngineServiceError> {
    let invalid = |message: &str| EngineServiceError::CustomProviderInvalid {
        id: spec.id.clone(),
        message: message.to_owned(),
    };
    if !valid_custom_id(&spec.id) {
        return Err(invalid(
            "id must match ^[a-z0-9][a-z0-9_-]*$ (lowercase, no `/`)",
        ));
    }
    if spec.base_url.trim().is_empty() {
        return Err(invalid("base_url is empty"));
    }
    // An empty api_key is deliberate: keyless local endpoints (LM Studio,
    // llama.cpp) serve OpenAI-compatible APIs without auth.
    let config = OpenAiConfig::from_values(
        spec.api_key.clone(),
        Some(spec.base_url.clone()),
        spec.default_model.clone(),
    )?;
    let default_model = spec
        .default_model
        .clone()
        .or_else(|| spec.models.first().map(|model| model.id.clone()))
        .unwrap_or_else(|| config.default_model.clone());
    let provider =
        OpenAiProvider::with_identity(spec.id.as_str(), config, spec.models.clone(), spec.thinking);
    Ok((Arc::new(provider), default_model))
}

/// A constructed provider paired with its default model id.
type ProviderWithDefault = (Arc<dyn agentloop_core::Provider>, String);

/// DeepSeek is served over an OpenAI-compatible Chat Completions API, so it's
/// a built-in on top of [`OpenAiProvider`] rather than a bespoke crate.
const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";
// `deepseek-v4-pro` is the current flagship id; the legacy `deepseek-chat` /
// `deepseek-reasoner` names are deprecated (they now just route to v4-flash).
const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-v4-pro";

/// Build the built-in DeepSeek provider from `DEEPSEEK_API_KEY` (optional
/// `DEEPSEEK_MODEL`). Returns `Ok(None)` when the key is unset, so callers
/// auto-register it only when the user has opted in — matching how Ollama is
/// gated. `(provider, default_model)` on success.
///
/// Note: speculative decoding (dSpark) is applied server-side by DeepSeek and
/// is transparent here — there is no request-time knob to set.
fn build_deepseek_from_env() -> Result<Option<ProviderWithDefault>, ProviderError> {
    let Some(api_key) = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let model = std::env::var("DEEPSEEK_MODEL").ok();
    build_deepseek(api_key, model).map(Some)
}

/// Pure builder for the DeepSeek provider (no env access, so it's directly
/// testable). `model` falls back to `DEEPSEEK_DEFAULT_MODEL` (`deepseek-v4-pro`)
/// — passing an explicit model matters because `from_values` otherwise defaults
/// to the OpenAI model (`gpt-4.1-mini`), which is wrong for DeepSeek.
fn build_deepseek(
    api_key: String,
    model: Option<String>,
) -> Result<ProviderWithDefault, ProviderError> {
    let model = model
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEEPSEEK_DEFAULT_MODEL.to_owned());
    let config = OpenAiConfig::from_values(
        api_key,
        Some(DEEPSEEK_BASE_URL.to_owned()),
        Some(model.clone()),
    )?;
    // DeepSeek accepts the DeepSeek-style `thinking` request field (reasoner),
    // which is exactly the capability `OpenAiProvider`'s `thinking` flag gates.
    let provider = OpenAiProvider::with_identity(DEEPSEEK_PROVIDER_ID, config, Vec::new(), true);
    Ok((Arc::new(provider), model))
}

fn resolve_real_providers(
    provider_arg: Option<&str>,
    model_arg: Option<String>,
    custom: &[CustomProviderSpec],
) -> EngineResult<(ProviderRegistry, ModelRef)> {
    let provider_name = match provider_arg {
        Some(provider) => provider,
        None if env_is_set("OPENAI_API_KEY") => OPENAI_PROVIDER_ID,
        None if env_is_set("ANTHROPIC_API_KEY") => ANTHROPIC_PROVIDER_ID,
        None if env_is_set("GEMINI_API_KEY") => GEMINI_PROVIDER_ID,
        None if env_is_set("DEEPSEEK_API_KEY") => DEEPSEEK_PROVIDER_ID,
        None if CopilotConfig::discoverable() => COPILOT_PROVIDER_ID,
        None if env_is_set("OLLAMA_HOST") || env_is_set("OLLAMA_MODEL") => OLLAMA_PROVIDER_ID,
        None => {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from("runtime"),
                hint: "set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, \
                       `DEEPSEEK_API_KEY`, `OLLAMA_HOST`/`OLLAMA_MODEL` for local Ollama, or sign \
                       in to GitHub Copilot (VS Code / Copilot CLI, or set `COPILOT_GITHUB_TOKEN`); \
                       optional model env vars: `OPENAI_MODEL`, `ANTHROPIC_MODEL`, \
                       `GEMINI_MODEL`, `DEEPSEEK_MODEL`, `OLLAMA_MODEL`, `COPILOT_MODEL`"
                    .to_owned(),
            }
            .into());
        }
    };

    match provider_name {
        // A custom `/connect openai …` spec wins over the env built-in: when one
        // exists this arm is skipped and the `other` branch below uses the spec.
        OPENAI_PROVIDER_ID if !custom.iter().any(|spec| spec.id == OPENAI_PROVIDER_ID) => {
            let provider = OpenAiProvider::from_env()?;
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{OPENAI_PROVIDER_ID}/{model}"))))
        }
        ANTHROPIC_PROVIDER_ID => {
            let provider = AnthropicProvider::from_env()?;
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{ANTHROPIC_PROVIDER_ID}/{model}")),
            ))
        }
        GEMINI_PROVIDER_ID => {
            let provider = GeminiProvider::from_env()?;
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{GEMINI_PROVIDER_ID}/{model}"))))
        }
        COPILOT_PROVIDER_ID => {
            let provider = CopilotProvider::from_env()?;
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{COPILOT_PROVIDER_ID}/{model}")),
            ))
        }
        OLLAMA_PROVIDER_ID => {
            let provider = OllamaProvider::from_env();
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{OLLAMA_PROVIDER_ID}/{model}"))))
        }
        other => {
            // A custom spec of the same id wins over a built-in (lets a user's
            // `/connect deepseek …` override the built-in DeepSeek).
            if let Some(spec) = custom.iter().find(|spec| spec.id == other) {
                let (provider, default_model) = build_custom_provider(spec)?;
                let model = model_arg.unwrap_or(default_model);
                let mut providers = ProviderRegistry::new();
                providers.register(provider);
                Ok((providers, ModelRef(format!("{other}/{model}"))))
            } else if other == DEEPSEEK_PROVIDER_ID {
                let (provider, default_model) =
                    build_deepseek_from_env()?.ok_or_else(|| ProviderError::AuthMissing {
                        provider: ProviderId::from(DEEPSEEK_PROVIDER_ID),
                        hint: "set `DEEPSEEK_API_KEY` (optional `DEEPSEEK_MODEL`)".to_owned(),
                    })?;
                let model = model_arg.unwrap_or(default_model);
                let mut providers = ProviderRegistry::new();
                providers.register(provider);
                Ok((providers, ModelRef(format!("{other}/{model}"))))
            } else {
                Err(EngineServiceError::UnsupportedProvider(other.to_owned()))
            }
        }
    }
}

/// Register every provider whose credentials resolve from the environment,
/// in the same precedence order [`resolve_real_providers`] detects them,
/// followed by every `custom` spec in vec order.
///
/// Providers with missing credentials are skipped (debug-traced); any other
/// construction error propagates. Custom specs shadowing a built-in id are
/// rejected with [`EngineServiceError::CustomProviderConflict`]; malformed or
/// duplicate specs with [`EngineServiceError::CustomProviderInvalid`].
/// `preferred` must resolve (it may name a custom id) and becomes the
/// registry priority. The returned [`ModelRef`] is provider-qualified:
/// `model_arg` wins (qualified against the priority provider unless it
/// already names one), else the priority provider's default model.
///
/// No credentials anywhere and no custom provider configured is not an
/// error here: it returns an empty registry and `None` default model,
/// deferring the failure to turn time (`AgentError::Other("no model
/// configured…")`) so a client can open with no provider configured and let
/// the user add one (e.g. via `/connect`) before prompting.
fn resolve_available_providers(
    preferred: Option<&str>,
    model_arg: Option<String>,
    custom: &[CustomProviderSpec],
) -> EngineResult<(ProviderRegistry, Option<ModelRef>)> {
    /// `(provider, its default model)` for a known name; `None` for unknown.
    fn build_provider(name: &str) -> Result<Option<ProviderWithDefault>, ProviderError> {
        fn boxed<P: agentloop_core::Provider + 'static>(
            provider: P,
            default_model: String,
        ) -> Option<ProviderWithDefault> {
            Some((Arc::new(provider), default_model))
        }
        match name {
            OPENAI_PROVIDER_ID => OpenAiProvider::from_env().map(|p| {
                let model = p.default_model().to_owned();
                boxed(p, model)
            }),
            ANTHROPIC_PROVIDER_ID => AnthropicProvider::from_env().map(|p| {
                let model = p.default_model().to_owned();
                boxed(p, model)
            }),
            GEMINI_PROVIDER_ID => GeminiProvider::from_env().map(|p| {
                let model = p.default_model().to_owned();
                boxed(p, model)
            }),
            COPILOT_PROVIDER_ID => CopilotProvider::from_env().map(|p| {
                let model = p.default_model().to_owned();
                boxed(p, model)
            }),
            OLLAMA_PROVIDER_ID => {
                let provider = OllamaProvider::from_env();
                let model = provider.default_model().to_owned();
                Ok(boxed(provider, model))
            }
            _ => Ok(None),
        }
    }

    let mut providers = ProviderRegistry::new();
    let mut defaults: Vec<(ProviderId, String)> = Vec::new();
    let mut register =
        |registry: &mut ProviderRegistry, provider: Arc<dyn agentloop_core::Provider>, model| {
            defaults.push((provider.id(), model));
            registry.register(provider);
        };

    for name in [
        OPENAI_PROVIDER_ID,
        ANTHROPIC_PROVIDER_ID,
        GEMINI_PROVIDER_ID,
        COPILOT_PROVIDER_ID,
    ] {
        // A custom `/connect openai …` spec wins over the env built-in; skip the
        // env registration here so the id is never registered twice (the custom
        // loop below registers it). Mirrors the DeepSeek guard.
        if name == OPENAI_PROVIDER_ID && custom.iter().any(|spec| spec.id == OPENAI_PROVIDER_ID) {
            continue;
        }
        match build_provider(name) {
            Ok(Some((provider, model))) => register(&mut providers, provider, model),
            Ok(None) => {}
            Err(ProviderError::AuthMissing { .. }) => {
                tracing::debug!(target: "engine", provider = name, "skipped: no credentials");
            }
            Err(err) => return Err(err.into()),
        }
    }
    // Ollama's `from_env` is infallible; auto-register only when its env vars
    // opt in, so a dead default endpoint doesn't join the registry unasked.
    if env_is_set("OLLAMA_HOST") || env_is_set("OLLAMA_MODEL") {
        if let Ok(Some((provider, model))) = build_provider(OLLAMA_PROVIDER_ID) {
            register(&mut providers, provider, model);
        }
    }
    // DeepSeek: OpenAI-compatible built-in, env-gated by `DEEPSEEK_API_KEY`.
    // Skip when a custom spec claims the id, so a user's `/connect deepseek …`
    // wins and we never register the id twice.
    if custom.iter().all(|spec| spec.id != DEEPSEEK_PROVIDER_ID) {
        if let Ok(Some((provider, model))) = build_deepseek_from_env() {
            register(&mut providers, provider, model);
        }
    }

    // Client-configured providers come after the built-ins, in vec order.
    // Shadowing a built-in id or repeating a custom id is rejected rather
    // than silently last-wins.
    let mut seen_custom_ids = std::collections::HashSet::new();
    for spec in custom {
        if BUILTIN_PROVIDER_IDS.contains(&spec.id.as_str()) {
            return Err(EngineServiceError::CustomProviderConflict(spec.id.clone()));
        }
        if !seen_custom_ids.insert(spec.id.as_str()) {
            return Err(EngineServiceError::CustomProviderInvalid {
                id: spec.id.clone(),
                message: "declared more than once".to_owned(),
            });
        }
        let (provider, model) = build_custom_provider(spec)?;
        register(&mut providers, provider, model);
    }

    if let Some(name) = preferred {
        let id = ProviderId::from(name);
        if providers.get(&id).is_none() {
            // Not auto-registered: build it explicitly so the caller gets the
            // precise error (or a working provider, e.g. ollama without env).
            match build_provider(name).map_err(EngineServiceError::from)? {
                Some((provider, model)) => register(&mut providers, provider, model),
                None => return Err(EngineServiceError::UnsupportedProvider(name.to_owned())),
            }
        }
        providers.set_priority(vec![id]);
    }

    // A qualified model ref (`provider/model`) names its own provider and
    // needs no priority provider to resolve against; the other two branches
    // do, and simply yield `None` when the registry is empty.
    let default_model = match model_arg {
        Some(model) if model.contains('/') => Some(ModelRef(model)),
        Some(model) => providers
            .ids()
            .first()
            .map(|first| ModelRef(format!("{first}/{model}"))),
        None => providers.ids().first().cloned().map(|first| {
            let model = defaults
                .iter()
                .find(|(id, _)| *id == first)
                .map(|(_, model)| model.clone())
                .unwrap_or_default();
            ModelRef(format!("{first}/{model}"))
        }),
    };

    Ok((providers, default_model))
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

fn env_is_set(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
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

    fn spec(id: &str) -> CustomProviderSpec {
        CustomProviderSpec {
            id: id.to_owned(),
            base_url: "https://example.test/v1".to_owned(),
            api_key: "sk-test".to_owned(),
            default_model: Some("test-chat".to_owned()),
            models: Vec::new(),
            thinking: false,
        }
    }

    fn model_info(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        }
    }

    #[test]
    fn resolve_max_iterations_uses_configured_value_or_falls_back_to_loop_default() {
        assert_eq!(resolve_max_iterations(Some(2_000)), 2_000);
        assert_eq!(
            resolve_max_iterations(None),
            LoopLimits::default().max_iterations
        );
    }

    #[test]
    fn unsupported_provider_is_invalid_request() {
        let err = match resolve_real_providers(Some("mock"), None, &[]) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        let engine_error = err.to_engine_error();
        assert_eq!(engine_error.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn unknown_preferred_provider_is_invalid_request_in_multi_resolver() {
        let err = match resolve_available_providers(Some("mock"), None, &[]) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn qualified_model_arg_passes_through_multi_resolver() {
        // A provider-qualified model names its own provider, so it survives
        // verbatim even with zero providers registered.
        let (_, model) = resolve_available_providers(None, Some("ollama/llama3".to_owned()), &[])
            .expect("qualified model arg never requires a resolvable provider");
        assert_eq!(
            model.expect("qualified model arg yields Some").0,
            "ollama/llama3"
        );
    }

    #[test]
    fn no_providers_and_no_custom_specs_never_errors() {
        // Zero credentials anywhere and no custom provider configured must
        // not be a startup error — the CLI opens with no default model and
        // the user adds a provider later (e.g. via `/connect`). Whether the
        // registry actually ends up empty depends on the ambient
        // environment (a dev shell may export a real provider key), so only
        // the success outcome is asserted unconditionally; the empty-model
        // invariant is checked when the registry happens to be empty.
        let (providers, model) = resolve_available_providers(None, None, &[])
            .expect("no providers configured must not error");
        if providers.ids().is_empty() {
            assert!(model.is_none());
        }
    }

    #[test]
    fn custom_spec_registers_in_multi_resolver() {
        let (providers, _) = match resolve_available_providers(None, None, &[spec("deepseek")]) {
            Ok(resolved) => resolved,
            Err(err) => panic!("custom provider should register: {err}"),
        };
        assert!(
            providers.ids().iter().any(|id| id.as_str() == "deepseek"),
            "registry should contain the custom id: {:?}",
            providers.ids()
        );
    }

    #[test]
    fn deepseek_builtin_has_correct_id_and_default_model() {
        let (provider, model) = build_deepseek("sk-test".to_owned(), None).expect("build");
        assert_eq!(provider.id().as_str(), DEEPSEEK_PROVIDER_ID);
        assert_eq!(model, DEEPSEEK_DEFAULT_MODEL);
    }

    #[test]
    fn deepseek_builtin_honors_model_override_and_ignores_blank() {
        let (_, model) = build_deepseek("sk-test".to_owned(), Some("deepseek-reasoner".to_owned()))
            .expect("build");
        assert_eq!(model, "deepseek-reasoner");
        // A blank override falls back to the default rather than an empty id.
        let (_, model) =
            build_deepseek("sk-test".to_owned(), Some("   ".to_owned())).expect("build");
        assert_eq!(model, DEEPSEEK_DEFAULT_MODEL);
    }

    #[test]
    fn custom_deepseek_does_not_conflict_with_builtin() {
        // "deepseek" is intentionally NOT in BUILTIN_PROVIDER_IDS, so a user's
        // `/connect deepseek …` (a custom spec) resolves rather than erroring
        // with CustomProviderConflict, and registers exactly one `deepseek`.
        let (providers, _) = resolve_available_providers(None, None, &[spec("deepseek")])
            .expect("custom deepseek must resolve without conflict");
        let deepseek_count = providers
            .ids()
            .iter()
            .filter(|id| id.as_str() == DEEPSEEK_PROVIDER_ID)
            .count();
        assert_eq!(
            deepseek_count,
            1,
            "exactly one deepseek: {:?}",
            providers.ids()
        );
    }

    #[test]
    fn preferred_custom_provider_sets_priority_and_default_model() {
        let (providers, model) =
            match resolve_available_providers(Some("deepseek"), None, &[spec("deepseek")]) {
                Ok(resolved) => resolved,
                Err(err) => panic!("preferred custom provider should resolve: {err}"),
            };
        assert_eq!(
            providers.ids().first().map(|id| id.as_str().to_owned()),
            Some("deepseek".to_owned())
        );
        assert_eq!(
            model.expect("preferred provider yields Some").0,
            "deepseek/test-chat"
        );
    }

    #[test]
    fn custom_default_model_falls_back_to_first_static_model() {
        let custom = CustomProviderSpec {
            default_model: None,
            models: vec![model_info("glm-4"), model_info("glm-4-air")],
            ..spec("glm")
        };
        let (_, model) = match resolve_available_providers(Some("glm"), None, &[custom]) {
            Ok(resolved) => resolved,
            Err(err) => panic!("custom provider should resolve: {err}"),
        };
        assert_eq!(
            model.expect("preferred provider yields Some").0,
            "glm/glm-4"
        );
    }

    #[test]
    fn custom_spec_shadowing_a_builtin_is_rejected() {
        let err = match resolve_available_providers(None, None, &[spec("anthropic")]) {
            Ok(_) => panic!("builtin id collision must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(
            &err,
            EngineServiceError::CustomProviderConflict(id) if id == "anthropic"
        ));
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn custom_openai_does_not_conflict_with_builtin() {
        // `openai` is intentionally NOT in BUILTIN_PROVIDER_IDS, so a user's
        // `/connect openai <key>` (a custom spec) resolves rather than erroring
        // with CustomProviderConflict, and registers exactly one `openai` even
        // when OPENAI_API_KEY is also set in the ambient environment.
        let (providers, _) = resolve_available_providers(None, None, &[spec("openai")])
            .expect("custom openai must resolve without conflict");
        let openai_count = providers
            .ids()
            .iter()
            .filter(|id| id.as_str() == OPENAI_PROVIDER_ID)
            .count();
        assert_eq!(openai_count, 1, "exactly one openai: {:?}", providers.ids());
    }

    #[test]
    fn single_provider_resolver_prefers_custom_openai_over_env() {
        // With a custom `openai` spec the env built-in arm is skipped and the
        // spec's endpoint (not api.openai.com) is used, even if OPENAI_API_KEY
        // is set — so this resolves without needing a real key.
        let (providers, model) = resolve_real_providers(Some("openai"), None, &[spec("openai")])
            .expect("custom openai spec must resolve");
        assert_eq!(
            providers
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["openai".to_owned()]
        );
        assert_eq!(model.0, "openai/test-chat");
    }

    #[test]
    fn malformed_custom_id_is_rejected() {
        for bad in ["Deep-Seek", "deep/seek", "", "-deepseek"] {
            let err = match resolve_available_providers(None, None, &[spec(bad)]) {
                Ok(_) => panic!("id `{bad}` must be rejected"),
                Err(err) => err,
            };
            assert!(
                matches!(&err, EngineServiceError::CustomProviderInvalid { id, .. } if id == bad),
                "id `{bad}` should be CustomProviderInvalid, got: {err}"
            );
        }
    }

    #[test]
    fn empty_base_url_rejected_empty_key_allowed() {
        // Keyless local endpoints (LM Studio) register with an empty key.
        let no_key = CustomProviderSpec {
            api_key: "  ".to_owned(),
            ..spec("lmstudio")
        };
        let (registry, _) =
            resolve_available_providers(None, None, &[no_key]).expect("keyless spec registers");
        assert!(registry.ids().iter().any(|id| id.as_str() == "lmstudio"));

        let no_url = CustomProviderSpec {
            base_url: String::new(),
            ..spec("deepseek")
        };
        assert!(matches!(
            resolve_available_providers(None, None, &[no_url]),
            Err(EngineServiceError::CustomProviderInvalid { id, .. }) if id == "deepseek"
        ));
    }

    #[test]
    fn duplicate_custom_ids_are_rejected() {
        let err =
            match resolve_available_providers(None, None, &[spec("deepseek"), spec("deepseek")]) {
                Ok(_) => panic!("duplicate custom ids must be rejected"),
                Err(err) => err,
            };
        assert!(matches!(
            &err,
            EngineServiceError::CustomProviderInvalid { id, .. } if id == "deepseek"
        ));
    }

    #[test]
    fn single_provider_resolver_builds_a_named_custom_spec() {
        let (providers, model) =
            match resolve_real_providers(Some("deepseek"), None, &[spec("deepseek")]) {
                Ok(resolved) => resolved,
                Err(err) => panic!("custom provider should resolve: {err}"),
            };
        assert_eq!(
            providers
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["deepseek".to_owned()]
        );
        assert_eq!(model.0, "deepseek/test-chat");
    }

    // ── workspace isolation lifecycle (EngineService orchestration) ───────────

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
        // Repointed cwd means the session no longer reads as isolated.
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
        let service = EngineService::new(agent, store.clone()); // isolation_default = Never
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
}
