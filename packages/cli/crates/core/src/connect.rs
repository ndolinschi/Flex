//! Custom-provider validation and the `/connect` provider gallery registry.
//!
//! Probes a candidate OpenAI-compatible endpoint by listing its models, so
//! the user gets immediate feedback (bad URL, bad key, no route) before the
//! config is saved. Shared by the TUI wizard and the headless subcommand.

use std::time::Duration;

use agentloop_contracts::ModelInfo;
use agentloop_core::Provider;
use agentloop_provider_openai::{OpenAiConfig, OpenAiProvider, oauth_tokens_discoverable};

use crate::prefs::{CliPrefs, ProviderConfig, resolve_api_key};

/// How long the probe gets before it counts as unreachable.
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(5);

/// Provider gallery grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCategory {
    Popular,
    Cloud,
    Local,
    Custom,
}

impl ProviderCategory {
    /// Section title in the connect gallery.
    pub fn label(self) -> &'static str {
        match self {
            Self::Popular => "Popular",
            Self::Cloud => "Cloud",
            Self::Local => "Local",
            Self::Custom => "Custom",
        }
    }
}

/// How the user authenticates with a gallery template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuth {
    /// Prompt for an API key (or accept env reference).
    ApiKey { env_var: Option<&'static str> },
    /// Credentials come only from the environment — no key step.
    EnvOnly { env_var: &'static str },
    /// GitHub device-flow sign-in (Copilot).
    DeviceFlow,
    /// Multiple auth methods — show a picker (OpenAI).
    MultiMethod,
}

/// One auth option in the `/connect` wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethodKind {
    OAuthBrowser,
    OAuthHeadless,
    ApiKey,
    DeviceFlow,
}

/// Label + kind for the auth-method step.
#[derive(Debug, Clone, Copy)]
pub struct AuthMethodSpec {
    pub kind: AuthMethodKind,
    pub label: &'static str,
}

/// Sentinel id for the gallery's free-text custom-provider row.
pub const CUSTOM_PROVIDER_ROW: &str = "__custom__";

/// One row in the `/connect` provider gallery.
#[derive(Debug, Clone, Copy)]
pub struct ProviderTemplate {
    pub id: &'static str,
    pub label: &'static str,
    pub category: ProviderCategory,
    pub base_url: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub auth: ProviderAuth,
    pub description: &'static str,
    pub thinking: bool,
}

/// All built-in gallery templates, in display order.
pub fn provider_templates() -> &'static [ProviderTemplate] {
    &[
        ProviderTemplate {
            id: "anthropic",
            label: "Anthropic",
            category: ProviderCategory::Popular,
            base_url: None,
            default_model: None,
            auth: ProviderAuth::EnvOnly {
                env_var: "ANTHROPIC_API_KEY",
            },
            description: "Claude models via API key",
            thinking: false,
        },
        ProviderTemplate {
            id: "openai",
            label: "OpenAI",
            category: ProviderCategory::Popular,
            base_url: Some("https://api.openai.com/v1"),
            default_model: Some("gpt-4.1-mini"),
            auth: ProviderAuth::MultiMethod,
            description: "GPT models · ChatGPT or API key",
            thinking: false,
        },
        ProviderTemplate {
            id: "deepseek",
            label: "DeepSeek",
            category: ProviderCategory::Popular,
            base_url: Some("https://api.deepseek.com/v1"),
            default_model: Some("deepseek-v4-pro"),
            auth: ProviderAuth::ApiKey {
                env_var: Some("DEEPSEEK_API_KEY"),
            },
            description: "V4 Pro + Flash · extended thinking",
            thinking: true,
        },
        ProviderTemplate {
            id: "copilot",
            label: "GitHub Copilot",
            category: ProviderCategory::Popular,
            base_url: None,
            default_model: None,
            auth: ProviderAuth::DeviceFlow,
            description: "Sign in with GitHub device flow",
            thinking: false,
        },
        ProviderTemplate {
            id: "gemini",
            label: "Google Gemini",
            category: ProviderCategory::Popular,
            base_url: None,
            default_model: None,
            auth: ProviderAuth::EnvOnly {
                env_var: "GEMINI_API_KEY",
            },
            description: "Gemini models via API key",
            thinking: false,
        },
        // OpenAI-compatible cloud inference providers. Reuse `OpenAiProvider`
        // via `/connect` (no dedicated crate). Default model ids are current
        // starting points; `/models` lists the provider's live catalog. Auth is
        // API-key entry ({env:VAR} accepted) — no env auto-detect, since these
        // are not env-registered built-ins.
        ProviderTemplate {
            id: "groq",
            label: "Groq",
            category: ProviderCategory::Cloud,
            base_url: Some("https://api.groq.com/openai/v1"),
            default_model: Some("openai/gpt-oss-120b"),
            auth: ProviderAuth::ApiKey { env_var: None },
            description: "Fast inference · OpenAI-compatible",
            thinking: false,
        },
        ProviderTemplate {
            id: "xai",
            label: "xAI Grok",
            category: ProviderCategory::Cloud,
            base_url: Some("https://api.x.ai/v1"),
            default_model: Some("grok-4.3"),
            auth: ProviderAuth::ApiKey { env_var: None },
            description: "Grok models · OpenAI-compatible",
            thinking: false,
        },
        ProviderTemplate {
            id: "openrouter",
            label: "OpenRouter",
            category: ProviderCategory::Cloud,
            base_url: Some("https://openrouter.ai/api/v1"),
            default_model: Some("openrouter/auto"),
            auth: ProviderAuth::ApiKey { env_var: None },
            description: "Gateway to many models · auto-routing",
            thinking: false,
        },
        ProviderTemplate {
            id: "cerebras",
            label: "Cerebras",
            category: ProviderCategory::Cloud,
            base_url: Some("https://api.cerebras.ai/v1"),
            default_model: Some("gpt-oss-120b"),
            auth: ProviderAuth::ApiKey { env_var: None },
            description: "Ultra-fast inference · OpenAI-compatible",
            thinking: false,
        },
        // AWS Bedrock is a first-party engine provider (not OpenAI-compatible):
        // it registers from AWS_BEARER_TOKEN_BEDROCK, so `base_url` is None and
        // auth is env-only (Converse API, region from AWS_REGION).
        ProviderTemplate {
            id: "bedrock",
            label: "AWS Bedrock",
            category: ProviderCategory::Cloud,
            base_url: None,
            default_model: None,
            auth: ProviderAuth::EnvOnly {
                env_var: "AWS_BEARER_TOKEN_BEDROCK",
            },
            description: "Claude/Llama/Nova · set AWS_BEARER_TOKEN_BEDROCK",
            thinking: false,
        },
        ProviderTemplate {
            id: "ollama",
            label: "Ollama",
            category: ProviderCategory::Local,
            base_url: None,
            default_model: None,
            auth: ProviderAuth::EnvOnly {
                env_var: "OLLAMA_HOST",
            },
            description: "Local models · set OLLAMA_HOST",
            thinking: false,
        },
    ]
}

/// Auth methods offered for a gallery template (empty when auth is implicit).
pub fn auth_methods(template: &ProviderTemplate) -> &'static [AuthMethodSpec] {
    match template.auth {
        ProviderAuth::MultiMethod if template.id == "openai" => &[
            AuthMethodSpec {
                kind: AuthMethodKind::OAuthBrowser,
                label: "ChatGPT Pro/Plus (browser)",
            },
            AuthMethodSpec {
                kind: AuthMethodKind::OAuthHeadless,
                label: "ChatGPT Pro/Plus (headless)",
            },
            AuthMethodSpec {
                kind: AuthMethodKind::ApiKey,
                label: "Manually enter API Key",
            },
        ],
        ProviderAuth::MultiMethod => &[],
        ProviderAuth::DeviceFlow => &[AuthMethodSpec {
            kind: AuthMethodKind::DeviceFlow,
            label: "GitHub device flow",
        }],
        ProviderAuth::ApiKey { .. } => &[AuthMethodSpec {
            kind: AuthMethodKind::ApiKey,
            label: "API key",
        }],
        ProviderAuth::EnvOnly { .. } => &[],
    }
}

/// Look up a gallery template by id.
pub fn provider_template(id: &str) -> Option<&'static ProviderTemplate> {
    provider_templates().iter().find(|t| t.id == id)
}

/// Known OpenAI-compatible providers: `(base_url, default_model, thinking)`.
pub fn known_provider_defaults(id: &str) -> Option<(&'static str, &'static str, bool)> {
    provider_template(id)
        .filter(|t| t.base_url.is_some())
        .map(|t| {
            (
                t.base_url.unwrap_or(""),
                t.default_model.unwrap_or(""),
                t.thinking,
            )
        })
        .or(match id {
            "openai" => Some(("https://api.openai.com/v1", "gpt-4.1-mini", false)),
            "deepseek" => Some(("https://api.deepseek.com/v1", "deepseek-v4-pro", true)),
            _ => None,
        })
}

/// Whether an environment variable is set and non-empty.
pub fn env_var_configured(var: &str) -> bool {
    std::env::var(var)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

/// Whether a gallery template appears connected (runtime provider, env key, or
/// saved custom config).
pub fn template_is_connected(
    template: &ProviderTemplate,
    runtime_providers: &[String],
    prefs: &CliPrefs,
) -> bool {
    if runtime_providers.iter().any(|id| id == template.id) {
        return true;
    }
    if prefs.providers.contains_key(template.id) {
        return true;
    }
    match template.auth {
        ProviderAuth::EnvOnly { env_var } => env_var_configured(env_var),
        ProviderAuth::ApiKey { env_var: Some(var) } => env_var_configured(var),
        ProviderAuth::ApiKey { env_var: None } => false,
        ProviderAuth::DeviceFlow => crate::has_copilot_credentials(),
        ProviderAuth::MultiMethod if template.id == "openai" => {
            env_var_configured("OPENAI_API_KEY") || oauth_tokens_discoverable()
        }
        ProviderAuth::MultiMethod => false,
    }
}

/// True when the user has no custom providers and no built-in env keys —
/// drives the getting-started card.
pub fn needs_provider_setup(runtime_providers: &[String], prefs: &CliPrefs) -> bool {
    if !runtime_providers.is_empty() {
        return false;
    }
    if !prefs.providers.is_empty() {
        return false;
    }
    !provider_templates()
        .iter()
        .any(|t| template_is_connected(t, runtime_providers, prefs))
}

/// Probe `config` by listing models through a throwaway provider instance.
///
/// A failure is not always fatal — endpoints without `/models` (some GLM
/// deployments) legitimately fail here while chat works; callers offer a
/// "save anyway" path when the user supplied a default model.
pub async fn validate_provider(
    id: &str,
    config: &ProviderConfig,
) -> Result<Vec<ModelInfo>, String> {
    let openai_config =
        if id == "openai" && config.api_key.trim().is_empty() && oauth_tokens_discoverable() {
            OpenAiConfig::from_oauth(
                None,
                Some(config.base_url.clone()),
                config.default_model.clone(),
            )
            .map_err(|err| err.to_string())?
        } else {
            let api_key = resolve_api_key(&config.api_key).map_err(|err| err.to_string())?;
            OpenAiConfig::from_values(
                api_key,
                Some(config.base_url.clone()),
                config.default_model.clone(),
            )
            .map_err(|err| err.to_string())?
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_lists_popular_providers() {
        let ids: Vec<_> = provider_templates()
            .iter()
            .filter(|t| t.category == ProviderCategory::Popular)
            .map(|t| t.id)
            .collect();
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"copilot"));
    }

    #[test]
    fn openai_has_three_auth_methods() {
        let template = provider_template("openai").expect("openai");
        assert_eq!(auth_methods(template).len(), 3);
    }

    #[test]
    fn known_defaults_match_openai_and_deepseek() {
        assert_eq!(
            known_provider_defaults("openai"),
            Some(("https://api.openai.com/v1", "gpt-4.1-mini", false))
        );
        assert_eq!(
            known_provider_defaults("deepseek"),
            Some(("https://api.deepseek.com/v1", "deepseek-v4-pro", true))
        );
    }

    #[test]
    fn needs_setup_false_when_runtime_providers_exist() {
        let prefs = CliPrefs::default();
        assert!(!needs_provider_setup(&["anthropic".to_owned()], &prefs));
    }

    #[test]
    fn cloud_openai_compatible_providers_have_usable_defaults() {
        for id in ["groq", "xai", "openrouter", "cerebras"] {
            let template = provider_template(id).unwrap_or_else(|| panic!("missing template {id}"));
            assert_eq!(template.category, ProviderCategory::Cloud);
            assert!(template.base_url.is_some(), "{id} needs a base_url");
            let (base, model, _thinking) =
                known_provider_defaults(id).unwrap_or_else(|| panic!("no defaults for {id}"));
            assert!(base.starts_with("https://"), "{id} base_url");
            assert!(!model.is_empty(), "{id} default_model");
        }
    }
}
