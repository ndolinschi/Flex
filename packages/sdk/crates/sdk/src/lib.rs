mod loop_agent;
pub mod mcp_store;
mod role_tiers;
pub mod routines;

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_core::Plugin;
use agentloop_providers::{
    ProviderOptions, connect_bedrock, resolve_available_providers, resolve_real_providers,
};

#[cfg(feature = "artifacts")]
pub use agentloop_artifacts::{self as artifacts, ArtifactsPlugin};
pub use agentloop_engine::{
    EngineConfig, EngineResult, EngineService, EngineServiceError, OutputVerbosity, RoleSpec,
    RoleToolProfile,
};
#[cfg(feature = "index")]
pub use agentloop_index::{self as index, IndexPlugin};
#[cfg(feature = "learning")]
pub use agentloop_learning::{self as learning, LearningPlugin};
pub use agentloop_mcp::{
    self as mcp, McpBridgeConfig, McpRemoteTool, McpServerConfig, McpServerTransport,
    StdioServerConfig, StreamableHttpConfig,
};
pub use agentloop_providers::{self as providers, CustomProviderSpec};
#[cfg(feature = "search")]
pub use agentloop_search::{self as search, SearchPlugin};
#[cfg(feature = "verifier")]
pub use agentloop_verifier::{self as verifier, VerifierPlugin};
pub use loop_agent::{ClawBot, ClawBotBuilder, LoopAgent, PromptSource};
pub use role_tiers::apply_research_model_tiers;

pub struct AgentBuilder {
    provider_opts: ProviderOptions,
    config: EngineConfig,
    plugins: Vec<Arc<dyn Plugin>>,
    all_providers: bool,

    #[cfg(feature = "search")]
    enable_search: bool,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            provider_opts: ProviderOptions::default(),
            config: EngineConfig::default(),
            plugins: Vec::new(),
            all_providers: false,
            #[cfg(feature = "search")]
            enable_search: false,
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

    pub fn fallback_models(mut self, models: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.config.default_fallback_models = models
            .into_iter()
            .map(|m| agentloop_contracts::ModelRef(m.into()))
            .collect();
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.config.cwd = Some(cwd.into());
        self
    }

    pub fn headless(mut self) -> Self {
        self.config.cwd = None;
        self
    }

    pub fn verbosity(mut self, level: OutputVerbosity) -> Self {
        self.config.verbosity = level;
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.config.date = date.into();
        self
    }

    pub fn custom_providers(mut self, custom: Vec<CustomProviderSpec>) -> Self {
        self.provider_opts.custom = custom;
        self
    }

    pub fn all_providers(mut self, yes: bool) -> Self {
        self.all_providers = yes;
        self
    }

    pub fn enable_plugin(mut self, id: &str) -> Self {
        let mut matched = false;
        #[cfg(feature = "search")]
        if id == "search" {
            self.enable_search = true;
            matched = true;
        }
        #[cfg(feature = "index")]
        if id == "index" {
            self.plugins.push(Arc::new(IndexPlugin::new()));
            matched = true;
        }
        #[cfg(feature = "learning")]
        if id == "learning" {
            if let Some(plugin) = LearningPlugin::with_default_dir() {
                self.plugins.push(Arc::new(plugin));
            }
            matched = true;
        }
        #[cfg(feature = "verifier")]
        if id == "verifier" {
            self.plugins.push(Arc::new(VerifierPlugin));
            matched = true;
        }
        #[cfg(feature = "artifacts")]
        if id == "artifacts" {
            self.plugins.push(Arc::new(ArtifactsPlugin::default()));
            matched = true;
        }
        if id == "messaging" {
            self.config.enable_peer_messaging = true;
            self.config.enable_switch_mode = true;
            matched = true;
        }
        if !matched {
            tracing::warn!(plugin = id, "unknown or feature-disabled plugin ignored");
        }
        self
    }

    pub fn peer_messaging(mut self, on: bool) -> Self {
        self.config.enable_peer_messaging = on;
        self
    }

    pub fn switch_mode(mut self, on: bool) -> Self {
        self.config.enable_switch_mode = on;
        self
    }

    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    pub fn executor(mut self, executor: Arc<dyn agentloop_core::Executor>) -> Self {
        self.config.executor = Some(executor);
        self
    }

    pub fn injection_scan(mut self, on: bool) -> Self {
        self.config.injection_scan = on;
        self
    }

    pub fn enable_workflow_tool(mut self, on: bool) -> Self {
        self.config.enable_workflow_tool = on;
        self
    }

    pub fn network(mut self, network: agentloop_core::NetworkPolicy) -> Self {
        self.config.network = network;
        self
    }

    pub fn provider_key(mut self, id: impl Into<String>, key: impl Into<String>) -> Self {
        self.provider_opts
            .provider_keys
            .insert(id.into(), key.into());
        self
    }

    pub fn provider_region(mut self, id: impl Into<String>, region: impl Into<String>) -> Self {
        self.provider_opts
            .provider_regions
            .insert(id.into(), region.into());
        self
    }

    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    pub fn config_mut(&mut self) -> &mut EngineConfig {
        &mut self.config
    }

    pub fn build(mut self) -> EngineResult<EngineService> {
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

        let cheap = apply_research_model_tiers(
            &providers,
            &mut self.config,
            self.provider_opts.provider.as_deref(),
        );

        #[cfg(feature = "search")]
        if self.enable_search {
            let already_has_search = self
                .plugins
                .iter()
                .chain(self.config.plugins.iter())
                .any(|plugin| plugin.id() == "search");
            if !already_has_search {
                let mut plugin = SearchPlugin::default();
                if let Some(model) = cheap {
                    plugin = plugin.with_researcher_models(vec![model]);
                }
                self.plugins.push(Arc::new(plugin));
            } else if cheap.is_some() {
                tracing::warn!(
                    "enable_plugin(\"search\") skipped: a search plugin is already registered; \
                     call SearchPlugin::with_researcher_models yourself to pin the researcher role"
                );
            }
        }

        self.config.plugins.extend(self.plugins);
        EngineService::native(providers, default_model, self.config)
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "index"))]
mod index_wiring_tests {
    use super::*;
    use agentloop_core::Plugin;

    #[test]
    fn enable_plugin_index_registers_search_code_and_find_symbol() {
        let _builder = AgentBuilder::new().enable_plugin("index");
        let plugin: Arc<dyn Plugin> = Arc::new(IndexPlugin::new().with_auto_context(false));
        let names: Vec<String> = plugin.tools().iter().map(|t| t.descriptor().name).collect();
        assert!(names.contains(&"SearchCode".to_owned()), "{names:?}");
        assert!(names.contains(&"FindSymbol".to_owned()), "{names:?}");
        assert!(names.contains(&"RepoMap".to_owned()), "{names:?}");
        assert_eq!(plugin.id(), "index");
    }
}

#[cfg(all(test, feature = "search"))]
mod search_wiring_tests {
    use super::*;
    use agentloop_core::Plugin;

    #[test]
    fn enable_plugin_search_registers_search_tools() {
        let _builder = AgentBuilder::new().enable_plugin("search");
        let plugin: Arc<dyn Plugin> = Arc::new(SearchPlugin::default());
        let names: Vec<String> = plugin.tools().iter().map(|t| t.descriptor().name).collect();
        assert!(names.contains(&"search_web".to_owned()), "{names:?}");
        assert!(names.contains(&"scrape_page".to_owned()), "{names:?}");
        assert_eq!(plugin.id(), "search");
    }
}

#[cfg(test)]
mod enable_plugin_tests {
    use super::*;

    #[test]
    fn enable_plugin_unknown_id_does_not_panic() {
        let _builder = AgentBuilder::new().enable_plugin("not-a-real-plugin");
    }
}
