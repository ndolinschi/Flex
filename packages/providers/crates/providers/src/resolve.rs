//! Provider detection, construction, and default-model resolution.
//!
//! Given provider-scoped options, decide which built-in providers have
//! credentials, build them (plus client-configured OpenAI-compatible specs and
//! first-party keys like Bedrock), and pick the default model. Two entry points
//! mirror the two service constructors — [`resolve_real_providers`] (single
//! preferred provider) and [`resolve_available_providers`] (every provider
//! whose credentials resolve).

use std::sync::Arc;

use agentloop_contracts::{ModelRef, ProviderId};
use agentloop_core::{ProviderError, ProviderRegistry};
use agentloop_engine::{EngineResult, EngineServiceError};
use agentloop_provider_anthropic::{ANTHROPIC_PROVIDER_ID, AnthropicConfig, AnthropicProvider};
use agentloop_provider_bedrock::{
    BEDROCK_PROVIDER_ID, BedrockAuth, BedrockConfig, BedrockProvider,
};
use agentloop_provider_chatgpt::{CHATGPT_PROVIDER_ID, ChatgptConfig, ChatgptProvider};
use agentloop_provider_copilot::{COPILOT_PROVIDER_ID, CopilotConfig, CopilotProvider};
use agentloop_provider_gemini::{GEMINI_PROVIDER_ID, GeminiConfig, GeminiProvider};
use agentloop_provider_ollama::{OLLAMA_PROVIDER_ID, OllamaProvider};
use agentloop_provider_openai::{OPENAI_PROVIDER_ID, OpenAiConfig, OpenAiProvider};

use crate::CustomProviderSpec;

/// Whether an environment variable is set and non-empty.
fn env_is_set(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// The built-in provider ids custom specs may not shadow. `openai` and
/// `deepseek` are deliberately absent: both are OpenAI-compatible endpoints a
/// user can supply credentials for via `/connect <id> <key>`, so a custom spec
/// of either id must resolve (and win over the env built-in) rather than be
/// rejected as a conflict.
const BUILTIN_PROVIDER_IDS: [&str; 6] = [
    ANTHROPIC_PROVIDER_ID,
    BEDROCK_PROVIDER_ID,
    GEMINI_PROVIDER_ID,
    COPILOT_PROVIDER_ID,
    CHATGPT_PROVIDER_ID,
    OLLAMA_PROVIDER_ID,
];

/// `true` when `id` matches `^[a-z0-9][a-z0-9_-]*$` (which also excludes `/`,
/// the [`ModelRef`] separator).
fn valid_custom_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Build one custom provider plus its default model id.
///
/// Validates the spec (id shape, non-empty base URL and key) so failures are
/// attributed to the custom id instead of surfacing as `openai` errors.
/// Default model precedence: `spec.default_model`, else the first static
/// model, else the OpenAI config default as a documented last resort.
fn build_custom_provider(
    spec: &CustomProviderSpec,
) -> Result<(Arc<dyn agentloop_core::Provider>, String), EngineServiceError> {
    let invalid = |message: &str| EngineServiceError::CustomProviderInvalid {
        id: spec.id.clone(),
        message: message.to_owned(),
    };
    if !valid_custom_id(&spec.id) {
        return Err(invalid(
            "id must match ^[a-z0-9][a-z0-9_-]*$ (lowercase, no `/`)",
        ));
    }
    if spec.base_url.trim().is_empty() {
        return Err(invalid("base_url is empty"));
    }
    let config = OpenAiConfig::from_values(
        spec.api_key.clone(),
        Some(spec.base_url.clone()),
        spec.default_model.clone(),
    )?;
    let default_model = spec
        .default_model
        .clone()
        .or_else(|| spec.models.first().map(|model| model.id.clone()))
        .unwrap_or_else(|| config.default_model.clone());
    let provider =
        OpenAiProvider::with_identity(spec.id.as_str(), config, spec.models.clone(), spec.thinking);
    Ok((Arc::new(provider), default_model))
}

/// A constructed provider paired with its default model id.
type ProviderWithDefault = (Arc<dyn agentloop_core::Provider>, String);

/// DeepSeek is served over an OpenAI-compatible Chat Completions API, so it's
/// a built-in on top of [`OpenAiProvider`] rather than a bespoke crate.
pub(crate) const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";
pub(crate) const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-v4-pro";

/// Build the built-in DeepSeek provider from `DEEPSEEK_API_KEY` (optional
/// `DEEPSEEK_MODEL`). Returns `Ok(None)` when the key is unset, so callers
/// auto-register it only when the user has opted in — matching how Ollama is
/// gated. `(provider, default_model)` on success.
///
/// Note: speculative decoding (dSpark) is applied server-side by DeepSeek and
/// is transparent here — there is no request-time knob to set.
fn build_deepseek_from_env() -> Result<Option<ProviderWithDefault>, ProviderError> {
    let Some(api_key) = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let model = std::env::var("DEEPSEEK_MODEL").ok();
    build_deepseek(api_key, model).map(Some)
}

/// Pure builder for the DeepSeek provider (no env access, so it's directly
/// testable). `model` falls back to `DEEPSEEK_DEFAULT_MODEL` (`deepseek-v4-pro`)
/// — passing an explicit model matters because `from_values` otherwise defaults
/// to the OpenAI model (`gpt-4.1-mini`), which is wrong for DeepSeek.
pub(crate) fn build_deepseek(
    api_key: String,
    model: Option<String>,
) -> Result<ProviderWithDefault, ProviderError> {
    let model = model
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEEPSEEK_DEFAULT_MODEL.to_owned());
    let config = OpenAiConfig::from_values(
        api_key,
        Some(DEEPSEEK_BASE_URL.to_owned()),
        Some(model.clone()),
    )?;
    let provider = OpenAiProvider::with_identity(DEEPSEEK_PROVIDER_ID, config, Vec::new(), true);
    Ok((Arc::new(provider), model))
}

/// An OpenAI-compatible provider preset, gated on its API-key env var —
/// exactly the DeepSeek pattern, table-driven. Preset ids are deliberately
/// NOT in [`BUILTIN_PROVIDER_IDS`], so a user's custom spec of the same id
/// resolves and wins over the env built-in.
pub(crate) struct CompatPreset {
    pub(crate) id: &'static str,
    pub(crate) key_env: &'static str,
    pub(crate) model_env: &'static str,
    pub(crate) base_url: &'static str,
    pub(crate) default_model: &'static str,
}

/// Built-in OpenAI-compatible presets. OpenRouter is the aggregator route to
/// GLM/Kimi/MiniMax and hundreds of other models without a bespoke client.
pub(crate) const COMPAT_PRESETS: [CompatPreset; 4] = [
    CompatPreset {
        id: "openrouter",
        key_env: "OPENROUTER_API_KEY",
        model_env: "OPENROUTER_MODEL",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "openrouter/auto",
    },
    CompatPreset {
        id: "groq",
        key_env: "GROQ_API_KEY",
        model_env: "GROQ_MODEL",
        base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
    },
    CompatPreset {
        id: "mistral",
        key_env: "MISTRAL_API_KEY",
        model_env: "MISTRAL_MODEL",
        base_url: "https://api.mistral.ai/v1",
        default_model: "mistral-large-latest",
    },
    CompatPreset {
        id: "xai",
        key_env: "XAI_API_KEY",
        model_env: "XAI_MODEL",
        base_url: "https://api.x.ai/v1",
        default_model: "grok-4",
    },
];

/// Pure builder for a compat preset (no env access, directly testable).
pub(crate) fn build_compat(
    preset: &CompatPreset,
    api_key: String,
    model: Option<String>,
) -> Result<ProviderWithDefault, ProviderError> {
    let model = model
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| preset.default_model.to_owned());
    let config = OpenAiConfig::from_values(
        api_key,
        Some(preset.base_url.to_owned()),
        Some(model.clone()),
    )?;
    let provider = OpenAiProvider::with_identity(preset.id, config, Vec::new(), false);
    Ok((Arc::new(provider), model))
}

/// Build a preset from its env vars; `Ok(None)` when the key is unset.
fn build_compat_from_env(
    preset: &CompatPreset,
) -> Result<Option<ProviderWithDefault>, ProviderError> {
    let Some(api_key) = std::env::var(preset.key_env)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let model = std::env::var(preset.model_env).ok();
    build_compat(preset, api_key, model).map(Some)
}

/// Register a Bedrock provider built from a client-connected API key
/// (`provider_keys["bedrock"]`), overriding any credential-less Bedrock the
/// environment may have registered. Region/model still come from the
/// environment (or Bedrock defaults). Returns its default model ref so callers
/// can adopt it when they have no other default; no-op without a bedrock key.
pub fn connect_bedrock(
    providers: &mut ProviderRegistry,
    provider_keys: &std::collections::BTreeMap<String, String>,
    provider_regions: &std::collections::BTreeMap<String, String>,
) -> Option<ModelRef> {
    let key = provider_keys.get(BEDROCK_PROVIDER_ID)?;
    if key.trim().is_empty() {
        return None;
    }
    let mut config = BedrockConfig::from_env();
    config.auth = BedrockAuth::Bearer(key.clone());
    if let Some(region) = provider_regions
        .get(BEDROCK_PROVIDER_ID)
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        config.region = region.to_owned();
    }
    let provider = BedrockProvider::new(config);
    let model = provider.default_model().to_owned();
    providers.register(Arc::new(provider));
    Some(ModelRef(format!("{BEDROCK_PROVIDER_ID}/{model}")))
}

pub fn resolve_real_providers(
    provider_arg: Option<&str>,
    model_arg: Option<String>,
    custom: &[CustomProviderSpec],
    provider_keys: &std::collections::BTreeMap<String, String>,
) -> EngineResult<(ProviderRegistry, ModelRef)> {
    let provider_name = match provider_arg {
        Some(provider) => provider,
        None if env_is_set("OPENAI_API_KEY") => OPENAI_PROVIDER_ID,
        None if env_is_set("ANTHROPIC_API_KEY") => ANTHROPIC_PROVIDER_ID,
        None if env_is_set("GEMINI_API_KEY") => GEMINI_PROVIDER_ID,
        None if env_is_set("DEEPSEEK_API_KEY") => DEEPSEEK_PROVIDER_ID,
        None if env_is_set("OPENROUTER_API_KEY") => "openrouter",
        None if env_is_set("AWS_BEARER_TOKEN_BEDROCK") => BEDROCK_PROVIDER_ID,
        None if CopilotConfig::discoverable() => COPILOT_PROVIDER_ID,
        None if ChatgptConfig::discoverable() => CHATGPT_PROVIDER_ID,
        None if env_is_set("OLLAMA_HOST") || env_is_set("OLLAMA_MODEL") => OLLAMA_PROVIDER_ID,
        None => {
            return Err(ProviderError::AuthMissing {
                provider: ProviderId::from("runtime"),
                hint: "set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, \
                       `DEEPSEEK_API_KEY`, `AWS_BEARER_TOKEN_BEDROCK` for AWS Bedrock, \
                       `OLLAMA_HOST`/`OLLAMA_MODEL` for local Ollama, sign \
                       in to GitHub Copilot (VS Code / Copilot CLI, or set `COPILOT_GITHUB_TOKEN`), \
                       or sign in with ChatGPT Plus/Pro; \
                       optional model env vars: `OPENAI_MODEL`, `ANTHROPIC_MODEL`, \
                       `GEMINI_MODEL`, `DEEPSEEK_MODEL`, `BEDROCK_MODEL`, `OLLAMA_MODEL`, \
                       `COPILOT_MODEL`, `CHATGPT_MODEL`"
                    .to_owned(),
            }
            .into());
        }
    };

    match provider_name {
        OPENAI_PROVIDER_ID if !custom.iter().any(|spec| spec.id == OPENAI_PROVIDER_ID) => {
            let provider = if let Some(key) = provider_keys.get(OPENAI_PROVIDER_ID) {
                OpenAiProvider::new(OpenAiConfig::from_values(
                    key.clone(),
                    std::env::var("OPENAI_BASE_URL").ok(),
                    std::env::var("OPENAI_MODEL").ok(),
                )?)
            } else {
                OpenAiProvider::from_env()?
            };
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{OPENAI_PROVIDER_ID}/{model}"))))
        }
        ANTHROPIC_PROVIDER_ID => {
            let provider = if let Some(key) = provider_keys.get(ANTHROPIC_PROVIDER_ID) {
                AnthropicProvider::new(AnthropicConfig::from_values(
                    key.clone(),
                    std::env::var("ANTHROPIC_BASE_URL").ok(),
                    std::env::var("ANTHROPIC_MODEL").ok(),
                )?)
            } else {
                AnthropicProvider::from_env()?
            };
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{ANTHROPIC_PROVIDER_ID}/{model}")),
            ))
        }
        GEMINI_PROVIDER_ID => {
            let provider = if let Some(key) = provider_keys.get(GEMINI_PROVIDER_ID) {
                GeminiProvider::new(GeminiConfig::from_values(
                    key.clone(),
                    std::env::var("GEMINI_BASE_URL").ok(),
                    std::env::var("GEMINI_MODEL").ok(),
                )?)
            } else {
                GeminiProvider::from_env()?
            };
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{GEMINI_PROVIDER_ID}/{model}"))))
        }
        COPILOT_PROVIDER_ID => {
            let provider = if let Some(key) = provider_keys.get(COPILOT_PROVIDER_ID) {
                CopilotProvider::new(CopilotConfig::with_token(key.clone()))
            } else {
                CopilotProvider::from_env()?
            };
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{COPILOT_PROVIDER_ID}/{model}")),
            ))
        }
        CHATGPT_PROVIDER_ID => {
            let provider = ChatgptProvider::from_oauth()?;
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{CHATGPT_PROVIDER_ID}/{model}")),
            ))
        }
        OLLAMA_PROVIDER_ID => {
            let provider = OllamaProvider::from_env();
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((providers, ModelRef(format!("{OLLAMA_PROVIDER_ID}/{model}"))))
        }
        BEDROCK_PROVIDER_ID => {
            let provider = if let Some(key) = provider_keys.get(BEDROCK_PROVIDER_ID) {
                // Mirror the other arms: a caller-supplied key wins over the
                // environment. Region still falls back through
                // `BedrockConfig::from_env`'s precedence (`BEDROCK_REGION` ->
                // `AWS_REGION` -> `AWS_DEFAULT_REGION` -> default) so a client
                // key alone doesn't lose region/model env overrides.
                let mut config = BedrockConfig::from_env();
                config.auth = BedrockAuth::Bearer(key.clone());
                BedrockProvider::new(config)
            } else {
                BedrockProvider::from_env()
            };
            if !provider.has_credentials() {
                return Err(ProviderError::AuthMissing {
                    provider: ProviderId::from(BEDROCK_PROVIDER_ID),
                    hint: "set `AWS_BEARER_TOKEN_BEDROCK` (a Bedrock API key), or AWS SigV4 \
                           credentials (`AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`, optional \
                           `AWS_SESSION_TOKEN`); optional `BEDROCK_REGION`/`BEDROCK_MODEL`"
                        .to_owned(),
                }
                .into());
            }
            let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());
            let mut providers = ProviderRegistry::new();
            providers.register(Arc::new(provider));
            Ok((
                providers,
                ModelRef(format!("{BEDROCK_PROVIDER_ID}/{model}")),
            ))
        }
        other => {
            if let Some(spec) = custom.iter().find(|spec| spec.id == other) {
                let (provider, default_model) = build_custom_provider(spec)?;
                let model = model_arg.unwrap_or(default_model);
                let mut providers = ProviderRegistry::new();
                providers.register(provider);
                Ok((providers, ModelRef(format!("{other}/{model}"))))
            } else if other == DEEPSEEK_PROVIDER_ID {
                let (provider, default_model) =
                    if let Some(key) = provider_keys.get(DEEPSEEK_PROVIDER_ID) {
                        build_deepseek(key.clone(), None)?
                    } else {
                        build_deepseek_from_env()?.ok_or_else(|| ProviderError::AuthMissing {
                            provider: ProviderId::from(DEEPSEEK_PROVIDER_ID),
                            hint: "set `DEEPSEEK_API_KEY` (optional `DEEPSEEK_MODEL`)".to_owned(),
                        })?
                    };
                let model = model_arg.unwrap_or(default_model);
                let mut providers = ProviderRegistry::new();
                providers.register(provider);
                Ok((providers, ModelRef(format!("{other}/{model}"))))
            } else if let Some(preset) = COMPAT_PRESETS.iter().find(|preset| preset.id == other) {
                let (provider, default_model) = if let Some(key) = provider_keys.get(preset.id) {
                    build_compat(preset, key.clone(), None)?
                } else {
                    build_compat_from_env(preset)?.ok_or_else(|| ProviderError::AuthMissing {
                        provider: ProviderId::from(preset.id),
                        hint: format!("set `{}` (optional `{}`)", preset.key_env, preset.model_env),
                    })?
                };
                let model = model_arg.unwrap_or(default_model);
                let mut providers = ProviderRegistry::new();
                providers.register(provider);
                Ok((providers, ModelRef(format!("{other}/{model}"))))
            } else {
                Err(EngineServiceError::UnsupportedProvider(other.to_owned()))
            }
        }
    }
}

/// Register every provider whose credentials resolve from the environment,
/// in the same precedence order [`resolve_real_providers`] detects them,
/// followed by every `custom` spec in vec order.
///
/// Providers with missing credentials are skipped (debug-traced); any other
/// construction error propagates. Custom specs shadowing a built-in id are
/// rejected with [`EngineServiceError::CustomProviderConflict`]; malformed or
/// duplicate specs with [`EngineServiceError::CustomProviderInvalid`].
/// `preferred` must resolve (it may name a custom id) and becomes the
/// registry priority. The returned [`ModelRef`] is provider-qualified:
/// `model_arg` wins (qualified against the priority provider unless it
/// already names one), else the priority provider's default model.
///
/// No credentials anywhere and no custom provider configured is not an
/// error here: it returns an empty registry and `None` default model,
/// deferring the failure to turn time so a client can open with no provider
/// configured and let the user add one (e.g. via `/connect`) before prompting.
pub fn resolve_available_providers(
    preferred: Option<&str>,
    model_arg: Option<String>,
    custom: &[CustomProviderSpec],
    provider_keys: &std::collections::BTreeMap<String, String>,
) -> EngineResult<(ProviderRegistry, Option<ModelRef>)> {
    /// `(provider, its default model)` for a known name; `None` for unknown.
    fn build_provider(
        name: &str,
        provider_keys: &std::collections::BTreeMap<String, String>,
    ) -> Result<Option<ProviderWithDefault>, ProviderError> {
        fn boxed<P: agentloop_core::Provider + 'static>(
            provider: P,
            default_model: String,
        ) -> Option<ProviderWithDefault> {
            Some((Arc::new(provider), default_model))
        }
        match name {
            OPENAI_PROVIDER_ID => {
                if let Some(key) = provider_keys.get(OPENAI_PROVIDER_ID) {
                    let config = OpenAiConfig::from_values(key.clone(), None, None)?;
                    let model = config.default_model.clone();
                    Ok(boxed(OpenAiProvider::new(config), model))
                } else {
                    OpenAiProvider::from_env().map(|p| {
                        let model = p.default_model().to_owned();
                        boxed(p, model)
                    })
                }
            }
            ANTHROPIC_PROVIDER_ID => {
                if let Some(key) = provider_keys.get(ANTHROPIC_PROVIDER_ID) {
                    let config = AnthropicConfig::from_values(key.clone(), None, None)?;
                    let model = config.default_model.clone();
                    Ok(boxed(AnthropicProvider::new(config), model))
                } else {
                    AnthropicProvider::from_env().map(|p| {
                        let model = p.default_model().to_owned();
                        boxed(p, model)
                    })
                }
            }
            GEMINI_PROVIDER_ID => {
                if let Some(key) = provider_keys.get(GEMINI_PROVIDER_ID) {
                    let config = GeminiConfig::from_values(key.clone(), None, None)?;
                    let model = config.default_model.clone();
                    Ok(boxed(GeminiProvider::new(config), model))
                } else {
                    GeminiProvider::from_env().map(|p| {
                        let model = p.default_model().to_owned();
                        boxed(p, model)
                    })
                }
            }
            COPILOT_PROVIDER_ID => {
                if let Some(key) = provider_keys.get(COPILOT_PROVIDER_ID) {
                    let provider = CopilotProvider::new(CopilotConfig::with_token(key.clone()));
                    let model = provider.default_model().to_owned();
                    Ok(boxed(provider, model))
                } else {
                    CopilotProvider::from_env().map(|p| {
                        let model = p.default_model().to_owned();
                        boxed(p, model)
                    })
                }
            }
            CHATGPT_PROVIDER_ID => ChatgptProvider::from_oauth().map(|p| {
                let model = p.default_model().to_owned();
                boxed(p, model)
            }),
            OLLAMA_PROVIDER_ID => {
                let provider = OllamaProvider::from_env();
                let model = provider.default_model().to_owned();
                Ok(boxed(provider, model))
            }
            BEDROCK_PROVIDER_ID => {
                let provider = if let Some(key) = provider_keys.get(BEDROCK_PROVIDER_ID) {
                    let mut config = BedrockConfig::from_env();
                    config.auth = BedrockAuth::Bearer(key.clone());
                    BedrockProvider::new(config)
                } else {
                    BedrockProvider::from_env()
                };
                let model = provider.default_model().to_owned();
                Ok(boxed(provider, model))
            }
            _ => Ok(None),
        }
    }

    let mut providers = ProviderRegistry::new();
    let mut defaults: Vec<(ProviderId, String)> = Vec::new();
    let mut register =
        |registry: &mut ProviderRegistry, provider: Arc<dyn agentloop_core::Provider>, model| {
            defaults.push((provider.id(), model));
            registry.register(provider);
        };

    for name in [
        OPENAI_PROVIDER_ID,
        ANTHROPIC_PROVIDER_ID,
        GEMINI_PROVIDER_ID,
        COPILOT_PROVIDER_ID,
        CHATGPT_PROVIDER_ID,
    ] {
        if name == OPENAI_PROVIDER_ID && custom.iter().any(|spec| spec.id == OPENAI_PROVIDER_ID) {
            continue;
        }
        match build_provider(name, provider_keys) {
            Ok(Some((provider, model))) => register(&mut providers, provider, model),
            Ok(None) => {}
            Err(ProviderError::AuthMissing { .. }) => {
                tracing::debug!(target: "providers", provider = name, "skipped: no credentials");
            }
            Err(err) => return Err(err.into()),
        }
    }
    // Ollama needs a host, not credentials — always register with the default
    // localhost when building the multi-provider registry. Gating on
    // `OLLAMA_HOST`/`OLLAMA_MODEL` hid local Ollama from desktop inline
    // completion (and any other all_providers consumer) when chat used a
    // different preferred provider. `list_models` already tolerates a down
    // daemon; stream calls surface the connection error.
    if let Ok(Some((provider, model))) = build_provider(OLLAMA_PROVIDER_ID, provider_keys) {
        register(&mut providers, provider, model);
    }
    if env_is_set("AWS_BEARER_TOKEN_BEDROCK") {
        if let Ok(Some((provider, model))) = build_provider(BEDROCK_PROVIDER_ID, provider_keys) {
            register(&mut providers, provider, model);
        }
    }
    if custom.iter().all(|spec| spec.id != DEEPSEEK_PROVIDER_ID) {
        let deepseek_opt = if let Some(key) = provider_keys.get(DEEPSEEK_PROVIDER_ID) {
            build_deepseek(key.clone(), None).ok()
        } else {
            build_deepseek_from_env().ok().flatten()
        };
        if let Some((provider, model)) = deepseek_opt {
            register(&mut providers, provider, model);
        }
    }

    for preset in &COMPAT_PRESETS {
        if custom.iter().any(|spec| spec.id == preset.id) {
            continue;
        }
        let built = if let Some(key) = provider_keys.get(preset.id) {
            build_compat(preset, key.clone(), None).ok()
        } else {
            build_compat_from_env(preset).ok().flatten()
        };
        if let Some((provider, model)) = built {
            register(&mut providers, provider, model);
        }
    }

    let mut seen_custom_ids = std::collections::HashSet::new();
    for spec in custom {
        if BUILTIN_PROVIDER_IDS.contains(&spec.id.as_str()) {
            return Err(EngineServiceError::CustomProviderConflict(spec.id.clone()));
        }
        if !seen_custom_ids.insert(spec.id.as_str()) {
            return Err(EngineServiceError::CustomProviderInvalid {
                id: spec.id.clone(),
                message: "declared more than once".to_owned(),
            });
        }
        let (provider, model) = build_custom_provider(spec)?;
        register(&mut providers, provider, model);
    }

    if let Some(name) = preferred {
        let id = ProviderId::from(name);
        if providers.get(&id).is_none() {
            match build_provider(name, provider_keys).map_err(EngineServiceError::from)? {
                Some((provider, model)) => register(&mut providers, provider, model),
                None => return Err(EngineServiceError::UnsupportedProvider(name.to_owned())),
            }
        }
        providers.set_priority(vec![id]);
    }

    let default_model = match model_arg {
        Some(model) if model.contains('/') => Some(ModelRef(model)),
        Some(model) => providers
            .ids()
            .first()
            .map(|first| ModelRef(format!("{first}/{model}"))),
        None => providers.ids().first().cloned().map(|first| {
            let model = defaults
                .iter()
                .find(|(id, _)| *id == first)
                .map(|(_, model)| model.clone())
                .unwrap_or_default();
            ModelRef(format!("{first}/{model}"))
        }),
    };

    Ok((providers, default_model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CustomProviderSpec;
    use agentloop_contracts::{ErrorCode, ModelInfo, ProviderId};

    fn no_keys() -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::new()
    }

    fn spec(id: &str) -> CustomProviderSpec {
        CustomProviderSpec {
            id: id.to_owned(),
            base_url: "https://example.test/v1".to_owned(),
            api_key: "sk-test".to_owned(),
            default_model: Some("test-chat".to_owned()),
            models: Vec::new(),
            thinking: false,
        }
    }

    #[test]
    fn connect_bedrock_registers_a_provider_from_a_key() {
        let mut providers = ProviderRegistry::new();
        let keys = std::collections::BTreeMap::from([(
            BEDROCK_PROVIDER_ID.to_owned(),
            "bedrock-api-key".to_owned(),
        )]);
        let default = connect_bedrock(&mut providers, &keys, &std::collections::BTreeMap::new());
        assert!(default.is_some_and(|m| m.0.starts_with("bedrock/")));
        assert!(
            providers
                .get(&ProviderId::from(BEDROCK_PROVIDER_ID))
                .is_some()
        );
    }

    #[test]
    fn connect_bedrock_with_region_override_registers() {
        let mut providers = ProviderRegistry::new();
        let keys = std::collections::BTreeMap::from([(
            BEDROCK_PROVIDER_ID.to_owned(),
            "bedrock-api-key".to_owned(),
        )]);
        let regions = std::collections::BTreeMap::from([(
            BEDROCK_PROVIDER_ID.to_owned(),
            "eu-west-1".to_owned(),
        )]);
        assert!(connect_bedrock(&mut providers, &keys, &regions).is_some());
        assert!(
            providers
                .get(&ProviderId::from(BEDROCK_PROVIDER_ID))
                .is_some()
        );
    }

    #[test]
    fn connect_bedrock_is_a_noop_without_a_key() {
        let mut providers = ProviderRegistry::new();
        let no_regions = std::collections::BTreeMap::new();
        assert!(
            connect_bedrock(
                &mut providers,
                &std::collections::BTreeMap::new(),
                &no_regions
            )
            .is_none()
        );
        let empty =
            std::collections::BTreeMap::from([(BEDROCK_PROVIDER_ID.to_owned(), " ".to_owned())]);
        assert!(connect_bedrock(&mut providers, &empty, &no_regions).is_none());
        assert!(
            providers
                .get(&ProviderId::from(BEDROCK_PROVIDER_ID))
                .is_none()
        );
    }

    #[test]
    fn bedrock_client_key_wins_over_missing_env_in_single_provider_resolver() {
        // Root-cause regression: a caller-supplied Bedrock key must resolve
        // even when no Bedrock env vars are set at all — previously this arm
        // called `BedrockProvider::from_env()` unconditionally and returned
        // `AuthMissing` before ever looking at `provider_keys`.
        let keys = std::collections::BTreeMap::from([(
            BEDROCK_PROVIDER_ID.to_owned(),
            "bedrock-api-key".to_owned(),
        )]);
        let (providers, model) =
            match resolve_real_providers(Some(BEDROCK_PROVIDER_ID), None, &[], &keys) {
                Ok(resolved) => resolved,
                Err(err) => panic!("client-supplied bedrock key must resolve: {err}"),
            };
        assert!(model.0.starts_with("bedrock/"));
        assert!(
            providers
                .get(&ProviderId::from(BEDROCK_PROVIDER_ID))
                .is_some()
        );
    }

    #[test]
    fn bedrock_without_key_or_env_is_auth_missing_in_single_provider_resolver() {
        let err = match resolve_real_providers(Some(BEDROCK_PROVIDER_ID), None, &[], &no_keys()) {
            Ok(_) => panic!("bedrock without any credentials must not resolve"),
            Err(err) => err,
        };
        assert_eq!(err.to_engine_error().code, ErrorCode::AuthMissing);
    }

    #[test]
    fn bedrock_client_key_wins_over_missing_env_in_multi_resolver() {
        let keys = std::collections::BTreeMap::from([(
            BEDROCK_PROVIDER_ID.to_owned(),
            "bedrock-api-key".to_owned(),
        )]);
        let (providers, model) =
            match resolve_available_providers(Some(BEDROCK_PROVIDER_ID), None, &[], &keys) {
                Ok(resolved) => resolved,
                Err(err) => panic!("client-supplied bedrock key must resolve: {err}"),
            };
        assert!(
            providers
                .get(&ProviderId::from(BEDROCK_PROVIDER_ID))
                .is_some()
        );
        assert!(
            model
                .expect("bedrock resolves a model")
                .0
                .starts_with("bedrock/")
        );
    }

    fn model_info(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        }
    }

    #[test]
    fn unsupported_provider_is_invalid_request() {
        let err = match resolve_real_providers(Some("mock"), None, &[], &no_keys()) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn unknown_preferred_provider_is_invalid_request_in_multi_resolver() {
        let err = match resolve_available_providers(Some("mock"), None, &[], &no_keys()) {
            Ok(_) => panic!("mock provider must not resolve at runtime"),
            Err(err) => err,
        };
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn qualified_model_arg_passes_through_multi_resolver() {
        let (_, model) =
            resolve_available_providers(None, Some("ollama/llama3".to_owned()), &[], &no_keys())
                .expect("qualified model arg never requires a resolvable provider");
        assert_eq!(
            model.expect("qualified model arg yields Some").0,
            "ollama/llama3"
        );
    }

    #[test]
    fn no_providers_and_no_custom_specs_never_errors() {
        let (providers, model) = resolve_available_providers(None, None, &[], &no_keys())
            .expect("no providers configured must not error");
        if providers.ids().is_empty() {
            assert!(model.is_none());
        }
    }

    #[test]
    fn ollama_always_registers_in_multi_resolver_without_env() {
        let (providers, _) = resolve_available_providers(None, None, &[], &no_keys())
            .expect("available resolver must not fail");
        assert!(
            providers
                .ids()
                .iter()
                .any(|id| id.as_str() == OLLAMA_PROVIDER_ID),
            "ollama must register without OLLAMA_HOST/OLLAMA_MODEL so desktop \
             inline completion can use a local daemon alongside another chat provider"
        );
    }

    #[test]
    fn custom_spec_registers_in_multi_resolver() {
        let (providers, _) =
            match resolve_available_providers(None, None, &[spec("deepseek")], &no_keys()) {
                Ok(resolved) => resolved,
                Err(err) => panic!("custom provider should register: {err}"),
            };
        assert!(
            providers.ids().iter().any(|id| id.as_str() == "deepseek"),
            "registry should contain the custom id: {:?}",
            providers.ids()
        );
    }

    #[test]
    fn deepseek_builtin_has_correct_id_and_default_model() {
        let (provider, model) = build_deepseek("sk-test".to_owned(), None).expect("build");
        assert_eq!(provider.id().as_str(), DEEPSEEK_PROVIDER_ID);
        assert_eq!(model, DEEPSEEK_DEFAULT_MODEL);
    }

    #[test]
    fn deepseek_builtin_honors_model_override_and_ignores_blank() {
        let (_, model) = build_deepseek("sk-test".to_owned(), Some("deepseek-reasoner".to_owned()))
            .expect("build");
        assert_eq!(model, "deepseek-reasoner");
        let (_, model) =
            build_deepseek("sk-test".to_owned(), Some("   ".to_owned())).expect("build");
        assert_eq!(model, DEEPSEEK_DEFAULT_MODEL);
    }

    #[test]
    fn custom_deepseek_does_not_conflict_with_builtin() {
        let (providers, _) =
            resolve_available_providers(None, None, &[spec("deepseek")], &no_keys())
                .expect("custom deepseek must resolve without conflict");
        let deepseek_count = providers
            .ids()
            .iter()
            .filter(|id| id.as_str() == DEEPSEEK_PROVIDER_ID)
            .count();
        assert_eq!(
            deepseek_count,
            1,
            "exactly one deepseek: {:?}",
            providers.ids()
        );
    }

    #[test]
    fn preferred_custom_provider_sets_priority_and_default_model() {
        let (providers, model) = match resolve_available_providers(
            Some("deepseek"),
            None,
            &[spec("deepseek")],
            &no_keys(),
        ) {
            Ok(resolved) => resolved,
            Err(err) => panic!("preferred custom provider should resolve: {err}"),
        };
        assert_eq!(
            providers.ids().first().map(|id| id.as_str().to_owned()),
            Some("deepseek".to_owned())
        );
        assert_eq!(
            model.expect("preferred provider yields Some").0,
            "deepseek/test-chat"
        );
    }

    #[test]
    fn custom_default_model_falls_back_to_first_static_model() {
        let custom = CustomProviderSpec {
            default_model: None,
            models: vec![model_info("glm-4"), model_info("glm-4-air")],
            ..spec("glm")
        };
        let (_, model) = match resolve_available_providers(Some("glm"), None, &[custom], &no_keys())
        {
            Ok(resolved) => resolved,
            Err(err) => panic!("custom provider should resolve: {err}"),
        };
        assert_eq!(
            model.expect("preferred provider yields Some").0,
            "glm/glm-4"
        );
    }

    #[test]
    fn custom_spec_shadowing_a_builtin_is_rejected() {
        let err = match resolve_available_providers(None, None, &[spec("anthropic")], &no_keys()) {
            Ok(_) => panic!("builtin id collision must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(
            &err,
            EngineServiceError::CustomProviderConflict(id) if id == "anthropic"
        ));
        assert_eq!(err.to_engine_error().code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn custom_openai_does_not_conflict_with_builtin() {
        let (providers, _) = resolve_available_providers(None, None, &[spec("openai")], &no_keys())
            .expect("custom openai must resolve without conflict");
        let openai_count = providers
            .ids()
            .iter()
            .filter(|id| id.as_str() == OPENAI_PROVIDER_ID)
            .count();
        assert_eq!(openai_count, 1, "exactly one openai: {:?}", providers.ids());
    }

    #[test]
    fn single_provider_resolver_prefers_custom_openai_over_env() {
        let (providers, model) =
            resolve_real_providers(Some("openai"), None, &[spec("openai")], &no_keys())
                .expect("custom openai spec must resolve");
        assert_eq!(
            providers
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["openai".to_owned()]
        );
        assert_eq!(model.0, "openai/test-chat");
    }

    #[test]
    fn malformed_custom_id_is_rejected() {
        for bad in ["Deep-Seek", "deep/seek", "", "-deepseek"] {
            let err = match resolve_available_providers(None, None, &[spec(bad)], &no_keys()) {
                Ok(_) => panic!("id `{bad}` must be rejected"),
                Err(err) => err,
            };
            assert!(
                matches!(&err, EngineServiceError::CustomProviderInvalid { id, .. } if id == bad),
                "id `{bad}` should be CustomProviderInvalid, got: {err}"
            );
        }
    }

    #[test]
    fn empty_base_url_rejected_empty_key_allowed() {
        let no_key = CustomProviderSpec {
            api_key: "  ".to_owned(),
            ..spec("lmstudio")
        };
        let (registry, _) = resolve_available_providers(None, None, &[no_key], &no_keys())
            .expect("keyless spec registers");
        assert!(registry.ids().iter().any(|id| id.as_str() == "lmstudio"));

        let no_url = CustomProviderSpec {
            base_url: String::new(),
            ..spec("deepseek")
        };
        assert!(matches!(
            resolve_available_providers(None, None, &[no_url], &no_keys()),
            Err(EngineServiceError::CustomProviderInvalid { id, .. }) if id == "deepseek"
        ));
    }

    #[test]
    fn duplicate_custom_ids_are_rejected() {
        let err = match resolve_available_providers(
            None,
            None,
            &[spec("deepseek"), spec("deepseek")],
            &no_keys(),
        ) {
            Ok(_) => panic!("duplicate custom ids must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(
            &err,
            EngineServiceError::CustomProviderInvalid { id, .. } if id == "deepseek"
        ));
    }

    #[test]
    fn single_provider_resolver_builds_a_named_custom_spec() {
        let (providers, model) =
            match resolve_real_providers(Some("deepseek"), None, &[spec("deepseek")], &no_keys()) {
                Ok(resolved) => resolved,
                Err(err) => panic!("custom provider should resolve: {err}"),
            };
        assert_eq!(
            providers
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["deepseek".to_owned()]
        );
        assert_eq!(model.0, "deepseek/test-chat");
    }
}
