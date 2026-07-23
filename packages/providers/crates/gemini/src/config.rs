use agentloop_contracts::ProviderId;
use agentloop_core::ProviderError;
use agentloop_provider_common::required_env;

pub const GEMINI_PROVIDER_ID: &str = "gemini";
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.0-flash";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

impl GeminiConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        let provider = ProviderId::from(GEMINI_PROVIDER_ID);
        Self::from_values(
            required_env(&provider, "GEMINI_API_KEY", "a Gemini API key")?,
            std::env::var("GEMINI_BASE_URL").ok(),
            std::env::var("GEMINI_MODEL").ok(),
        )
    }

    pub fn from_values(
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, ProviderError> {
        let provider = ProviderId::from(GEMINI_PROVIDER_ID);
        let api_key = api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err(ProviderError::AuthMissing {
                provider,
                hint: "set `GEMINI_API_KEY` to a Gemini API key".to_owned(),
            });
        }
        Ok(Self {
            api_key,
            base_url: normalize_base_url(base_url.as_deref()),
            default_model: model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_GEMINI_MODEL)
                .to_owned(),
        })
    }

    pub fn stream_generate_content_url(&self, model: &str) -> String {
        let model_path = if model.starts_with("models/") {
            model.to_owned()
        } else {
            format!("models/{model}")
        };
        format!(
            "{}/{model_path}:streamGenerateContent?alt=sse",
            self.base_url
        )
    }

    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

fn normalize_base_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_GEMINI_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_standard_env_names_and_defaults() {
        let config = GeminiConfig::from_values(" gem-key ".to_owned(), None, None);
        match config {
            Ok(config) => {
                assert_eq!(config.api_key, "gem-key");
                assert_eq!(config.base_url, DEFAULT_GEMINI_BASE_URL);
                assert_eq!(config.default_model, DEFAULT_GEMINI_MODEL);
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_honors_base_url_and_model_overrides() {
        let config = GeminiConfig::from_values(
            "gem-key".to_owned(),
            Some(" https://example.test/v1beta/ ".to_owned()),
            Some(" gemini-test ".to_owned()),
        );
        match config {
            Ok(config) => {
                assert_eq!(config.base_url, "https://example.test/v1beta");
                assert_eq!(config.default_model, "gemini-test");
                assert_eq!(
                    config.stream_generate_content_url("gemini-test"),
                    "https://example.test/v1beta/models/gemini-test:streamGenerateContent?alt=sse"
                );
            }
            Err(err) => panic!("config should load: {err}"),
        }
    }

    #[test]
    fn config_rejects_missing_api_key() {
        let err = GeminiConfig::from_values(" ".to_owned(), None, None);
        assert!(matches!(err, Err(ProviderError::AuthMissing { .. })));
    }
}
