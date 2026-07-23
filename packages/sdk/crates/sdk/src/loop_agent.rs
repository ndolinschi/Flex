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

pub type PromptSource<'a> =
    Box<dyn FnMut(&[TurnSummary]) -> BoxFuture<'a, Option<PromptInput>> + Send + 'a>;

#[async_trait]
pub trait LoopAgent: Send + Sync {
    async fn step(&self, input: PromptInput) -> EngineResult<TurnSummary>;

    async fn run_loop(&self, next_prompt: PromptSource<'_>) -> EngineResult<Vec<TurnSummary>>;

    fn session_id(&self) -> &SessionId;

    fn memory_dir(&self) -> Option<&Path>;

    async fn cancel(&self) -> EngineResult<()>;
}

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

    pub fn provider(mut self, id: impl Into<String>) -> Self {
        self.provider_opts.provider = Some(id.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.provider_opts.model = Some(model.into());
        self
    }

    pub fn all_providers(mut self, yes: bool) -> Self {
        self.all_providers = yes;
        self
    }

    pub fn tools<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools = names.into_iter().map(Into::into).collect();
        self
    }

    pub fn role_name(mut self, name: impl Into<String>) -> Self {
        self.role_name = name.into();
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn turn_timeout(mut self, timeout: Duration) -> Self {
        self.turn_timeout = Some(timeout);
        self
    }

    pub fn loop_timeout(mut self, timeout: Duration) -> Self {
        self.loop_timeout = Some(timeout);
        self
    }

    pub fn memory_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.memory_dir = Some(dir.into());
        self
    }

    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

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

pub struct ClawBot {
    service: EngineService,
    session: SessionId,
    turn_timeout: Option<Duration>,
    loop_timeout: Option<Duration>,
    memory_dir: Option<PathBuf>,
}

impl ClawBot {
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
