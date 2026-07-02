//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and real-provider native-loop
//! composition. It intentionally does not expose provider wire types or install
//! mock providers at runtime.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, Answer, CompactionSummary, EngineError, ErrorCode, Hello, ModelRef,
    NewSessionParams, PermissionDecision, PermissionRequestId, PromptInput, ProviderId, QuestionId,
    SessionEvent, SessionId, SessionMeta, Transcript, TurnId, TurnOptions, TurnSummary, now_ms,
    reduce,
};
use agentloop_core::{
    Agent, AgentError, EventStream, ProviderError, ProviderRegistry, SessionStore, StoreError,
};
use agentloop_loop::NativeAgentBuilder;
use agentloop_prompts::{
    CommandDiscoveryConfig, CommandError, CommandRegistry, PromptError, SystemPromptAssembler,
    SystemPromptConfig, Vars,
};
use agentloop_provider_anthropic::{ANTHROPIC_PROVIDER_ID, AnthropicProvider};
use agentloop_provider_copilot::{COPILOT_PROVIDER_ID, CopilotConfig, CopilotProvider};
use agentloop_provider_gemini::{GEMINI_PROVIDER_ID, GeminiProvider};
use agentloop_provider_ollama::{OLLAMA_PROVIDER_ID, OllamaProvider};
use agentloop_provider_openai::{OPENAI_PROVIDER_ID, OpenAiProvider};
use agentloop_session::MemoryStore;
use agentloop_tools::BaseTools;

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cwd: PathBuf,
    pub date: String,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            cwd: PathBuf::from("."),
            date: String::new(),
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
    #[error(
        "provider `{0}` is not available in this build; supported runtime providers: `openai`, `anthropic`, `gemini`, `ollama`"
    )]
    UnsupportedProvider(String),
}

impl EngineServiceError {
    pub fn to_engine_error(&self) -> EngineError {
        match self {
            Self::Agent(err) => err.to_engine_error(),
            Self::Provider(err) => err.to_engine_error(),
            Self::Store(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::Prompt(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Command(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::UnsupportedProvider(_) => {
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
            resolve_real_providers(options.provider.as_deref(), model)?;
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
            resolve_available_providers(options.provider.as_deref(), model)?;
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
        options: EngineOptions,
    ) -> EngineResult<Self> {
        let BaseTools {
            registry: tools,
            pending_questions,
            ..
        } = agentloop_tools::base_tools();
        let system_prompt =
            SystemPromptAssembler::new(SystemPromptConfig::default()).assemble(&Vars {
                cwd: options.cwd.display().to_string(),
                date: options.date,
            })?;
        let commands = CommandRegistry::discover(CommandDiscoveryConfig {
            user_dir: default_user_command_dir(),
            project_dir: Some(options.cwd.join(".agent").join("commands")),
        })?;

        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let agent = NativeAgentBuilder::new(store.clone())
            .providers(providers.clone())
            .tools(tools)
            .questions(pending_questions)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .default_model(default_model)
            .build();
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

fn resolve_real_providers(
    provider_arg: Option<&str>,
    model_arg: Option<String>,
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
        other => Err(EngineServiceError::UnsupportedProvider(other.to_owned())),
    }
}

/// Register every provider whose credentials resolve from the environment,
/// in the same precedence order [`resolve_real_providers`] detects them.
///
/// Providers with missing credentials are skipped (debug-traced); any other
/// construction error propagates. `preferred` must resolve and becomes the
/// registry priority. The returned [`ModelRef`] is always provider-qualified:
/// `model_arg` wins (qualified against the priority provider unless it
/// already names one), else the priority provider's default model.
fn resolve_available_providers(
    preferred: Option<&str>,
    model_arg: Option<String>,
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

    #[test]
    fn unsupported_provider_is_invalid_request() {
        let err = match resolve_real_providers(Some("mock"), None) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        let engine_error = err.to_engine_error();
        assert_eq!(engine_error.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn unknown_preferred_provider_is_invalid_request_in_multi_resolver() {
        let err = match resolve_available_providers(Some("mock"), None) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn qualified_model_arg_passes_through_multi_resolver() {
        // Whatever providers the environment yields, an explicit
        // provider-qualified model must survive verbatim.
        if let Ok((_, model)) = resolve_available_providers(None, Some("ollama/llama3".to_owned()))
        {
            assert_eq!(model.0, "ollama/llama3");
        }
    }
}
