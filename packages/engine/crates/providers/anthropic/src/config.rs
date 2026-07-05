//! Anthropic environment configuration.

use agentloop_contracts::{ModelInfo, ProviderId};
use agentloop_core::ProviderError;
use agentloop_provider_common::required_env;

pub const ANTHROPIC_PROVIDER_ID: &str = "anthropic";
pub const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Page size for Anthropic `/models` listing (API default is small).
pub(crate) const MODEL_LIST_PAGE_LIMIT: u32 = 100;

/// Baseline Claude model ids merged into live listings when the API omits
/// them (pagination gaps, account visibility). Direct Anthropic API keys list
/// every tier here; Copilot uses its own `/models` endpoint via the
/// `copilot/` provider.
pub(crate) fn known_anthropic_models() -> Vec<ModelInfo> {
    const IDS: &[&str] = &[
        "claude-opus-4-6",
        "claude-opus-4-5",
        "claude-sonnet-4-5",
        "claude-haiku-4-5",
        "claude-3-7-sonnet-latest",
        "claude-3-5-haiku-latest",
    ];
    IDS.iter()
        .map(|id| ModelInfo {
            id: (*id).to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

impl AnthropicConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        let provider = ProviderId::from(ANTHROPIC_PROVIDER_ID);
        Self::from_values(
            required_env(&provider, "ANTHROPIC_API_KEY", "an Anthropic API key")?,
            std::env::var("ANTHROPIC_BASE_URL").ok(),
            std::env::var("ANTHROPIC_MODEL").ok(),
        )
    }

    pub fn from_values(
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, ProviderError> {
        let provider = ProviderId::from(ANTHROPIC_PROVIDER_ID);
        let api_key = api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err(ProviderError::AuthMissing {
                provider,
                hint: "set `ANTHROPIC_API_KEY` to an Anthropic API key".to_owned(),
            });
        }
        Ok(Self {
            api_key,
            base_url: normalize_base_url(base_url.as_deref()),
            default_model: model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_ANTHROPIC_MODEL)
                .to_owned(),
        })
    }

    pub fn messages_url(&self) -> String {
        format!("{}/messages", self.base_url)
    }

    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

fn normalize_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ANTHROPIC_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_standard_env_names_and_defaults() {
        let config = AnthropicConfig::from_values(" sk-ant-test ".to_owned(), None, None);
        match config {
            Ok(config) => {
                assert_eq!(config.api_key, "sk-ant-test");
                assert_eq!(config.base_url, DEFAULT_ANTHROPIC_BASE_URL);
                assert_eq!(config.default_model, DEFAULT_ANTHROPIC_MODEL);
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_honors_base_url_and_model_overrides() {
        let config = AnthropicConfig::from_values(
            "sk-ant-test".to_owned(),
            Some(" https://example.test/v1/ ".to_owned()),
            Some(" claude-test ".to_owned()),
        );
        match config {
            Ok(config) => {
                assert_eq!(config.base_url, "https://example.test/v1");
                assert_eq!(config.default_model, "claude-test");
                assert_eq!(config.messages_url(), "https://example.test/v1/messages");
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_rejects_missing_api_key() {
        let err = AnthropicConfig::from_values(" ".to_owned(), None, None);
        assert!(matches!(err, Err(ProviderError::AuthMissing { .. })));
    }

    #[test]
    fn known_models_include_haiku_tier() {
        let models = known_anthropic_models();
        assert!(
            models.iter().any(|model| model.id == "claude-haiku-4-5"),
            "baseline catalog should list haiku: {:?}",
            models.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
    }
}
