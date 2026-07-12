//! Ollama environment configuration.

pub const OLLAMA_PROVIDER_ID: &str = "ollama";
pub const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaConfig {
    pub host: String,
    pub default_model: String,
}

impl OllamaConfig {
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("OLLAMA_HOST").ok(),
            std::env::var("OLLAMA_MODEL").ok(),
        )
    }

    pub fn from_values(host: Option<String>, model: Option<String>) -> Self {
        Self {
            host: normalize_host(host.as_deref()),
            default_model: model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_OLLAMA_MODEL)
                .to_owned(),
        }
    }

    pub fn chat_url(&self) -> String {
        format!("{}/api/chat", self.host)
    }

    pub fn tags_url(&self) -> String {
        format!("{}/api/tags", self.host)
    }
}

fn normalize_host(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OLLAMA_HOST)
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_standard_env_names_and_defaults() {
        let config = OllamaConfig::from_values(None, None);
        assert_eq!(config.host, DEFAULT_OLLAMA_HOST);
        assert_eq!(config.default_model, DEFAULT_OLLAMA_MODEL);
    }

    #[test]
    fn config_honors_host_and_model_overrides() {
        let config = OllamaConfig::from_values(
            Some(" http://localhost:11435/ ".to_owned()),
            Some(" qwen-test ".to_owned()),
        );
        assert_eq!(config.host, "http://localhost:11435");
        assert_eq!(config.default_model, "qwen-test");
        assert_eq!(config.chat_url(), "http://localhost:11435/api/chat");
    }
}
