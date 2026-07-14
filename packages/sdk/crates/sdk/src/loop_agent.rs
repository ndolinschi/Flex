//! Loop agents: long-lived bots pinned to a directory, driven turn by turn.
//!
//! [`LoopAgent`] is the contract; [`ClawBot`] is the native implementation —
//! a root session with a dedicated role (tool allowlist), an optional
//! persistent-memory directory, a per-turn timeout, and an overall wall-clock
//! budget for [`LoopAgent::run_loop`].
//!
//! ```no_run
//! use std::time::Duration;
//! use agentloop_sdk::{ClawBotBuilder, LoopAgent};
//! use agentloop_contracts::PromptInput;
//!
//! # async fn demo() -> agentloop_sdk::EngineResult<()> {
//! let bot = ClawBotBuilder::new("/path/to/project")
//!     .provider("anthropic")
//!     .tools(["Read", "Grep", "Glob"])
//!     .memory_dir("/path/to/project/.bot-memory")
//!     .turn_timeout(Duration::from_secs(120))
//!     .loop_timeout(Duration::from_secs(600))
//!     .build()
//!     .await?;
//! let summary = bot.step(PromptInput::text("Summarize the repo layout.")).await?;
//! # let _ = summary;
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::BoxFuture;

use agentloop_contracts::{NewSessionParams, PromptInput, SessionId, TurnOptions, TurnSummary};
use agentloop_core::Plugin;
use agentloop_engine::{EngineConfig, EngineResult, EngineService, RoleSpec, RoleToolProfile};
use agentloop_providers::{
    ProviderOptions, connect_bedrock, resolve_available_providers, resolve_real_providers,
};

use crate::role_tiers::apply_research_model_tiers;

/// Yields the next prompt for a loop iteration, given the summaries of the
/// turns completed so far this `run_loop` call. Return `None` to stop.
pub type PromptSource<'a> =
    Box<dyn FnMut(&[TurnSummary]) -> BoxFuture<'a, Option<PromptInput>> + Send + 'a>;

/// A long-lived agent driven in a loop: one session, repeated turns, bounded
/// by per-turn and overall wall-clock budgets.
#[async_trait]
pub trait LoopAgent: Send + Sync {
    /// Run one turn now, respecting the per-turn timeout. Does not consume
    /// the overall loop budget (only [`LoopAgent::run_loop`] enforces it).
    async fn step(&self, input: PromptInput) -> EngineResult<TurnSummary>;

    /// Drive turns from `next_prompt` until it returns `None` or the overall
    /// loop timeout elapses (an in-flight turn is cancelled at the deadline).
    /// Returns the summaries of every completed turn.
    async fn run_loop(&self, next_prompt: PromptSource<'_>) -> EngineResult<Vec<TurnSummary>>;

    /// The underlying session id.
    fn session_id(&self) -> &SessionId;

    /// The persistent-memory directory, when one was configured.
    fn memory_dir(&self) -> Option<&Path>;

    /// Cancel the in-flight turn, if any.
    async fn cancel(&self) -> EngineResult<()>;
}

/// Builder for [`ClawBot`]: a loop agent pinned to `cwd` with a restricted
/// tool allowlist and optional persistent memory.
pub struct ClawBotBuilder {
    provider_opts: ProviderOptions,
    config: EngineConfig,
    plugins: Vec<Arc<dyn Plugin>>,
    all_providers: bool,
    cwd: PathBuf,
    tools: Vec<String>,
    role_name: String,
    system_prompt: Option<String>,
    turn_timeout: Option<Duration>,
    loop_timeout: Option<Duration>,
    memory_dir: Option<PathBuf>,
}

impl ClawBotBuilder {
    /// A bot pinned to `cwd` — the directory every path-taking tool is
    /// sandboxed to.
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            provider_opts: ProviderOptions::default(),
            config: EngineConfig::default(),
            plugins: Vec::new(),
            all_providers: false,
            cwd: cwd.into(),
            tools: Vec::new(),
            role_name: "clawbot".to_owned(),
            system_prompt: None,
            turn_timeout: None,
            loop_timeout: None,
            memory_dir: None,
        }
    }

    /// Preferred provider id (else auto-detected from the environment).
    pub fn provider(mut self, id: impl Into<String>) -> Self {
        self.provider_opts.provider = Some(id.into());
        self
    }

    /// Default model id (optionally `provider/`-qualified).
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.provider_opts.model = Some(model.into());
        self
    }

    /// Register every provider whose credentials resolve.
    pub fn all_providers(mut self, yes: bool) -> Self {
        self.all_providers = yes;
        self
    }

    /// Explicit tool allowlist for the bot (e.g. `["Read", "Grep"]`). Empty
    /// (the default) grants the full registry.
    pub fn tools<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools = names.into_iter().map(Into::into).collect();
        self
    }

    /// Role name registered for the bot (default `clawbot`); override when it
    /// would collide with a user-configured role.
    pub fn role_name(mut self, name: impl Into<String>) -> Self {
        self.role_name = name.into();
        self
    }

    /// System-prompt text appended for the bot's role.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Hard wall-clock budget per turn; the turn is cancelled when it elapses.
    pub fn turn_timeout(mut self, timeout: Duration) -> Self {
        self.turn_timeout = Some(timeout);
        self
    }

    /// Overall wall-clock budget for each [`LoopAgent::run_loop`] call.
    pub fn loop_timeout(mut self, timeout: Duration) -> Self {
        self.loop_timeout = Some(timeout);
        self
    }

    /// Directory where the bot persists durable memory notes across sessions
    /// (enables the learning plugin's `MemoryWrite` tool; requires the
    /// `learning` feature).
    pub fn memory_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.memory_dir = Some(dir.into());
        self
    }

    /// Add a plugin (the builder wraps it in `Arc` internally).
    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    /// Replace the engine-scoped configuration wholesale (advanced). The
    /// builder still overrides `cwd` and appends the bot's role at build.
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Resolve providers, register the bot role, create the pinned session.
    pub async fn build(mut self) -> EngineResult<ClawBot> {
        let tools = if self.tools.is_empty() {
            RoleToolProfile::Full
        } else {
            RoleToolProfile::Allow(self.tools)
        };
        self.config.roles.push(RoleSpec {
            tools,
            prompt: self.system_prompt,
            ..RoleSpec::new(self.role_name.clone())
        });
        self.config.cwd = Some(self.cwd.clone());

        #[cfg(feature = "learning")]
        if let Some(dir) = &self.memory_dir {
            let plugin =
                agentloop_learning::LearningPlugin::with_default_dir().ok_or_else(|| {
                    agentloop_core::AgentError::Other(
                        "memory_dir requires a resolvable home directory".to_owned(),
                    )
                })?;
            self.plugins
                .push(Arc::new(plugin.with_memory_dir(Some(dir.clone()))));
        }
        #[cfg(not(feature = "learning"))]
        if self.memory_dir.is_some() {
            return Err(agentloop_core::AgentError::Other(
                "memory_dir requires the `learning` feature".to_owned(),
            )
            .into());
        }

        self.config.plugins.extend(self.plugins);

        let (mut providers, mut default_model) = if self.all_providers {
            resolve_available_providers(
                self.provider_opts.provider.as_deref(),
                self.provider_opts.model.clone(),
                &self.provider_opts.custom,
                &self.provider_opts.provider_keys,
            )?
        } else {
            let (providers, model) = resolve_real_providers(
                self.provider_opts.provider.as_deref(),
                self.provider_opts.model.clone(),
                &self.provider_opts.custom,
                &self.provider_opts.provider_keys,
            )?;
            (providers, Some(model))
        };
        if let Some(bedrock_model) = connect_bedrock(
            &mut providers,
            &self.provider_opts.provider_keys,
            &self.provider_opts.provider_regions,
        ) {
            if self.all_providers {
                default_model = default_model.or(Some(bedrock_model));
            }
        }
        let _ = apply_research_model_tiers(
            &providers,
            &mut self.config,
            self.provider_opts.provider.as_deref(),
        );
        let service = EngineService::native(providers, default_model, self.config)?;

        let session = service
            .create_session(NewSessionParams {
                cwd: Some(self.cwd),
                role: Some(self.role_name),
                ..Default::default()
            })
            .await?;

        Ok(ClawBot {
            service,
            session,
            turn_timeout: self.turn_timeout,
            loop_timeout: self.loop_timeout,
            memory_dir: self.memory_dir,
        })
    }
}

/// The native [`LoopAgent`]: one root session with a dedicated role, driven
/// through [`EngineService`].
pub struct ClawBot {
    service: EngineService,
    session: SessionId,
    turn_timeout: Option<Duration>,
    loop_timeout: Option<Duration>,
    memory_dir: Option<PathBuf>,
}

impl ClawBot {
    /// The composed engine service, for event subscription or advanced calls.
    pub fn service(&self) -> &EngineService {
        &self.service
    }

    fn turn_options(&self) -> TurnOptions {
        TurnOptions {
            turn_timeout_ms: self
                .turn_timeout
                .map(|t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
            ..Default::default()
        }
    }
}

#[async_trait]
impl LoopAgent for ClawBot {
    async fn step(&self, input: PromptInput) -> EngineResult<TurnSummary> {
        self.service
            .prompt(&self.session, input, self.turn_options())
            .await
    }

    async fn run_loop(&self, mut next_prompt: PromptSource<'_>) -> EngineResult<Vec<TurnSummary>> {
        let deadline = self.loop_timeout.map(|t| tokio::time::Instant::now() + t);
        let mut summaries = Vec::new();
        loop {
            if let Some(deadline) = deadline {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
            }
            let Some(input) = next_prompt(&summaries).await else {
                break;
            };
            let turn = self.step(input);
            match deadline {
                Some(deadline) => {
                    tokio::pin!(turn);
                    tokio::select! {
                        result = &mut turn => summaries.push(result?),
                        _ = tokio::time::sleep_until(deadline) => {
                            // Deadline hit mid-turn: cancel, then let the turn
                            // wind down gracefully (cancellation is not an
                            // error; the summary reports `Cancelled`).
                            self.service.cancel(&self.session).await?;
                            summaries.push(turn.await?);
                            break;
                        }
                    }
                }
                None => summaries.push(turn.await?),
            }
        }
        Ok(summaries)
    }

    fn session_id(&self) -> &SessionId {
        &self.session
    }

    fn memory_dir(&self) -> Option<&Path> {
        self.memory_dir.as_deref()
    }

    async fn cancel(&self) -> EngineResult<()> {
        self.service.cancel(&self.session).await
    }
}
