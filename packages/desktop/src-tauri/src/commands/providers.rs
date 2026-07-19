//! Provider config, profiles, and model listing.

use super::common::{parse_isolation, require_service};
use super::prelude::*;
use super::routines::respawn_cron_loop;

#[tauri::command]
pub async fn hello(state: State<'_, AppState>) -> DesktopResult<serde_json::Value> {
    let service = require_service(&state).await?;
    serde_json::to_value(service.hello()).map_err(|e| DesktopError::Message(e.to_string()))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn get_provider_config(state: State<'_, AppState>) -> DesktopResult<ProviderConfigView> {
    let cfg = state.config.lock().await;
    Ok(cfg.view())
}

/// Switch the secret storage backend (`"file"` | `"keychain"`), migrating
/// the master key from wherever it currently lives to the new backend (see
/// `config::set_secret_storage`/`secrets::SecretsStore::switch_mode`).
/// `secrets.enc` itself is untouched — only the key's location changes.
/// Returns the refreshed config view (which reports the new
/// `secretStorage` value) on success; on failure the old backend is left
/// intact and the config in `state` is unchanged.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn set_secret_storage(
    state: State<'_, AppState>,
    mode: String,
) -> DesktopResult<ProviderConfigView> {
    let target = SecretStorageMode::parse(mode.trim())
        .ok_or_else(|| DesktopError::Message(format!("unknown secret storage mode: {mode}")))?;
    let mut cfg = state.config.lock().await.clone();
    crate::config::set_secret_storage(&mut cfg, target)?;
    *state.config.lock().await = cfg.clone();
    Ok(cfg.view())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn list_builtin_providers() -> DesktopResult<Vec<BuiltinProvider>> {
    Ok(vec![
        BuiltinProvider::new("anthropic", "Anthropic", true),
        BuiltinProvider::new("openai", "OpenAI", true),
        BuiltinProvider::new("gemini", "Google Gemini", true),
        BuiltinProvider::new("deepseek", "DeepSeek", true),
        BuiltinProvider::new("openrouter", "OpenRouter", true),
        BuiltinProvider::new("groq", "Groq", true),
        BuiltinProvider::new("mistral", "Mistral", true),
        BuiltinProvider::new("xai", "xAI", true),
        BuiltinProvider::new("ollama", "Ollama", false),
        BuiltinProvider::new("bedrock", "Amazon Bedrock", true),
        // Copilot uses GitHub device-flow OAuth (or an existing editor
        // sign-in), not a required pasted API key.
        BuiltinProvider::new("copilot", "GitHub Copilot", false),
        // ChatGPT Plus/Pro subscription via Codex Responses OAuth.
        BuiltinProvider::new("chatgpt", "ChatGPT", false),
    ])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinProvider {
    pub id: String,
    pub label: String,
    pub requires_api_key: bool,
}

impl BuiltinProvider {
    fn new(id: &str, label: &str, requires_api_key: bool) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            requires_api_key,
        }
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn validate_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SaveProviderConfigInput,
) -> DesktopResult<Vec<ModelInfoDto>> {
    let mut trial = state.config.lock().await.clone();
    apply_save_input(&mut trial, &input)?;
    let service = build_service(&trial, state.store.clone(), app.clone())?;
    list_models_from(&service).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn save_provider_config(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SaveProviderConfigInput,
) -> DesktopResult<ProviderConfigView> {
    let mut cfg = state.config.lock().await.clone();
    apply_save_input(&mut cfg, &input)?;

    let service = build_service(&cfg, state.store.clone(), app.clone())?;
    let _ = list_models_from(&service).await?;

    persist_config(&cfg)?;
    *state.config.lock().await = cfg.clone();
    *state.service.lock().await = Some(service);
    respawn_cron_loop(&state).await;
    Ok(cfg.view())
}

// ---------------------------------------------------------------------------
// Named provider connections ("profiles"): the user can configure several
// named connections (e.g. two AWS accounts under different Bedrock keys/
// regions, plus "Anthropic direct") and switch which one is active. See
// `config::ProviderProfile`/`ProviderConfig::active_profile` for the model
// and `compose::build_service`, which reads the active profile as its sole
// source of provider/key/region/model/fallbacks. `get_provider_config`/
// `validate_provider`/`save_provider_config` above stay as thin adapters
// over the legacy top-level `prefs` fields (unchanged) so existing call
// sites keep working; these commands are the new profile-aware surface the
// Settings page's "Connections" section drives.
// ---------------------------------------------------------------------------

pub(crate) fn chatgpt_oauth_discoverable() -> bool {
    agentloop_sdk::providers::chatgpt::ChatgptConfig::discoverable()
}

pub(crate) fn profile_view(cfg: &ProviderConfig, profile: &ProviderProfile) -> ProviderProfileView {
    let has_key = cfg.profile_keys.contains_key(&profile.id)
        || (profile.provider == "copilot"
            && agentloop_sdk::providers::copilot::CopilotConfig::discoverable())
        || (profile.provider == "chatgpt" && chatgpt_oauth_discoverable());
    let is_active = cfg.prefs.active_profile_id.as_deref() == Some(profile.id.as_str());
    ProviderProfileView {
        id: profile.id.clone(),
        label: profile.label.clone(),
        provider: profile.provider.clone(),
        base_url: profile.base_url.clone(),
        region: profile.region.clone(),
        default_model: profile.default_model.clone(),
        fallback_models: profile.fallback_models.clone(),
        default_isolation: profile.default_isolation.clone(),
        has_key,
        is_active,
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn profiles_list(state: State<'_, AppState>) -> DesktopResult<Vec<ProviderProfileView>> {
    let cfg = state.config.lock().await;
    Ok(cfg
        .prefs
        .profiles
        .iter()
        .map(|p| profile_view(&cfg, p))
        .collect())
}

/// Turn a `ProviderProfileInput` into the `ProviderProfile` to persist,
/// validating shared fields. Does not touch the keychain — callers decide
/// whether to write `input.api_key` (empty/omitted means "keep existing").
pub(crate) fn build_profile(
    id: String,
    input: &ProviderProfileInput,
) -> DesktopResult<ProviderProfile> {
    let label = input.label.trim();
    if label.is_empty() {
        return Err(DesktopError::Message("connection name is required".into()));
    }
    let provider = input.provider.trim();
    if provider.is_empty() {
        return Err(DesktopError::Message("provider is required".into()));
    }
    let default_isolation = match input.default_isolation.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(trimmed) if parse_isolation(Some(trimmed)).is_some() => Some(trimmed.to_owned()),
        Some(trimmed) => {
            return Err(DesktopError::Message(format!(
                "unknown isolation policy: {trimmed}"
            )));
        }
    };
    Ok(ProviderProfile {
        id,
        label: label.to_owned(),
        provider: provider.to_owned(),
        base_url: input
            .base_url
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        region: input
            .region
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        default_model: input
            .default_model
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        fallback_models: input
            .fallback_models
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        default_isolation,
    })
}

pub(crate) fn new_profile_id(existing: &[ProviderProfile]) -> String {
    loop {
        let candidate = format!("profile-{}", uuid_like_suffix());
        if !existing.iter().any(|p| p.id == candidate) {
            return candidate;
        }
    }
}

/// Short random-ish suffix without pulling in a UUID dependency — collision
/// risk is a non-issue given `new_profile_id`'s existing-id check above.
pub(crate) fn uuid_like_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}")
}

/// Create or update a profile. `input.id` empty/`None` creates a new profile
/// (backend mints the id); a matching existing id updates it in place.
/// `input.api_key`: empty/omitted keeps the existing stored key (update) or
/// leaves the profile keyless (create — fine for providers like Ollama).
/// Does not activate or rebuild the engine service — call `profile_activate`
/// for that (mirrors the old save flow's explicit "Save & continue" step).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn profile_upsert(
    state: State<'_, AppState>,
    profile: ProviderProfileInput,
) -> DesktopResult<ProviderProfileView> {
    let mut cfg = state.config.lock().await.clone();

    let existing_id = profile
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|id| cfg.prefs.profiles.iter().any(|p| p.id == *id));

    let id = match existing_id {
        Some(id) => id.to_owned(),
        None => new_profile_id(&cfg.prefs.profiles),
    };

    let built = build_profile(id.clone(), &profile)?;

    if let Some(key) = profile
        .api_key
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cfg.profile_keys.insert(id.clone(), key.to_owned());
    }

    if let Some(pos) = cfg.prefs.profiles.iter().position(|p| p.id == id) {
        cfg.prefs.profiles[pos] = built;
    } else {
        cfg.prefs.profiles.push(built);
    }

    // First profile ever created becomes active automatically so a
    // brand-new install can go straight from "New connection" to a working
    // engine without a separate activate step.
    if cfg.prefs.active_profile_id.is_none() {
        cfg.prefs.active_profile_id = Some(id.clone());
    }

    persist_config(&cfg)?;
    let view = profile_view(
        &cfg,
        cfg.prefs
            .profiles
            .iter()
            .find(|p| p.id == id)
            .expect("just inserted"),
    );
    *state.config.lock().await = cfg;
    Ok(view)
}

/// Remove a profile. Errors if it's the active one — activate a different
/// profile first (mirrors why `delete_session` doesn't special-case "the
/// last session": here it's specifically the *active* one that's protected,
/// since removing it would leave the engine service pointed at a config
/// that's no longer there).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn profile_remove(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let mut cfg = state.config.lock().await.clone();
    let id = id.trim();
    if cfg.prefs.active_profile_id.as_deref() == Some(id) {
        return Err(DesktopError::Message(
            "cannot remove the active connection — activate another one first".into(),
        ));
    }
    if !cfg.prefs.profiles.iter().any(|p| p.id == id) {
        return Err(DesktopError::Message(format!("connection not found: {id}")));
    }
    cfg.prefs.profiles.retain(|p| p.id != id);
    cfg.profile_keys.remove(id);
    persist_config(&cfg)?;
    *state.config.lock().await = cfg;
    Ok(())
}

/// Activate a profile: persist the switch and rebuild the engine service
/// from it (mirrors `save_provider_config`'s persist-then-rebuild sequence).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn profile_activate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> DesktopResult<ProviderConfigView> {
    let mut cfg = state.config.lock().await.clone();
    let id = id.trim();
    if !cfg.prefs.profiles.iter().any(|p| p.id == id) {
        return Err(DesktopError::Message(format!("connection not found: {id}")));
    }
    cfg.prefs.active_profile_id = Some(id.to_owned());

    let service = build_service(&cfg, state.store.clone(), app.clone())?;

    persist_config(&cfg)?;
    *state.config.lock().await = cfg.clone();
    *state.service.lock().await = Some(service);
    respawn_cron_loop(&state).await;
    Ok(cfg.view())
}

/// Validate a connection using *exactly* the passed-in form values — the bug
/// this fixes: the old `validate_provider` path (still used by the legacy
/// single-config form) and, more fundamentally, `resolve_real_providers`'s
/// Bedrock arm (`packages/providers/crates/providers/src/resolve.rs`,
/// read-only from here) resolve credentials from the environment/stored
/// config rather than the freshly typed key, so pasting a new Bedrock key
/// and clicking Validate failed with "authentication missing for bedrock"
/// even though the key was right there in the form. This builds a one-off
/// trial profile from `input` (falling back to the *stored* key for this
/// profile id when `input.api_key` is empty, so re-validating an unchanged
/// saved connection doesn't demand re-pasting the key) and runs it through
/// the same `build_service` construction path `profile_activate` uses —
/// including the `with_bedrock_env` scoped-env workaround in `compose.rs`
/// that makes sure a client-supplied Bedrock key actually reaches the
/// provider instead of being dropped in favor of an unset env var.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn validate_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ProviderProfileInput,
) -> DesktopResult<Vec<ModelInfoDto>> {
    let mut cfg = state.config.lock().await.clone();

    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("__validate_trial__")
        .to_owned();

    let built = build_profile(id.clone(), &input)?;

    // Precedence: a freshly typed key always wins; otherwise fall back to
    // whatever is already stored for this profile id (re-validating an
    // existing connection without retyping the key). Copilot and Ollama
    // may validate without a profile key when credentials are discoverable
    // (editor/device-flow sign-in for Copilot; local host for Ollama).
    if let Some(key) = input
        .api_key
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cfg.profile_keys.insert(id.clone(), key.to_owned());
    } else if !(built.provider == "ollama"
        || cfg.profile_keys.contains_key(&id)
        || (built.provider == "copilot"
            && agentloop_sdk::providers::copilot::CopilotConfig::discoverable())
        || (built.provider == "chatgpt" && chatgpt_oauth_discoverable()))
    {
        return Err(DesktopError::Message(
            "API key is required for this provider".into(),
        ));
    }

    if let Some(pos) = cfg.prefs.profiles.iter().position(|p| p.id == id) {
        cfg.prefs.profiles[pos] = built;
    } else {
        cfg.prefs.profiles.push(built);
    }
    cfg.prefs.active_profile_id = Some(id);

    let service = build_service(&cfg, state.store.clone(), app.clone())?;
    list_models_from(&service).await
}

/// True when `provider_id` already has usable credentials — legacy
/// `cfg.keys`, an active/matching profile key, or oauth/ollama discovery.
/// Used by [`apply_save_input`] so plugins-only saves (Learning/Verifier
/// toggles) do not falsely demand a pasted API key when profiles hold it.
pub(crate) fn provider_credentials_present(cfg: &ProviderConfig, provider_id: &str) -> bool {
    if provider_id == "ollama" {
        return true;
    }
    if provider_id == "copilot" && agentloop_sdk::providers::copilot::CopilotConfig::discoverable()
    {
        return true;
    }
    if provider_id == "chatgpt" && chatgpt_oauth_discoverable() {
        return true;
    }
    if cfg.keys.contains_key(provider_id) {
        return true;
    }
    if let Some(profile) = cfg.active_profile() {
        if profile.provider == provider_id && cfg.profile_keys.contains_key(&profile.id) {
            return true;
        }
    }
    cfg.prefs
        .profiles
        .iter()
        .any(|p| p.provider == provider_id && cfg.profile_keys.contains_key(&p.id))
}

pub(crate) fn apply_save_input(
    cfg: &mut ProviderConfig,
    input: &SaveProviderConfigInput,
) -> DesktopResult<()> {
    let id = input.preferred_provider.trim();
    if id.is_empty() {
        return Err(DesktopError::Message("provider is required".into()));
    }
    cfg.prefs.preferred_provider = Some(id.to_owned());
    cfg.prefs.base_url = input
        .base_url
        .as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    cfg.prefs.region = input
        .region
        .as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    cfg.prefs.default_model = input
        .default_model
        .as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    // Working directory is chosen per session via the project picker — settings
    // no longer owns a default cwd. Leave any legacy prefs.cwd untouched.

    if let Some(plugins) = &input.plugins {
        cfg.prefs.plugins = plugins.clone();
    }
    if let Some(fallbacks) = &input.fallback_models {
        cfg.prefs.fallback_models = fallbacks
            .iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(iso) = &input.default_isolation {
        let trimmed = iso.trim();
        cfg.prefs.default_isolation = if trimmed.is_empty() {
            None
        } else if parse_isolation(Some(trimmed)).is_some() {
            Some(trimmed.to_owned())
        } else {
            return Err(DesktopError::Message(format!(
                "unknown isolation policy: {trimmed}"
            )));
        };
    }

    if let Some(key) = input
        .api_key
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cfg.keys.insert(id.to_owned(), key.to_owned());
    } else if !provider_credentials_present(cfg, id) {
        return Err(DesktopError::Message(
            "API key is required for this provider".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod apply_save_input_tests {
    use super::*;
    use crate::config::{PluginPrefs, ProviderConfig, ProviderPrefs, ProviderProfile};

    fn cfg_with_profile_key(provider: &str, key: &str) -> ProviderConfig {
        let mut cfg = ProviderConfig {
            prefs: ProviderPrefs {
                preferred_provider: Some(provider.to_owned()),
                active_profile_id: Some("p1".into()),
                profiles: vec![ProviderProfile {
                    id: "p1".into(),
                    label: "Test".into(),
                    provider: provider.to_owned(),
                    base_url: None,
                    region: None,
                    default_model: None,
                    fallback_models: None,
                    default_isolation: None,
                }],
                ..ProviderPrefs::default()
            },
            keys: Default::default(),
            profile_keys: Default::default(),
        };
        cfg.profile_keys.insert("p1".into(), key.to_owned());
        cfg
    }

    #[test]
    fn plugins_toggle_ok_when_only_profile_key_present() {
        let mut cfg = cfg_with_profile_key("anthropic", "sk-test");
        let input = SaveProviderConfigInput {
            preferred_provider: "anthropic".into(),
            api_key: None,
            base_url: None,
            region: None,
            default_model: None,
            cwd: None,
            plugins: Some(PluginPrefs {
                learning: true,
                ..PluginPrefs::default()
            }),
            fallback_models: None,
            default_isolation: None,
        };
        apply_save_input(&mut cfg, &input).expect("profile key should satisfy gate");
        assert!(cfg.prefs.plugins.learning);
    }

    #[test]
    fn plugins_toggle_err_without_any_credentials() {
        let mut cfg = ProviderConfig {
            prefs: ProviderPrefs {
                preferred_provider: Some("anthropic".into()),
                ..ProviderPrefs::default()
            },
            keys: Default::default(),
            profile_keys: Default::default(),
        };
        let input = SaveProviderConfigInput {
            preferred_provider: "anthropic".into(),
            api_key: None,
            base_url: None,
            region: None,
            default_model: None,
            cwd: None,
            plugins: Some(PluginPrefs {
                verifier: true,
                ..PluginPrefs::default()
            }),
            fallback_models: None,
            default_isolation: None,
        };
        let err = apply_save_input(&mut cfg, &input).unwrap_err();
        assert!(
            err.to_string().contains("API key is required"),
            "unexpected: {err}"
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfoDto {
    pub id: String,
    pub display_name: Option<String>,
    pub provider_id: String,
    pub context_window: Option<u32>,
}

pub(crate) async fn list_models_from(service: &EngineService) -> DesktopResult<Vec<ModelInfoDto>> {
    let registry = service.provider_registry();
    let mut out = Vec::new();
    for pid in registry.ids() {
        let Some(provider) = registry.get(&pid) else {
            continue;
        };
        match provider.list_models().await {
            Ok(models) => {
                for m in models {
                    out.push(ModelInfoDto {
                        id: format!("{}/{}", pid.as_str(), m.id),
                        display_name: m.display_name.or(Some(m.id.clone())),
                        provider_id: pid.as_str().to_owned(),
                        context_window: m.context_window,
                    });
                }
            }
            Err(err) => {
                tracing::warn!(provider = %pid, error = %err, "list_models failed");
            }
        }
    }
    if out.is_empty() {
        return Err(DesktopError::Message(
            "could not list models — check host and API key".into(),
        ));
    }
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> DesktopResult<Vec<ModelInfoDto>> {
    let service = require_service(&state).await?;
    list_models_from(&service).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> DesktopResult<Vec<String>> {
    let service = require_service(&state).await?;
    Ok(service
        .provider_registry()
        .ids()
        .into_iter()
        .map(|id| id.as_str().to_owned())
        .collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn is_configured(state: State<'_, AppState>) -> DesktopResult<bool> {
    let cfg = state.config.lock().await;
    let has_service = state.service.lock().await.is_some();
    Ok(cfg.is_ready() && has_service)
}
