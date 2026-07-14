//! Embeddable SDK — compose providers + the native engine + plugins behind a
//! small builder.
//!
//! ```no_run
//! use agentloop_sdk::AgentBuilder;
//!
//! # fn main() -> agentloop_sdk::EngineResult<()> {
//! let service = AgentBuilder::new()
//!     .provider("anthropic")
//!     .model("claude-sonnet-4")
//!     .enable_plugin("search")
//!     .build()?;
//! # let _ = service;
//! # Ok(())
//! # }
//! ```

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

/// Fluent builder that resolves providers, enables plugins, and composes a
/// native [`EngineService`].
pub struct AgentBuilder {
    provider_opts: ProviderOptions,
    config: EngineConfig,
    plugins: Vec<Arc<dyn Plugin>>,
    all_providers: bool,
    /// Deferred `enable_plugin("search")` so build-time tier routing can pin
    /// the researcher role to a cheap model when one is available.
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

    /// Engine-wide default fallback chain (optionally `provider/`-qualified
    /// entries), used by a session created without its own
    /// `NewSessionParams.fallback_models`. Cross-provider entries need
    /// [`AgentBuilder::all_providers`] so the other provider is registered.
    pub fn fallback_models(mut self, models: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.config.default_fallback_models = models
            .into_iter()
            .map(|m| agentloop_contracts::ModelRef(m.into()))
            .collect();
        self
    }

    /// Working directory the session (and its tools) are sandboxed to.
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.config.cwd = Some(cwd.into());
        self
    }

    /// Run in headless/research mode with no project directory and read-only
    /// tools. The agent can still use the search plugin if enabled.
    pub fn headless(mut self) -> Self {
        self.config.cwd = None;
        self
    }

    /// Set the NDJSON event output verbosity level (default: [`OutputVerbosity::Medium`]).
    pub fn verbosity(mut self, level: OutputVerbosity) -> Self {
        self.config.verbosity = level;
        self
    }

    /// Current date string injected into the system prompt.
    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.config.date = date.into();
        self
    }

    /// Client-configured OpenAI-compatible providers.
    pub fn custom_providers(mut self, custom: Vec<CustomProviderSpec>) -> Self {
        self.provider_opts.custom = custom;
        self
    }

    /// Register every provider whose credentials resolve (instead of a single
    /// preferred one), so provider-qualified models can switch per turn.
    pub fn all_providers(mut self, yes: bool) -> Self {
        self.all_providers = yes;
        self
    }

    /// Enable a built-in plugin by id. Currently recognizes `"search"`,
    /// `"index"`, `"learning"`, and `"verifier"` (when the matching feature
    /// is enabled); unknown or feature-disabled ids are ignored with a
    /// warning. Use [`AgentBuilder::plugin`] for custom plugins.
    pub fn enable_plugin(mut self, id: &str) -> Self {
        let mut matched = false;
        #[cfg(feature = "search")]
        if id == "search" {
            // Deferred until [`AgentBuilder::build`] so tier routing can pin
            // the researcher role to a cheap model when one is registered.
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
        if !matched {
            tracing::warn!(plugin = id, "unknown or feature-disabled plugin ignored");
        }
        self
    }

    /// Add a plugin (the builder wraps it in `Arc` internally).
    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    /// Set the command-execution backend shell tools run through (e.g. an
    /// `agentloop_executors::DockerExecutor`). Default: local host execution.
    pub fn executor(mut self, executor: Arc<dyn agentloop_core::Executor>) -> Self {
        self.config.executor = Some(executor);
        self
    }

    /// Scan tool results with prompt-injection heuristics, fencing flagged
    /// content in an explicit warning before the model reads it.
    pub fn injection_scan(mut self, on: bool) -> Self {
        self.config.injection_scan = on;
        self
    }

    /// Register the `RunWorkflow` tool (a declarative multi-step subagent
    /// pipeline the model can run in one call). Off by default.
    pub fn enable_workflow_tool(mut self, on: bool) -> Self {
        self.config.enable_workflow_tool = on;
        self
    }

    /// Network posture for shell commands (`Denied` requires a backend with
    /// network isolation, e.g. docker).
    pub fn network(mut self, network: agentloop_core::NetworkPolicy) -> Self {
        self.config.network = network;
        self
    }

    /// Set an API key for a built-in provider, bypassing the environment
    /// variable. For example: `.provider_key("deepseek", "sk-...")`.
    pub fn provider_key(mut self, id: impl Into<String>, key: impl Into<String>) -> Self {
        self.provider_opts
            .provider_keys
            .insert(id.into(), key.into());
        self
    }

    /// Set a region override for a region-scoped built-in provider, bypassing
    /// the environment variable. Currently only Bedrock consults this
    /// (`.provider_region("bedrock", "eu-west-1")`); ignored by providers that
    /// have no region concept.
    pub fn provider_region(mut self, id: impl Into<String>, region: impl Into<String>) -> Self {
        self.provider_opts
            .provider_regions
            .insert(id.into(), region.into());
        self
    }

    /// Replace the engine-scoped configuration wholesale (advanced: isolation,
    /// MCP, hooks, roles, session store, …). Any plugins already added via the
    /// builder are preserved and merged in at [`AgentBuilder::build`].
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Mutable access to the engine-scoped configuration for fine-grained
    /// tweaks without replacing it wholesale.
    pub fn config_mut(&mut self) -> &mut EngineConfig {
        &mut self.config
    }

    /// Resolve providers, fold in enabled plugins, and build the service.
    ///
    /// When a known cheap/strong model pair is registered, pins `searcher`
    /// to the cheap model and `worker` to the strong one (see
    /// [`apply_research_model_tiers`]). Explicit `EngineConfig.roles` entries
    /// for those names win. When search is enabled via
    /// [`AgentBuilder::enable_plugin`], the `researcher` role is also pinned
    /// to the cheap model (independent of searcher/worker overrides). Desktop
    /// compose goes through this path automatically.
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

    /// Desktop `compose.rs` calls `enable_plugin("index")` when
    /// `prefs.plugins.index` is true; this locks the tool names that call
    /// must surface (`SearchCode` / `FindSymbol` / `RepoMap`).
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
