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

#[derive(Debug, Clone)]
pub struct CustomProviderSpec {
    pub id: String,

    pub base_url: String,

    pub api_key: String,

    pub default_model: Option<String>,

    pub models: Vec<ModelInfo>,

    pub thinking: bool,
}

#[derive(Clone, Default)]
pub struct ProviderOptions {
    pub provider: Option<String>,

    pub model: Option<String>,

    pub custom: Vec<CustomProviderSpec>,

    pub provider_keys: BTreeMap<String, String>,

    pub provider_regions: BTreeMap<String, String>,
}

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
