//! Custom-provider validation for the `/connect` flow.
//!
//! Probes a candidate OpenAI-compatible endpoint by listing its models, so
//! the user gets immediate feedback (bad URL, bad key, no route) before the
//! config is saved. Shared by the TUI wizard and the headless subcommand.

use std::time::Duration;

use agentloop_contracts::ModelInfo;
use agentloop_core::Provider;
use agentloop_provider_openai::{OpenAiConfig, OpenAiProvider};

use crate::prefs::{ProviderConfig, resolve_api_key};

/// How long the probe gets before it counts as unreachable.
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(5);

/// Probe `config` by listing models through a throwaway provider instance.
///
/// A failure is not always fatal — endpoints without `/models` (some GLM
/// deployments) legitimately fail here while chat works; callers offer a
/// "save anyway" path when the user supplied a default model.
pub async fn validate_provider(
    id: &str,
    config: &ProviderConfig,
) -> Result<Vec<ModelInfo>, String> {
    let api_key = resolve_api_key(&config.api_key).map_err(|err| err.to_string())?;
    let openai_config = OpenAiConfig::from_values(
        api_key,
        Some(config.base_url.clone()),
        config.default_model.clone(),
    )
    .map_err(|err| err.to_string())?;
    let provider = OpenAiProvider::with_identity(id, openai_config, Vec::new(), config.thinking);
    match tokio::time::timeout(VALIDATE_TIMEOUT, provider.list_models()).await {
        Ok(Ok(models)) => Ok(models),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err(format!(
            "no response within {}s — check the base URL",
            VALIDATE_TIMEOUT.as_secs()
        )),
    }
}
