//! Engine service front door.
//!
//! This crate owns the ready runtime boundary above concrete agents: handshake,
//! session operations, replay/materialized items, and real-provider native-loop
//! composition. It intentionally does not expose provider wire types or install
//! mock providers at runtime.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, Answer, EngineError, ErrorCode, Hello, ModelRef, NewSessionParams,
    PermissionDecision, PermissionRequestId, PromptInput, ProviderId, QuestionId, SessionEvent,
    SessionId, SessionMeta, Transcript, TurnId, TurnOptions, TurnSummary, now_ms, reduce,
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
}

impl EngineService {
    pub fn new(agent: Arc<dyn Agent>, store: Arc<dyn SessionStore>) -> Self {
        Self {
            agent,
            store,
            commands: Arc::new(CommandRegistry::builtins()),
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
        }
    }

    pub fn native(options: EngineOptions) -> EngineResult<Self> {
        let (providers, default_model) =
            resolve_real_providers(options.provider.as_deref(), options.model)?;
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
            .providers(providers)
            .tools(tools)
            .questions(pending_questions)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .default_model(default_model)
            .build();
        Ok(Self::with_commands(agent, store, commands))
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
        None if env_is_set("OLLAMA_HOST") || env_is_set("OLLAMA_MODEL") => OLLAMA_PROVIDER_ID,
        None => {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from("runtime"),
                hint: "set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, or \
                       `OLLAMA_HOST`/`OLLAMA_MODEL` for local Ollama (optional model env vars: \
                       `OPENAI_MODEL`, `ANTHROPIC_MODEL`, `GEMINI_MODEL`, `OLLAMA_MODEL`)"
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
}
