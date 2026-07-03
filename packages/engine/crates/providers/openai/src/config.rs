//! OpenAI environment configuration.

use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use agentloop_provider_common::required_env;

pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1-mini";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

impl OpenAiConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        let provider = ProviderId::from(OPENAI_PROVIDER_ID);
        Self::from_values(
            required_env(&provider, "OPENAI_API_KEY", "an OpenAI API key")?,
            std::env::var("OPENAI_BASE_URL").ok(),
            std::env::var("OPENAI_MODEL").ok(),
        )
    }

    /// Build a config from explicit values. An empty `api_key` is allowed —
    /// it means a keyless local endpoint (LM Studio, llama.cpp) and no
    /// Authorization header is sent. The env path (`from_env`) still
    /// requires `OPENAI_API_KEY` via [`required_env`].
    pub fn from_values(
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key: api_key.trim().to_owned(),
            base_url: normalize_base_url(base_url.as_deref()),
            default_model: model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_OPENAI_MODEL)
                .to_owned(),
        })
    }

    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

fn normalize_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OPENAI_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_standard_env_names_and_defaults() {
        let config = OpenAiConfig::from_values(" sk-test ".to_owned(), None, None);
        match config {
            Ok(config) => {
                assert_eq!(config.api_key, "sk-test");
                assert_eq!(config.base_url, DEFAULT_OPENAI_BASE_URL);
                assert_eq!(config.default_model, DEFAULT_OPENAI_MODEL);
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_honors_base_url_and_model_overrides() {
        let config = OpenAiConfig::from_values(
            "sk-test".to_owned(),
            Some(" https://example.test/v1/ ".to_owned()),
            Some(" custom-model ".to_owned()),
        );
        match config {
            Ok(config) => {
                assert_eq!(config.base_url, "https://example.test/v1");
                assert_eq!(config.default_model, "custom-model");
                assert_eq!(
                    config.chat_completions_url(),
                    "https://example.test/v1/chat/completions"
                );
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_accepts_empty_api_key_for_keyless_endpoints() {
        let config = OpenAiConfig::from_values(" ".to_owned(), None, None).expect("keyless ok");
        assert_eq!(config.api_key, "");
    }
}
