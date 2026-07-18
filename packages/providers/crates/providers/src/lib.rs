//! Provider facade — the connector umbrella and composition helper.
//!
//! This crate is where provider *selection and construction* live (moved out of
//! the provider-agnostic engine). It resolves which built-in LLM providers have
//! credentials, builds them (plus client-configured OpenAI-compatible specs and
//! first-party keys like Bedrock), picks a default model, and hands the
//! resulting [`ProviderRegistry`] to the engine via [`agentloop_engine::EngineService::native`].
//!
//! It also re-exports the external-agent connectors (the former delegators,
//! still `Agent` impls) so a composition root can reach every connector through
//! a single dependency.

use std::collections::BTreeMap;

use agentloop_contracts::ModelInfo;

mod resolve;

pub use resolve::{connect_bedrock, resolve_available_providers, resolve_real_providers};

pub use agentloop_engine::{EngineConfig, EngineResult, EngineService, EngineServiceError};

pub use agentloop_provider_anthropic as anthropic;
pub use agentloop_provider_bedrock as bedrock;
pub use agentloop_provider_chatgpt as chatgpt;
pub use agentloop_provider_copilot as copilot;
pub use agentloop_provider_gemini as gemini;
pub use agentloop_provider_ollama as ollama;
pub use agentloop_provider_openai as openai;

pub use agentloop_delegator_acp as delegator_acp;
pub use agentloop_delegator_claude_code as delegator_claude_code;
pub use agentloop_delegator_copilot as delegator_copilot;
pub use agentloop_delegator_cursor as delegator_cursor;
pub use agentloop_delegator_grok as delegator_grok;
pub use agentloop_delegator_opencode as delegator_opencode;

/// One client-configured OpenAI-compatible provider, registered alongside the
/// built-in providers under its own id.
#[derive(Debug, Clone)]
pub struct CustomProviderSpec {
    /// Registry id; must match `^[a-z0-9][a-z0-9_-]*$` (no `/`, which is the
    /// [`agentloop_contracts::ModelRef`] separator) and must not collide with a
    /// built-in id.
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

/// Provider-scoped inputs that select and configure the providers a native
/// [`EngineService`] runs over. Paired with an [`EngineConfig`] (engine-scoped).
#[derive(Clone, Default)]
pub struct ProviderOptions {
    /// Preferred provider id, or `None` to auto-detect from the environment.
    pub provider: Option<String>,
    /// Default model id (optionally `provider/`-qualified).
    pub model: Option<String>,
    /// Client-configured OpenAI-compatible providers, registered after the
    /// built-ins in vec order.
    pub custom: Vec<CustomProviderSpec>,
    /// Explicit API keys for built-in providers (`provider id → key`), consulted
    /// before the environment. Empty = environment-only.
    pub provider_keys: BTreeMap<String, String>,
    /// Explicit region overrides for region-scoped built-in providers
    /// (`provider id → region`). Empty = environment/default region.
    pub provider_regions: BTreeMap<String, String>,
}

/// Native loop over a single provider resolved from `opts.provider` or the
/// environment.
pub fn native(opts: ProviderOptions, config: EngineConfig) -> EngineResult<EngineService> {
    let (mut providers, default_model) = resolve_real_providers(
        opts.provider.as_deref(),
        opts.model,
        &opts.custom,
        &opts.provider_keys,
    )?;
    connect_bedrock(&mut providers, &opts.provider_keys, &opts.provider_regions);
    EngineService::native(providers, Some(default_model), config)
}

/// Native loop over every provider whose credentials resolve, so
/// provider-qualified model refs can switch providers per turn.
///
/// Succeeds with an empty registry and no default model when nothing resolves.
pub fn native_all(opts: ProviderOptions, config: EngineConfig) -> EngineResult<EngineService> {
    let (mut providers, mut default_model) = resolve_available_providers(
        opts.provider.as_deref(),
        opts.model,
        &opts.custom,
        &opts.provider_keys,
    )?;
    if let Some(bedrock_model) =
        connect_bedrock(&mut providers, &opts.provider_keys, &opts.provider_regions)
    {
        default_model = default_model.or(Some(bedrock_model));
    }
    EngineService::native(providers, default_model, config)
}
