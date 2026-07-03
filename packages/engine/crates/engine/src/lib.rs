//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and real-provider native-loop
//! composition. It intentionally does not expose provider wire types or install
//! mock providers at runtime.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, Answer, CompactionSummary, EngineError, ErrorCode, Hello, ModelInfo, ModelRef,
    NewSessionParams, PermissionDecision, PermissionMode, PermissionRequestId, PromptInput,
    ProviderId, QuestionId, SessionEvent, SessionId, SessionMeta, Transcript, TurnId, TurnOptions,
    TurnSummary, now_ms, reduce,
};
use agentloop_core::{
    Agent, AgentError, EventStream, ProviderError, ProviderRegistry, SessionStore, StoreError,
};
use agentloop_loop::NativeAgentBuilder;
pub use agentloop_loop::roles::{RoleError, RoleRegistry, RoleSpec, RoleToolProfile, valid_name};
use agentloop_mcp::{McpBridgeConfig, McpBridgeError, McpManager};
use agentloop_prompts::{
    CommandDiscoveryConfig, CommandError, CommandRegistry, PromptError, SystemPromptAssembler,
    SystemPromptConfig, Vars,
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
    Mcp(#[from] McpBridgeError),
    #[error(transparent)]
    Role(#[from] RoleError),
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
            Self::Mcp(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Role(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
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
}

impl EngineService {
    pub fn new(agent: Arc<dyn Agent>, store: Arc<dyn SessionStore>) -> Self {
        Self {
            agent,
            store,
            commands: Arc::new(CommandRegistry::builtins()),
            providers: ProviderRegistry::new(),
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
        }
    }

    /// Native loop with a single provider resolved from `options.provider` or
    /// the environment.
    pub fn native(mut options: EngineOptions) -> EngineResult<Self> {
        let model = options.model.take();
        let (providers, default_model) =
            resolve_real_providers(options.provider.as_deref(), model, &options.custom)?;
        Self::build_native(providers, default_model, options)
    }

    /// Native loop with every provider whose credentials resolve, so
    /// provider-qualified [`ModelRef`]s can switch providers per turn.
    ///
    /// `options.provider` names the preferred provider (it must resolve and
    /// becomes the priority for bare model refs); `options.model` picks the
    /// default model, qualified against the preferred provider unless it
    /// already carries a `provider/` prefix.
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
        default_model: ModelRef,
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
        let mut builder = NativeAgentBuilder::new(store.clone())
            .providers(providers.clone())
            .tools(tools)
            .questions(pending_questions)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .default_model(default_model)
            .roles(options.roles.clone());
        if let Some(manager) = mcp_manager {
            builder = builder.mcp(manager);
        }
        let agent = builder.build();
        let mut service = Self::with_commands(agent, store, commands);
        service.providers = providers;
        Ok(service)
    }

    pub fn hello(&self) -> Hello {
        let mut caps = self.agent.capabilities();
        if caps.commands.is_empty() {
            caps.commands = self.commands.infos();
        }
        Hello::new(caps)
    }

    pub async fn create_session(&self, params: NewSessionParams) -> EngineResult<SessionId> {
        Ok(self.agent.create_session(params).await?)
    }

    pub async fn resume_session(&self, id: &SessionId) -> EngineResult<()> {
        Ok(self.agent.resume_session(id).await?)
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

/// The built-in provider ids custom specs may not shadow.
const BUILTIN_PROVIDER_IDS: [&str; 5] = [
    OPENAI_PROVIDER_ID,
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
        None if CopilotConfig::discoverable() => COPILOT_PROVIDER_ID,
        None if env_is_set("OLLAMA_HOST") || env_is_set("OLLAMA_MODEL") => OLLAMA_PROVIDER_ID,
        None => {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from("runtime"),
                hint: "set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, \
                       `OLLAMA_HOST`/`OLLAMA_MODEL` for local Ollama, or sign in to GitHub \
                       Copilot (VS Code / Copilot CLI, or set `COPILOT_GITHUB_TOKEN`); \
                       optional model env vars: `OPENAI_MODEL`, `ANTHROPIC_MODEL`, \
                       `GEMINI_MODEL`, `OLLAMA_MODEL`, `COPILOT_MODEL`"
                    .to_owned(),
            }
            .into());
        }
    };

    match provider_name {
        OPENAI_PROVIDER_ID => {
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
            if let Some(spec) = custom.iter().find(|spec| spec.id == other) {
                let (provider, default_model) = build_custom_provider(spec)?;
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
/// registry priority. The returned [`ModelRef`] is always provider-qualified:
/// `model_arg` wins (qualified against the priority provider unless it
/// already names one), else the priority provider's default model.
fn resolve_available_providers(
    preferred: Option<&str>,
    model_arg: Option<String>,
    custom: &[CustomProviderSpec],
) -> EngineResult<(ProviderRegistry, ModelRef)> {
    /// A constructed provider paired with its default model id.
    type ProviderWithDefault = (Arc<dyn agentloop_core::Provider>, String);

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

    let Some(first) = providers.ids().first().cloned() else {
        return Err(ProviderError::AuthMissing {
            provider: ProviderId::from("runtime"),
            hint: "set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, \
                   `OLLAMA_HOST`/`OLLAMA_MODEL` for local Ollama, or sign in to GitHub \
                   Copilot (VS Code / Copilot CLI, or set `COPILOT_GITHUB_TOKEN`); \
                   optional model env vars: `OPENAI_MODEL`, `ANTHROPIC_MODEL`, \
                   `GEMINI_MODEL`, `OLLAMA_MODEL`, `COPILOT_MODEL`"
                .to_owned(),
        }
        .into());
    };

    let default_model = match model_arg {
        Some(model) if model.contains('/') => ModelRef(model),
        Some(model) => ModelRef(format!("{first}/{model}")),
        None => {
            let model = defaults
                .iter()
                .find(|(id, _)| *id == first)
                .map(|(_, model)| model.clone())
                .unwrap_or_default();
            ModelRef(format!("{first}/{model}"))
        }
    };

    Ok((providers, default_model))
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
        // Whatever providers the environment yields, an explicit
        // provider-qualified model must survive verbatim.
        if let Ok((_, model)) =
            resolve_available_providers(None, Some("ollama/llama3".to_owned()), &[])
        {
            assert_eq!(model.0, "ollama/llama3");
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
        assert_eq!(model.0, "deepseek/test-chat");
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
        assert_eq!(model.0, "glm/glm-4");
    }

    #[test]
    fn custom_spec_shadowing_a_builtin_is_rejected() {
        let err = match resolve_available_providers(None, None, &[spec("openai")]) {
            Ok(_) => panic!("builtin id collision must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(
            &err,
            EngineServiceError::CustomProviderConflict(id) if id == "openai"
        ));
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
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
}
