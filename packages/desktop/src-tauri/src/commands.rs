//! Tauri commands — thin wrappers over `EngineService` + keychain config.

use std::path::PathBuf;

use std::sync::Arc;

use agentloop_channel::{RoutineSpec, RoutineStore, RoutineTrigger};
use agentloop_contracts::{
    Answer, BlobSource, CommandInfo, ContentBlock, Effort, GoalSpec, IntegrationOutcome,
    IsolationPolicy, ModelRef, NewSessionParams, PermissionDecision, PermissionMode,
    PermissionRequestId, PromptInput, QuestionId, SessionEvent, SessionId, SessionMeta,
    SessionMetaPatch, TurnOptions, TurnSummary,
};
use agentloop_core::{BackgroundEntrySummary, WorkspaceStatus};
use agentloop_sdk::EngineService;
use agentloop_sdk::mcp::McpToolClient;
use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner, default_routines_dir};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::compose::build_service;
use crate::config::{
    ProviderConfig, ProviderConfigView, ProviderProfile, ProviderProfileInput,
    ProviderProfileView, SaveProviderConfigInput, persist_config,
};
use crate::error::{DesktopError, DesktopResult};
use crate::secrets::SecretStorageMode;
use crate::state::AppState;

async fn require_service(state: &AppState) -> DesktopResult<EngineService> {
    state
        .service
        .lock()
        .await
        .clone()
        .ok_or(DesktopError::NotConfigured)
}

#[tauri::command]
pub async fn hello(state: State<'_, AppState>) -> DesktopResult<serde_json::Value> {
    let service = require_service(&state).await?;
    serde_json::to_value(service.hello()).map_err(|e| DesktopError::Message(e.to_string()))
}

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
        BuiltinProvider::new("copilot", "GitHub Copilot", true),
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

#[tauri::command]
pub async fn validate_provider(
    state: State<'_, AppState>,
    input: SaveProviderConfigInput,
) -> DesktopResult<Vec<ModelInfoDto>> {
    let mut trial = state.config.lock().await.clone();
    apply_save_input(&mut trial, &input)?;
    let service = build_service(&trial, state.store.clone())?;
    list_models_from(&service).await
}

#[tauri::command]
pub async fn save_provider_config(
    state: State<'_, AppState>,
    input: SaveProviderConfigInput,
) -> DesktopResult<ProviderConfigView> {
    let mut cfg = state.config.lock().await.clone();
    apply_save_input(&mut cfg, &input)?;

    let service = build_service(&cfg, state.store.clone())?;
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

fn profile_view(cfg: &ProviderConfig, profile: &ProviderProfile) -> ProviderProfileView {
    let has_key = cfg.profile_keys.contains_key(&profile.id);
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
fn build_profile(id: String, input: &ProviderProfileInput) -> DesktopResult<ProviderProfile> {
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

fn new_profile_id(existing: &[ProviderProfile]) -> String {
    loop {
        let candidate = format!("profile-{}", uuid_like_suffix());
        if !existing.iter().any(|p| p.id == candidate) {
            return candidate;
        }
    }
}

/// Short random-ish suffix without pulling in a UUID dependency — collision
/// risk is a non-issue given `new_profile_id`'s existing-id check above.
fn uuid_like_suffix() -> String {
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
#[tauri::command]
pub async fn profile_activate(
    state: State<'_, AppState>,
    id: String,
) -> DesktopResult<ProviderConfigView> {
    let mut cfg = state.config.lock().await.clone();
    let id = id.trim();
    if !cfg.prefs.profiles.iter().any(|p| p.id == id) {
        return Err(DesktopError::Message(format!("connection not found: {id}")));
    }
    cfg.prefs.active_profile_id = Some(id.to_owned());

    let service = build_service(&cfg, state.store.clone())?;

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
#[tauri::command]
pub async fn validate_profile(
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
    // existing connection without retyping the key).
    if let Some(key) = input
        .api_key
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cfg.profile_keys.insert(id.clone(), key.to_owned());
    } else if built.provider != "ollama" && !cfg.profile_keys.contains_key(&id) {
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

    let service = build_service(&cfg, state.store.clone())?;
    list_models_from(&service).await
}

fn parse_isolation(raw: Option<&str>) -> Option<IsolationPolicy> {
    match raw? {
        "never" => Some(IsolationPolicy::Never),
        "optional" => Some(IsolationPolicy::Optional),
        "required" => Some(IsolationPolicy::Required),
        _ => None,
    }
}

fn apply_save_input(cfg: &mut ProviderConfig, input: &SaveProviderConfigInput) -> DesktopResult<()> {
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
    } else if id != "ollama" && !cfg.keys.contains_key(id) {
        return Err(DesktopError::Message(
            "API key is required for this provider".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfoDto {
    pub id: String,
    pub display_name: Option<String>,
    pub provider_id: String,
    pub context_window: Option<u32>,
}

async fn list_models_from(service: &EngineService) -> DesktopResult<Vec<ModelInfoDto>> {
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

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> DesktopResult<Vec<ModelInfoDto>> {
    let service = require_service(&state).await?;
    list_models_from(&service).await
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    /// `never` | `optional` | `required` — falls back to prefs.default_isolation.
    pub isolation: Option<String>,
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    input: CreateSessionInput,
) -> DesktopResult<SessionMeta> {
    let cfg = state.config.lock().await.clone();
    let model = input
        .model
        .or(cfg.prefs.default_model.clone())
        .map(ModelRef);
    let cwd = input
        .cwd
        .map(PathBuf::from)
        .or_else(|| cfg.prefs.cwd.as_ref().map(PathBuf::from));
    let isolation = parse_isolation(input.isolation.as_deref())
        .or_else(|| parse_isolation(cfg.prefs.default_isolation.as_deref()));

    let service = require_service(&state).await?;
    let id = service
        .create_session(NewSessionParams {
            title: input.title,
            model,
            cwd,
            isolation,
            ..NewSessionParams::default()
        })
        .await?;
    let meta = service.session_meta(&id).await?;

    // Only non-isolated sessions need a baseline: isolated sessions get a
    // clean private worktree, so their `git_status` is already scoped to
    // this session's own changes. Non-fatal on any git failure — the
    // Changes panel just falls back to the full-repo view for this session.
    if meta.base_cwd.is_none() {
        if let Some(baseline) = capture_session_baseline(&meta.cwd) {
            state
                .session_baselines
                .lock()
                .await
                .insert(id.to_string(), baseline);
        } else {
            tracing::warn!(
                session_id = %id,
                cwd = %meta.cwd.display(),
                "failed to capture session baseline; Changes panel will show full repo status"
            );
        }
    }

    Ok(meta)
}

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> DesktopResult<Vec<SessionMeta>> {
    let service = require_service(&state).await?;
    Ok(service.list_sessions().await?)
}

#[tauri::command]
pub async fn session_meta(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<SessionMeta> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.session_meta(&id).await?)
}

#[tauri::command]
pub async fn resume_session(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let result = match service.resume_session(&id).await {
        Ok(()) => Ok(()),
        Err(err) => {
            // The engine can't always distinguish "workspace/cwd is gone" from
            // other launch failures (e.g. a delegated agent's process spawn
            // just returns an OS error string). Check the persisted cwd
            // ourselves so the sidebar can show something actionable instead
            // of a raw "No such file or directory (os error 2)".
            if let Ok(meta) = service.session_meta(&id).await {
                if !meta.cwd.exists() {
                    return Err(DesktopError::Message(format!(
                        "workspace missing: {} ({err})",
                        meta.cwd.display()
                    )));
                }
            }
            Err(DesktopError::from(err))
        }
    };

    // Backfill a session baseline on resume if one wasn't captured at
    // creation time (sessions created before this feature shipped, or where
    // the app restarted between `create_session` and now). This means the
    // Changes panel scopes to "since this resume" rather than "since the
    // session was created" for those sessions — still strictly better than
    // showing the whole repo's pre-existing dirty state. Non-isolated
    // sessions only; isolated sessions don't need a baseline at all.
    if result.is_ok() {
        let has_baseline = state
            .session_baselines
            .lock()
            .await
            .contains_key(id.as_str());
        if !has_baseline {
            if let Ok(meta) = service.session_meta(&id).await {
                if meta.base_cwd.is_none() {
                    if let Some(baseline) = capture_session_baseline(&meta.cwd) {
                        state
                            .session_baselines
                            .lock()
                            .await
                            .insert(id.to_string(), baseline);
                    }
                }
            }
        }
    }

    result
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionInput {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
}

#[tauri::command]
pub async fn update_session(
    state: State<'_, AppState>,
    session_id: String,
    patch: UpdateSessionInput,
) -> DesktopResult<SessionMeta> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service
        .update_session(
            &id,
            SessionMetaPatch {
                title: patch.title,
                model: patch.model.map(ModelRef),
                cwd: patch.cwd.map(PathBuf::from),
                ..Default::default()
            },
        )
        .await?)
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let id = SessionId::from(session_id);
    if let Some(handle) = state.subscriptions.lock().await.remove(id.as_str()) {
        handle.abort();
    }
    let service = require_service(&state).await?;
    Ok(service.delete_session(&id).await?)
}

#[tauri::command]
pub async fn replay(
    state: State<'_, AppState>,
    session_id: String,
    from_seq: Option<u64>,
) -> DesktopResult<Vec<SessionEvent>> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.replay(&id, from_seq.unwrap_or(0)).await?)
}

#[tauri::command]
pub async fn subscribe_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    let id = SessionId::from(session_id.clone());

    if let Some(handle) = state.subscriptions.lock().await.remove(&session_id) {
        handle.abort();
    }

    let service = require_service(&state).await?;
    let stream = service.subscribe(&id)?;

    let key = session_id.clone();
    let handle = tokio::spawn(async move {
        let mut stream = stream;
        while let Some(event) = stream.next().await {
            if app.emit("session-event", &event).is_err() {
                break;
            }
        }
    });

    state.subscriptions.lock().await.insert(key, handle);
    Ok(())
}

#[tauri::command]
pub async fn unsubscribe_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    if let Some(handle) = state.subscriptions.lock().await.remove(&session_id) {
        handle.abort();
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAttachment {
    pub path: String,
    pub kind: String,
    pub name: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCommandInput {
    pub session_id: String,
    pub text: String,
    pub model: Option<String>,
    /// Maps composer mode → engine `PermissionMode` (`plan` / `default` / …).
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub attachments: Vec<PromptAttachment>,
    /// `Effort` enum's serde wire value ("low" | "medium" | "high" | "xhigh" |
    /// "max"). Invalid/unrecognized values parse to `None` (engine default)
    /// rather than erroring — see `parse_effort`.
    #[serde(default)]
    pub effort: Option<String>,
    /// The composer mode picked in the UI ("agent" | "plan" | "ask" | "flex"),
    /// distinct from `permission_mode` (its derived wire value — see
    /// `ModePicker.tsx::modeToPermission`). Only `"flex"` currently changes
    /// backend behavior: it appends the orchestrator system prompt below.
    #[serde(default)]
    pub composer_mode: Option<String>,
}

fn parse_effort(raw: Option<&str>) -> Option<Effort> {
    match raw? {
        "low" => Some(Effort::Low),
        "medium" => Some(Effort::Medium),
        "high" => Some(Effort::High),
        "xhigh" => Some(Effort::XHigh),
        "max" => Some(Effort::Max),
        _ => None,
    }
}

fn parse_permission_mode(raw: Option<&str>) -> Option<PermissionMode> {
    match raw? {
        "default" => Some(PermissionMode::Default),
        "accept_edits" | "acceptEdits" => Some(PermissionMode::AcceptEdits),
        "plan" => Some(PermissionMode::Plan),
        "dont_ask" | "dontAsk" => Some(PermissionMode::DontAsk),
        "bypass_permissions" | "bypassPermissions" => Some(PermissionMode::BypassPermissions),
        _ => None,
    }
}

fn guess_media_type(path: &str, kind: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match (kind, ext.as_str()) {
        ("image", "png") => "image/png".into(),
        ("image", "jpg" | "jpeg") => "image/jpeg".into(),
        ("image", "gif") => "image/gif".into(),
        ("image", "webp") => "image/webp".into(),
        ("image", _) => "image/png".into(),
        (_, "pdf") => "application/pdf".into(),
        (_, "md") => "text/markdown".into(),
        (_, "json") => "application/json".into(),
        (_, "ts" | "tsx" | "js" | "jsx") => "text/plain".into(),
        (_, "rs") => "text/plain".into(),
        (_, "txt") => "text/plain".into(),
        _ => "application/octet-stream".into(),
    }
}

fn build_prompt_input(input: &PromptCommandInput) -> PromptInput {
    let mut parts = Vec::new();
    let text = input.text.trim();
    if !text.is_empty() {
        parts.push(ContentBlock::markdown(text));
    }
    for att in &input.attachments {
        let path = PathBuf::from(&att.path);
        let name = att
            .name
            .clone()
            .or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "attachment".into());
        let media_type = att
            .media_type
            .clone()
            .unwrap_or_else(|| guess_media_type(&att.path, &att.kind));
        let data = BlobSource::Path { path };
        if att.kind == "image" {
            parts.push(ContentBlock::Image { media_type, data });
        } else {
            parts.push(ContentBlock::File {
                name,
                media_type,
                data,
            });
        }
    }
    if parts.is_empty() {
        return PromptInput::text("");
    }
    PromptInput {
        parts,
        command: None,
    }
}

/// Character budget for the per-project memory section injected into every
/// turn's system prompt (see `prompt` below). Deliberately modest — unlike
/// the one-shot global memory load, this rides every turn, so it uses an
/// explicit cap well under `agentloop_prompts::DEFAULT_MEMORY_BUDGET_CHARS`
/// rather than `0` (which would mean "use the 8k default").
const PROJECT_MEMORY_PROMPT_BUDGET_CHARS: usize = 4_000;

/// System prompt appended for the Flex composer mode, instructing the model
/// to act as an orchestrator over the `planner` / `plan-reviewer` /
/// `flex-worker` roles registered in `compose.rs::flex_composer_roles`. The
/// model runs with `PermissionMode::DontAsk` in this mode (see
/// `ModePicker.tsx::modeToPermission`), so it — and every subagent it
/// spawns, which inherit the parent's permission mode — must never leave a
/// permission ask pending.
const FLEX_ORCHESTRATOR_PROMPT: &str = "\
You are an orchestrator. First classify the task:
- SIMPLE (single-file change, question, quick fix): do it yourself directly, no subagents.
- COMPLEX (multi-file feature, refactor, \"build X\"): orchestrate as below.

PLAN: if you are a top-tier model, draft the plan yourself; otherwise spawn \
Agent(role=planner, model=<top tier>) with the full task. The planner may \
spawn its own read-only context gatherers.

REVIEW: send the plan to Verify (or Agent role=plan-reviewer) using a \
DIFFERENT model than the planner, passing ONLY the task statement plus the \
plan text — nothing else. If REJECTED: revise with the planner, addressing \
every numbered objection. Hard limit: 3 revision cycles. After the 3rd \
rejection, stop and present both the plan and the objections to the user \
for a decision — do not keep revising past that point.

EXECUTE: once the plan is APPROVED, split it into independent steps and \
spawn flex-worker agents (each gets an isolated worktree, merged back \
automatically) with COMPLETE, self-contained prompts — the step, the \
relevant file paths, and the verification commands to run. Run independent \
steps in parallel, up to 8 at a time.

MERGE/VERIFY: after workers finish, review the integration results, run the \
project's verification commands, and summarize what changed.

Model tiers: pick the top-tier planner from the models available to you by \
name (opus/sol/terra/o1-class names are top tier; sonnet/grok/gpt-class names \
are middle tier). Always use the full `provider/model` id from your model \
list when overriding a subagent's model.

You run with DontAsk permissions, and every subagent you spawn inherits \
that: never leave a permission ask pending, and never block waiting on one.";

#[tauri::command]
pub async fn prompt(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id.clone());
    let meta = service.session_meta(&id).await.ok();
    let cwd_notice = meta.as_ref().map(|meta| {
        format!(
            "Session working directory: {}. This is the working directory for all relative paths and where the user's project lives; it supersedes any 'Primary working directory' stated earlier in this prompt.",
            meta.cwd.display()
        )
    });
    // Purge expired memories (global + this session's project dir) before
    // assembling the system prompt, so a memory that expired since the
    // engine's one-shot global load at startup — or one that was never
    // surfaced via the Memory page's self-cleaning `*_list` calls — never
    // rides into the model's context on this turn. The global load itself
    // happened once at engine construction and can't be re-run per turn
    // (that path is engine-side, read-only from here), but deleting the file
    // now stops it from persisting into any future turn/session.
    if let Ok(dir) = memory_dir() {
        purge_expired_memories(&dir);
    }
    if let Some(meta) = meta.as_ref() {
        purge_expired_memories(&meta.cwd.join(".agent").join("memory"));
    }
    let project_memory = meta.as_ref().and_then(|meta| {
        agentloop_prompts::load_memory_section(&agentloop_prompts::MemoryConfig {
            dir: Some(meta.cwd.join(".agent").join("memory")),
            budget_chars: PROJECT_MEMORY_PROMPT_BUDGET_CHARS,
        })
    });
    let mut system_append = match (cwd_notice, project_memory) {
        (Some(a), Some(b)) => Some(format!("{a}\n\n{b}")),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if input.composer_mode.as_deref() == Some("flex") {
        system_append = Some(match system_append {
            Some(existing) => format!("{existing}\n\n{FLEX_ORCHESTRATOR_PROMPT}"),
            None => FLEX_ORCHESTRATOR_PROMPT.to_owned(),
        });
    }
    let opts = TurnOptions {
        model: input.model.clone().map(ModelRef),
        permission_mode: parse_permission_mode(input.permission_mode.as_deref()),
        system_append,
        effort: parse_effort(input.effort.as_deref()),
        ..TurnOptions::default()
    };
    let prompt_input = build_prompt_input(&input);
    Ok(service.prompt(&id, prompt_input, opts).await?)
}

#[tauri::command]
pub async fn cancel(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.cancel(&id).await?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundProcessDto {
    pub process_id: String,
    pub command: Option<String>,
    pub running: bool,
    pub started_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
}

impl From<BackgroundEntrySummary> for BackgroundProcessDto {
    fn from(entry: BackgroundEntrySummary) -> Self {
        Self {
            process_id: entry.id,
            command: Some(entry.command),
            running: entry.running,
            started_at_ms: Some(entry.started_at_ms),
            exit_code: entry.exit_code,
        }
    }
}

/// List background processes (started via `Bash`'s `run_in_background`) for
/// a session, for a "background processes" panel. Thin pass-through to
/// `EngineService::background_list`, which itself proxies to
/// `BackgroundProcessRegistry::list` (`packages/engine/crates/core/src/executor.rs`).
#[tauri::command]
pub async fn background_list(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<Vec<BackgroundProcessDto>> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service
        .background_list(&id)
        .into_iter()
        .map(BackgroundProcessDto::from)
        .collect())
}

/// Kill one background process by id, for the Stop button on a running
/// background-process row. Thin pass-through to
/// `EngineService::background_kill`; a `false` result (unknown id — already
/// reaped or never existed) is not an error, so it's swallowed here rather
/// than surfaced as one.
#[tauri::command]
pub async fn background_kill(
    state: State<'_, AppState>,
    session_id: String,
    process_id: String,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let _ = service.background_kill(&id, &process_id).await?;
    Ok(())
}

/// Ask a still-running **foreground** shell call to move to the background
/// (see `MOVE-TO-BACKGROUND`): the "Move to background" affordance on a
/// running shell row in `ToolStepGroup`. Thin pass-through to
/// `EngineService::background_demote`. Returns `false` — not an error — when
/// there's nothing to do: the call already finished, the id is unknown, or
/// the session's execution backend doesn't support demote (only the local
/// backend does; docker/ssh sessions get no visible effect). The caller
/// should treat `false` the same as `true` from the user's perspective —
/// silently do nothing rather than show an error, since "the command already
/// finished" is not exceptional.
#[tauri::command]
pub async fn background_demote(
    state: State<'_, AppState>,
    session_id: String,
    call_id: String,
) -> DesktopResult<bool> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.background_demote(&id, &call_id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondPermissionInput {
    pub session_id: String,
    pub request_id: String,
    pub decision: String,
    pub reason: Option<String>,
}

#[tauri::command]
pub async fn respond_permission(
    state: State<'_, AppState>,
    input: RespondPermissionInput,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id);
    let request_id = PermissionRequestId::from(input.request_id);
    let decision = match input.decision.as_str() {
        "allow_once" | "allowOnce" => PermissionDecision::AllowOnce,
        "allow_always" | "allowAlways" => PermissionDecision::AllowAlways,
        "deny" => PermissionDecision::Deny {
            reason: input.reason,
        },
        other => {
            return Err(DesktopError::Message(format!(
                "unknown permission decision: {other}"
            )));
        }
    };
    Ok(service
        .respond_permission(&id, request_id, decision)
        .await?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondQuestionInput {
    pub session_id: String,
    pub request_id: String,
    pub answers: Vec<AnswerDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerDto {
    pub question: String,
    pub selected: Vec<String>,
}

#[tauri::command]
pub async fn respond_question(
    state: State<'_, AppState>,
    input: RespondQuestionInput,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id);
    let request_id = QuestionId::from(input.request_id);
    let answers: Vec<Answer> = input
        .answers
        .into_iter()
        .map(|a| Answer {
            question: a.question,
            selected: a.selected,
        })
        .collect();
    Ok(service.respond_question(&id, request_id, answers).await?)
}

#[tauri::command]
pub async fn is_configured(state: State<'_, AppState>) -> DesktopResult<bool> {
    let cfg = state.config.lock().await;
    let has_service = state.service.lock().await.is_some();
    Ok(cfg.is_ready() && has_service)
}

/// Whether `cwd` is inside a git repository at all (`git rev-parse
/// --git-dir` succeeds), regardless of whether it has any commits yet. Used
/// to gate the entire git chrome (branch pill, changes badge, commit bar,
/// Changes tab content) — a non-git folder should show none of it, while a
/// freshly `git init`-ed repo with an unborn HEAD legitimately keeps it (see
/// `current_head_sha`'s empty-string-HEAD handling, which already treats
/// "no commits yet" as a normal state rather than an error).
#[tauri::command]
pub fn git_is_repo(cwd: String) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Read-only current-branch lookup for the composer context bar.
#[tauri::command]
pub fn git_branch(cwd: String) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

/// Local branch names for the branch picker (`git branch --format`).
#[tauri::command]
pub fn git_list_branches(cwd: String) -> DesktopResult<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git list branches failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git list branches failed".into()
        } else {
            stderr
        }));
    }
    let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

/// Check out a local branch in the session cwd.
#[tauri::command]
pub fn git_checkout(cwd: String, branch: String) -> DesktopResult<()> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(DesktopError::Message("invalid branch name".into()));
    }
    let output = std::process::Command::new("git")
        .args(["checkout", branch])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git checkout failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            format!("git checkout {branch} failed")
        } else {
            stderr
        }));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    /// Path relative to `cwd` (rename keeps the new path).
    pub path: String,
    /// Porcelain letter: "M" | "A" | "D" | "R" | "?" (untracked) | other.
    pub status: String,
    /// Lines added per `git diff --numstat HEAD`; None for binary/untracked.
    pub added: Option<u32>,
    /// Lines removed; None for binary/untracked.
    pub removed: Option<u32>,
}

/// Read-only working-tree status for the Changes panel. Non-git dirs yield
/// an empty list (mirrors `git_branch`'s tolerance).
#[tauri::command]
pub fn git_status(cwd: String) -> DesktopResult<Vec<GitFileStatus>> {
    git_status_full(&cwd)
}

/// Shared implementation behind [`git_status`] and
/// [`git_status_since_baseline`]'s full-repo fallback path.
fn git_status_full(cwd: &str) -> DesktopResult<Vec<GitFileStatus>> {
    let porcelain = match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).to_string()
        }
        _ => return Ok(Vec::new()),
    };

    // Line counts per changed file; binary files report "-" and are skipped.
    let mut counts: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    if let Ok(out) = std::process::Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(cwd)
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let mut parts = line.split('\t');
                let (Some(a), Some(r), Some(path)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                if let (Ok(a), Ok(r)) = (a.parse::<u32>(), r.parse::<u32>()) {
                    // Renames appear as "old => new" or "{old => new}/tail".
                    let path = path
                        .rsplit(" => ")
                        .next()
                        .unwrap_or(path)
                        .trim_end_matches('}')
                        .to_string();
                    counts.insert(path, (a, r));
                }
            }
        }
    }

    let mut files = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let mut path = line[3..].trim().to_string();
        // Rename lines: "R  old -> new" — keep the new path.
        if let Some((_, new)) = path.split_once(" -> ") {
            path = new.trim().to_string();
        }
        // Strip porcelain quoting for paths with special characters.
        if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            path = path[1..path.len() - 1].to_string();
        }
        let status = if code == "??" {
            "?".to_string()
        } else {
            code.trim()
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "M".to_string())
        };
        let (added, removed) = counts
            .get(&path)
            .map(|&(a, r)| (Some(a), Some(r)))
            .unwrap_or((None, None));
        files.push(GitFileStatus {
            path,
            status,
            added,
            removed,
        });
    }
    Ok(files)
}

/// Working-tree status scoped to what this session has actually touched,
/// for the Changes panel. Falls back to the full-repo [`git_status`] result
/// (unchanged shape) whenever session-scoping isn't possible or safe:
///
/// - Isolated sessions (`base_cwd.is_some()`) already run in a private
///   worktree, so the plain status is already session-scoped.
/// - No baseline was captured for this session (e.g. it predates this
///   feature, or the app restarted between creation and baseline capture
///   and `resume_session` hasn't run yet).
/// - The repo's HEAD has moved since the baseline was captured (a commit,
///   checkout, etc. invalidates the recorded content hashes' meaning).
///
/// Otherwise, a dirty path from the current `git status` is kept only if it
/// wasn't dirty at baseline time, or its content hash has changed since.
#[tauri::command]
pub async fn git_status_since_baseline(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<Vec<GitFileStatus>> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    let cwd_str = cwd.to_string_lossy().to_string();

    if base_cwd.is_some() {
        return git_status_full(&cwd_str);
    }

    let baseline = {
        let baselines = state.session_baselines.lock().await;
        baselines
            .get(&session_id)
            .map(|b| (b.head_sha.clone(), b.files.clone()))
    };
    let Some((baseline_head, baseline_files)) = baseline else {
        return git_status_full(&cwd_str);
    };

    if current_head_sha(&cwd) != baseline_head {
        return git_status_full(&cwd_str);
    }

    let all = git_status_full(&cwd_str)?;
    let filtered = all
        .into_iter()
        .filter(|f| match baseline_files.get(&f.path) {
            None => true,
            // Untracked dir already recorded at baseline time (see the "dir"
            // sentinel in `capture_session_baseline`) — there's no blob to
            // hash for a directory, and an already-untracked dir isn't a
            // session change, so it's always filtered out regardless of what
            // may have changed inside it (mirrors git's own porcelain
            // granularity, which also collapses to the single dir entry).
            Some(baseline_hash) if baseline_hash == "dir" => false,
            Some(baseline_hash) => {
                let current_hash =
                    hash_object(&cwd, &f.path).unwrap_or_else(|| "deleted".to_string());
                &current_hash != baseline_hash
            }
        })
        .collect();
    Ok(filtered)
}

/// `git hash-object <path>` relative to `cwd`; used to detect whether a
/// dirty path's content has changed since baseline capture. Returns `None`
/// on any git failure (missing file, not a git repo, etc.) so callers can
/// treat the path as "unknown" rather than failing outright.
fn hash_object(cwd: &std::path::Path, path: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["hash-object", path])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git rev-parse HEAD` in `cwd`; empty string if there is no HEAD yet
/// (e.g. a freshly initialized repo with no commits) rather than an error,
/// since that's a legitimate baseline state.
fn current_head_sha(cwd: &std::path::Path) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Capture a [`crate::state::SessionBaseline`] snapshot of `cwd`'s dirty
/// state, for scoping the Changes panel to this session's own edits. Only
/// meaningful for non-isolated sessions (isolated sessions already get a
/// clean worktree, so their `git_status` is inherently session-scoped).
///
/// Non-fatal by design: any git failure (not a repo, git missing, etc.)
/// simply yields no baseline, and `git_status_since_baseline` gracefully
/// degrades to the full-repo `git_status` in that case.
fn capture_session_baseline(cwd: &std::path::Path) -> Option<crate::state::SessionBaseline> {
    let porcelain = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !porcelain.status.success() {
        return None;
    }
    let head_sha = current_head_sha(cwd);

    let mut files = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&porcelain.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let mut path = line[3..].trim().to_string();
        if let Some((_, new)) = path.split_once(" -> ") {
            path = new.trim().to_string();
        }
        if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            path = path[1..path.len() - 1].to_string();
        }
        // Untracked dirs are reported by porcelain as a single "dir/" entry
        // with no single blob to hash. Record them with the "dir" sentinel
        // instead of skipping them outright: skipping meant an
        // already-untracked dir at baseline time was absent from
        // `baseline.files`, so `git_status_since_baseline`'s filter (`None`
        // => "not in baseline" => keep) treated it as a brand-new session
        // change — the "phantom session changes" bug. With the sentinel
        // recorded, that same dir entry is now `Some("dir")` in the filter
        // and gets correctly dropped as pre-existing. A dir newly created
        // during the session still has no baseline entry at all, so it's
        // still kept. Note: files added inside an already-untracked dir stay
        // collapsed under the single dir entry by porcelain itself (git's
        // own display has the same granularity) — acceptable.
        if path.ends_with('/') {
            files.insert(path, "dir".to_string());
            continue;
        }
        let is_deleted = code.contains('D');
        let hash = if is_deleted {
            "deleted".to_string()
        } else {
            match hash_object(cwd, &path) {
                Some(h) => h,
                None => "deleted".to_string(),
            }
        };
        files.insert(path, hash);
    }

    Some(crate::state::SessionBaseline { head_sha, files })
}

const MAX_DIFF_BYTES: usize = 200 * 1024;

/// Truncate `text` to `MAX_DIFF_BYTES` at a char boundary, appending a marker
/// so callers can tell the diff was cut short. Shared by all diff commands.
fn truncate_diff(mut text: String) -> String {
    if text.len() > MAX_DIFF_BYTES {
        let mut cut = MAX_DIFF_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… diff truncated …\n");
    }
    text
}

/// `git diff <rev> -- <path>` in `dir`, falling back to a `--no-index` diff
/// against `/dev/null` when the file has no history against `rev` (i.e. it's
/// untracked there). Shared by `git_diff` and `review_file_diff`.
fn diff_against_rev(dir: &std::path::Path, rev: &str, path: &str) -> DesktopResult<String> {
    let tracked = std::process::Command::new("git")
        .args(["diff", rev, "--", path])
        .current_dir(dir)
        .output()
        .map_err(|e| DesktopError::Message(format!("git diff failed: {e}")))?;

    let mut text = if tracked.status.success() {
        String::from_utf8_lossy(&tracked.stdout).to_string()
    } else {
        String::new()
    };

    if text.trim().is_empty() {
        // Untracked file: diff against /dev/null (exit code 1 means "differs",
        // which is success for --no-index; >1 is a real error).
        let untracked = std::process::Command::new("git")
            .args(["diff", "--no-index", "--", "/dev/null", path])
            .current_dir(dir)
            .output()
            .map_err(|e| DesktopError::Message(format!("git diff failed: {e}")))?;
        match untracked.status.code() {
            Some(0) | Some(1) => {
                text = String::from_utf8_lossy(&untracked.stdout).to_string();
            }
            _ => {
                let stderr =
                    String::from_utf8_lossy(&untracked.stderr).trim().to_string();
                return Err(DesktopError::Message(if stderr.is_empty() {
                    "git diff failed".into()
                } else {
                    stderr
                }));
            }
        }
    }

    Ok(truncate_diff(text))
}

/// Unified diff for one file (read-only, capped) for the Changes panel.
#[tauri::command]
pub fn git_diff(cwd: String, path: String) -> DesktopResult<String> {
    let path = path.trim();
    if path.is_empty() || path.starts_with('-') {
        return Err(DesktopError::Message("invalid path".into()));
    }
    diff_against_rev(std::path::Path::new(&cwd), "HEAD", path)
}

/// Stage everything and commit in the session's working directory, for the
/// "Commit & Push" bar above the composer. Isolated sessions
/// integrate their worktree back into the base repo instead (`integrate_session`)
/// — committing directly here would strand the commit in a throwaway worktree —
/// so this is rejected up front for those sessions.
///
/// Returns the resulting commit's short SHA.
#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
) -> DesktopResult<String> {
    let message = message.trim();
    if message.is_empty() {
        return Err(DesktopError::Message("commit message is required".into()));
    }
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let add = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git add failed: {e}")))?;
    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git add failed".into()
        } else {
            stderr
        }));
    }

    let commit = std::process::Command::new("git")
        .args(["commit", "-m"])
        .arg(message)
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git commit failed: {e}")))?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git commit failed".into()
        } else {
            stderr
        }));
    }

    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git rev-parse failed: {e}")))?;
    if !sha.status.success() {
        let stderr = String::from_utf8_lossy(&sha.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git rev-parse failed".into()
        } else {
            stderr
        }));
    }
    Ok(String::from_utf8_lossy(&sha.stdout).trim().to_string())
}

/// Push the current branch in the session's working directory. Same
/// isolated-session restriction as `git_commit` (see its doc comment).
#[tauri::command]
pub async fn git_push(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let push = std::process::Command::new("git")
        .args(["push"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git push failed: {e}")))?;
    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git push failed".into()
        } else {
            stderr
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Review flow: per-file keep/undo + hunk-patch apply. Finer-grained,
// desktop-side git operations layered on top of the whole-workspace
// integrate/discard flow (`integrate_session`/`discard_session` above), for
// the per-file Changes-panel review UI. Follows the same "shell git directly"
// precedent as `git_status`/`git_diff` rather than routing through the
// engine.
// ---------------------------------------------------------------------------

/// Reject absolute paths and any path containing a `..` component — every
/// review command takes a path that is supposed to be repo-relative, and
/// these are shelled straight into `git -C <dir> ... -- <path>` /
/// filesystem calls, so path traversal must be ruled out up front.
fn validate_repo_relative_path(path: &str) -> DesktopResult<&str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("path is required".into()));
    }
    let as_path = std::path::Path::new(trimmed);
    if as_path.is_absolute() {
        return Err(DesktopError::Message(format!(
            "path must be repo-relative, got absolute path: {trimmed}"
        )));
    }
    if as_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DesktopError::Message(format!(
            "path must not contain '..': {trimmed}"
        )));
    }
    Ok(trimmed)
}

/// Two-letter `git status --porcelain` code for a single path (e.g. `"??"`,
/// `" M"`, `"D "`), or `None` if the path has no pending changes.
fn porcelain_code(dir: &std::path::Path, path: &str) -> DesktopResult<Option<String>> {
    let out = std::process::Command::new("git")
        .args(["-C"])
        .arg(dir)
        .args(["status", "--porcelain", "--", path])
        .output()
        .map_err(|e| DesktopError::Message(format!("git status failed for `{path}`: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "git status failed for `{path}`: {}",
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(line) = stdout.lines().next() else {
        return Ok(None);
    };
    if line.len() < 2 {
        return Ok(None);
    }
    Ok(Some(line[..2].to_string()))
}

/// `git -C <base_dir> rev-parse HEAD`, used as the stable "pre-agent" base
/// state to diff/restore against for isolated sessions (the worktree's own
/// HEAD can move — `integrate_session` commits agent changes into it).
fn base_head_sha(base_dir: &std::path::Path) -> DesktopResult<String> {
    let out = std::process::Command::new("git")
        .args(["-C"])
        .arg(base_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| {
            DesktopError::Message(format!(
                "git rev-parse HEAD failed in base repo `{}`: {e}",
                base_dir.display()
            ))
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "git rev-parse HEAD failed in base repo `{}`: {}",
            base_dir.display(),
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Resolve the session's working directory + optional base-repo directory,
/// mirroring the `meta.cwd` / `meta.base_cwd` split documented on
/// `SessionMeta`: `cwd` is the worktree root when isolated, else the repo
/// itself; `base_cwd` is `Some` only when isolated.
async fn review_dirs(
    state: &AppState,
    session_id: &str,
) -> DesktopResult<(PathBuf, Option<PathBuf>)> {
    let service = require_service(state).await?;
    let id = SessionId::from(session_id.to_string());
    let meta = service.session_meta(&id).await?;
    Ok((meta.cwd, meta.base_cwd))
}

/// Revert one file's agent changes in the session's working directory
/// (worktree root if isolated, else the repo itself) back to the pre-agent
/// base state.
///
/// - Untracked in the working dir (`??`) → delete it.
/// - Isolated session → restore the path from the *base* repo's HEAD commit
///   (not the worktree's own HEAD, which `integrate_session` may have
///   advanced by committing agent changes) via
///   `git checkout <base_head_sha> -- <path>`. Falls back to the worktree's
///   own HEAD if the base sha somehow isn't reachable there.
/// - Non-isolated session → `git checkout HEAD -- <path>`.
#[tauri::command]
pub async fn review_undo_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let path = validate_repo_relative_path(&path)?;
    let (dir, base_cwd) = review_dirs(&state, &session_id).await?;

    if let Some(code) = porcelain_code(&dir, path)? {
        if code == "??" {
            let full = dir.join(path);
            return std::fs::remove_file(&full).map_err(|e| {
                DesktopError::Message(format!(
                    "failed to delete untracked file `{}`: {e}",
                    full.display()
                ))
            });
        }
    }

    let checkout_from = |rev: &str| -> DesktopResult<std::process::Output> {
        std::process::Command::new("git")
            .args(["-C"])
            .arg(&dir)
            .args(["checkout", rev, "--", path])
            .output()
            .map_err(|e| {
                DesktopError::Message(format!(
                    "git checkout {rev} -- {path} failed in `{}`: {e}",
                    dir.display()
                ))
            })
    };

    if let Some(base_dir) = &base_cwd {
        let base_head = base_head_sha(base_dir)?;
        let out = checkout_from(&base_head)?;
        if out.status.success() {
            return Ok(());
        }
        // Base sha unreachable in the worktree (shouldn't happen — the
        // worktree branches from it) — fall back to the worktree's own HEAD.
        let fallback = checkout_from("HEAD")?;
        if fallback.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&fallback.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "failed to revert `{path}` in `{}`: {}",
            dir.display(),
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }

    let out = checkout_from("HEAD")?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(DesktopError::Message(format!(
        "failed to revert `{path}` in `{}`: {}",
        dir.display(),
        if stderr.is_empty() {
            "unknown error".to_string()
        } else {
            stderr
        }
    )))
}

/// Make the base repo's copy of one file match the worktree's current copy
/// (isolated sessions only). This is a plain working-tree write — it never
/// runs `git add` in the base repo, so the base repo's index stays exactly
/// as the user left it; `integrate_session` is the sanctioned path for a
/// real merge.
///
/// - File exists in the worktree → create parent dirs in the base repo and
///   copy the file's bytes over.
/// - File was deleted in the worktree (porcelain ` D` / `D `) → remove it
///   from the base repo's working tree (missing file is not an error).
#[tauri::command]
pub async fn review_keep_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let path = validate_repo_relative_path(&path)?;
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let Some(base_dir) = base_cwd else {
        return Err(DesktopError::Message("session is not isolated".into()));
    };

    let src = worktree.join(path);
    let dst = base_dir.join(path);

    if src.exists() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DesktopError::Message(format!(
                    "failed to create directory `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::copy(&src, &dst).map_err(|e| {
            DesktopError::Message(format!(
                "failed to copy `{}` to `{}`: {e}",
                src.display(),
                dst.display()
            ))
        })?;
        Ok(())
    } else {
        match std::fs::remove_file(&dst) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DesktopError::Message(format!(
                "failed to remove `{}`: {e}",
                dst.display()
            ))),
        }
    }
}

/// Apply (or reverse-apply) a unified-diff patch — produced client-side from
/// filtered hunks — against either the session's worktree or its base repo.
#[tauri::command]
pub async fn review_apply_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch: String,
    target: String,
    reverse: bool,
) -> DesktopResult<()> {
    if patch.trim().is_empty() {
        return Err(DesktopError::Message("patch is empty".into()));
    }
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let dir = match target.as_str() {
        "worktree" => worktree,
        "base" => base_cwd.ok_or_else(|| {
            DesktopError::Message("session is not isolated — no base directory".into())
        })?,
        other => {
            return Err(DesktopError::Message(format!(
                "unknown patch target: {other} (expected \"worktree\" or \"base\")"
            )));
        }
    };

    let file_name = format!(
        "flex-review-patch-{}-{}.diff",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let patch_path = std::env::temp_dir().join(file_name);
    std::fs::write(&patch_path, &patch).map_err(|e| {
        DesktopError::Message(format!(
            "failed to write temp patch file `{}`: {e}",
            patch_path.display()
        ))
    })?;

    let mut args: Vec<&str> = vec!["-C"];
    let dir_str = dir.to_string_lossy();
    args.push(&dir_str);
    args.push("apply");
    if reverse {
        args.push("--reverse");
    }
    args.push("--whitespace=nowarn");
    let patch_path_str = patch_path.to_string_lossy();
    args.push(&patch_path_str);

    let result = std::process::Command::new("git").args(&args).output();

    let cleanup = std::fs::remove_file(&patch_path);
    if let Err(e) = cleanup {
        tracing::warn!(path = %patch_path.display(), error = %e, "failed to remove temp patch file");
    }

    let out = result.map_err(|e| DesktopError::Message(format!("git apply failed: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(DesktopError::Message(format!(
            "patch failed: {}",
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )))
    }
}

/// Unified diff for one file, always computed against the pre-agent base
/// state: for isolated sessions this is the base repo's HEAD (so committed
/// *and* uncommitted agent changes both show up, since `integrate_session`
/// may have already committed some into the worktree); for non-isolated
/// sessions this is identical to `git_diff` (HEAD, with an untracked-file
/// fallback).
#[tauri::command]
pub async fn review_file_diff(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<String> {
    let path = validate_repo_relative_path(&path)?;
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;

    let base_head = match &base_cwd {
        Some(base_dir) => base_head_sha(base_dir)?,
        None => "HEAD".to_string(),
    };
    diff_against_rev(&worktree, &base_head, path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// Path relative to `cwd`, forward-slashed.
    pub path: String,
    /// Basename, shown as the primary label.
    pub name: String,
}

/// Rank a path against a lowercase needle. Lower is better; `None` = no match.
fn score_file(rel_path: &str, name: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(100); // browse mode — rank by path length afterwards
    }
    let path_l = rel_path.to_lowercase();
    let name_l = name.to_lowercase();
    if name_l.starts_with(needle) {
        return Some(0);
    }
    if name_l.contains(needle) {
        return Some(1);
    }
    if path_l.contains(needle) {
        return Some(2);
    }
    if is_subsequence(needle, &path_l) {
        return Some(3);
    }
    None
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut chars = hay.chars();
    needle
        .chars()
        .all(|nc| chars.by_ref().any(|hc| hc == nc))
}

/// Read-only fuzzy file search under `cwd` for composer @-mentions. Respects
/// `.gitignore`/`.git/exclude` (via the `ignore` crate) and caps results.
#[tauri::command]
pub fn list_files(cwd: String, query: String) -> Vec<FileHit> {
    let root = PathBuf::from(&cwd);
    if !root.is_dir() {
        return Vec::new();
    }
    let needle = query.trim().to_lowercase();

    // Bound the walk so huge repos can't stall an interactive keystroke.
    const MAX_WALK: usize = 20_000;
    let mut walked = 0usize;
    let mut hits: Vec<(i32, FileHit)> = Vec::new();

    let walker = ignore::WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .build();

    for entry in walker.flatten() {
        if walked >= MAX_WALK {
            break;
        }
        walked += 1;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() || rel_str.starts_with(".git/") {
            continue;
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_str.clone());
        let Some(score) = score_file(&rel_str, &name, &needle) else {
            continue;
        };
        hits.push((score, FileHit { path: rel_str, name }));
    }

    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.path.len().cmp(&b.1.path.len()))
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    hits.truncate(50);
    hits.into_iter().map(|(_, h)| h).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfoDto {
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
}

#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>) -> DesktopResult<Vec<CommandInfoDto>> {
    let service = require_service(&state).await?;
    let hello = service.hello();
    Ok(hello
        .capabilities
        .commands
        .into_iter()
        .map(|c: CommandInfo| CommandInfoDto {
            name: c.name,
            description: c.description,
            args_hint: c.args_hint,
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatusDto {
    pub files_changed: u32,
    pub summary: String,
}

#[tauri::command]
pub async fn is_isolated(state: State<'_, AppState>, session_id: String) -> DesktopResult<bool> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.is_isolated(&id).await?)
}

#[tauri::command]
pub async fn workspace_status(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<Option<WorkspaceStatusDto>> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let status: Option<WorkspaceStatus> = service.workspace_status(&id).await?;
    Ok(status.map(|s| WorkspaceStatusDto {
        files_changed: s.files_changed,
        summary: s.summary,
    }))
}

#[tauri::command]
pub async fn integrate_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<IntegrationOutcome> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.integrate_session(&id).await?)
}

#[tauri::command]
pub async fn discard_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.discard_session(&id).await?)
}

#[tauri::command]
pub async fn revert(
    state: State<'_, AppState>,
    session_id: String,
    snapshot_id: String,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.revert(&id, &snapshot_id).await?)
}

// ---------------------------------------------------------------------------
// Routines (automations): saved goal configurations run by a cron schedule or
// webhook trigger instead of a human sending a prompt. See
// `agentloop_channel::routine` for the underlying contracts and
// `agentloop_sdk::routines` for the file store + runner.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineTriggerDto {
    pub kind: String,
    pub expr: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineDto {
    pub id: String,
    pub prompt: String,
    pub max_iterations: u32,
    pub max_identical_failures: u32,
    pub token_budget: Option<u64>,
    pub require_verification: bool,
    pub trigger: RoutineTriggerDto,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRunRecordDto {
    pub session_id: String,
    pub started_ms: u64,
    pub stop_reason: String,
    pub iterations: u32,
}

fn routine_trigger_from_dto(dto: &RoutineTriggerDto) -> DesktopResult<RoutineTrigger> {
    match dto.kind.as_str() {
        "cron" => {
            let expr = dto
                .expr
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DesktopError::Message("cron trigger requires `expr`".into()))?;
            Ok(RoutineTrigger::Cron {
                expr: expr.to_owned(),
            })
        }
        "webhook" => {
            let path = dto
                .path
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DesktopError::Message("webhook trigger requires `path`".into()))?;
            Ok(RoutineTrigger::Webhook {
                path: path.to_owned(),
            })
        }
        other => Err(DesktopError::Message(format!(
            "unknown routine trigger kind: {other}"
        ))),
    }
}

fn routine_trigger_to_dto(trigger: &RoutineTrigger) -> RoutineTriggerDto {
    match trigger {
        RoutineTrigger::Cron { expr } => RoutineTriggerDto {
            kind: "cron".into(),
            expr: Some(expr.clone()),
            path: None,
        },
        RoutineTrigger::Webhook { path } => RoutineTriggerDto {
            kind: "webhook".into(),
            expr: None,
            path: Some(path.clone()),
        },
    }
}

fn routine_dto_to_spec(dto: RoutineDto) -> DesktopResult<RoutineSpec> {
    let trigger = routine_trigger_from_dto(&dto.trigger)?;
    Ok(RoutineSpec {
        id: dto.id,
        goal: GoalSpec {
            prompt: dto.prompt,
            max_iterations: dto.max_iterations,
            max_identical_failures: dto.max_identical_failures,
            token_budget: dto.token_budget,
            require_verification: dto.require_verification,
        },
        session_seed: NewSessionParams {
            title: dto.title,
            cwd: dto.cwd.map(PathBuf::from),
            model: dto.model.map(ModelRef),
            ..NewSessionParams::default()
        },
        trigger,
    })
}

fn routine_spec_to_dto(spec: RoutineSpec) -> RoutineDto {
    RoutineDto {
        id: spec.id,
        prompt: spec.goal.prompt,
        max_iterations: spec.goal.max_iterations,
        max_identical_failures: spec.goal.max_identical_failures,
        token_budget: spec.goal.token_budget,
        require_verification: spec.goal.require_verification,
        trigger: routine_trigger_to_dto(&spec.trigger),
        title: spec.session_seed.title,
        cwd: spec
            .session_seed
            .cwd
            .map(|p| p.to_string_lossy().into_owned()),
        model: spec.session_seed.model.map(|m| m.0),
    }
}

fn validate_routine_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("routine id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace)
    {
        return Err(DesktopError::Message(
            "routine id must not contain slashes or whitespace".into(),
        ));
    }
    Ok(trimmed)
}

fn routine_store() -> DesktopResult<FileRoutineStore> {
    FileRoutineStore::with_default_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

#[tauri::command]
pub async fn routines_list() -> DesktopResult<Vec<RoutineDto>> {
    let store = routine_store()?;
    let mut specs = RoutineStore::list(&store)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs.into_iter().map(routine_spec_to_dto).collect())
}

#[tauri::command]
pub async fn routines_upsert(routine: RoutineDto) -> DesktopResult<()> {
    validate_routine_id(&routine.id)?;
    if routine.prompt.trim().is_empty() {
        return Err(DesktopError::Message("routine prompt is required".into()));
    }
    let spec = routine_dto_to_spec(routine)?;
    let store = routine_store()?;
    RoutineStore::upsert(&store, spec)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))
}

#[tauri::command]
pub async fn routines_remove(id: String) -> DesktopResult<()> {
    let id = validate_routine_id(&id)?;
    let store = routine_store()?;
    RoutineStore::remove(&store, id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))
}

#[tauri::command]
pub async fn routines_run(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    validate_routine_id(&id)?;
    let service = require_service(&state).await?;
    let store = routine_store()?;
    let runner = RoutineRunner::new(Arc::new(service), Arc::new(store));
    tauri::async_runtime::spawn(async move {
        if let Err(e) = runner.run_by_id(&id).await {
            tracing::warn!(error = %e, routine = %id, "routine run failed");
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn routines_history(id: String) -> DesktopResult<Vec<RoutineRunRecordDto>> {
    let id = validate_routine_id(&id)?;
    let Some(dir) = default_routines_dir() else {
        return Err(DesktopError::Message(
            "could not resolve home directory".into(),
        ));
    };
    let path = dir.join(format!("{id}.history.jsonl"));
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DesktopError::Message(e.to_string())),
    };

    let mut records = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<agentloop_channel::RoutineRunRecord>(line) {
            Ok(record) => {
                let stop_reason = serde_json::to_value(&record.outcome.stop_reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "unknown".into());
                records.push(RoutineRunRecordDto {
                    session_id: record.session_id.as_str().to_owned(),
                    started_ms: record.started_ms,
                    stop_reason,
                    iterations: record.outcome.iterations,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, routine = %id, "skipping malformed routine history line");
            }
        }
    }
    Ok(records)
}

// ---------------------------------------------------------------------------
// MCP servers: user-configured Model Context Protocol servers whose tools are
// bridged into the native tool registry (`agentloop_mcp::McpManager`, folded
// in by `EngineService::native` from `EngineConfig.mcp`). Specs persist via
// `agentloop_sdk::mcp_store::FileMcpStore` (one `<id>.toml` under
// `~/.config/agentloop/mcp`, mirroring `FileRoutineStore`). Unlike routines,
// saving/removing a server must rebuild the engine service (mirrors
// `save_provider_config`) since `EngineConfig.mcp` is only read at
// composition time in `compose::build_service` — there is no hot-reload of a
// running session, hence the UI copy pointing users at restarting sessions.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    pub enabled: bool,
}

fn mcp_dto_to_config(dto: McpServerDto) -> agentloop_sdk::McpServerConfig {
    agentloop_sdk::McpServerConfig {
        name: dto.id,
        display_name: None,
        enabled: dto.enabled,
        transport: agentloop_sdk::McpServerTransport::Stdio(agentloop_sdk::StdioServerConfig {
            command: dto.command,
            args: dto.args,
            env: dto.env,
            cwd: None,
        }),
        tool_name_prefix: None,
    }
}

fn mcp_config_to_dto(config: agentloop_sdk::McpServerConfig) -> McpServerDto {
    let (command, args, env) = match config.transport {
        agentloop_sdk::McpServerTransport::Stdio(stdio) => (stdio.command, stdio.args, stdio.env),
        // The desktop UI only manages stdio servers (MVP scope); any other
        // transport a `.toml` file might carry (hand-edited, or added by a
        // future UI) still round-trips through list/remove, just with an
        // empty command shown — `mcp_upsert` always writes Stdio, so this
        // path shouldn't normally be hit for desktop-managed servers.
        agentloop_sdk::McpServerTransport::StreamableHttp(http)
        | agentloop_sdk::McpServerTransport::Sse(http) => (http.url, Vec::new(), http.headers),
        _ => (String::new(), Vec::new(), std::collections::BTreeMap::new()),
    };
    McpServerDto {
        id: config.name,
        command,
        args,
        env,
        enabled: config.enabled,
    }
}

fn validate_mcp_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("server id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace)
    {
        return Err(DesktopError::Message(
            "server id must not contain slashes or whitespace".into(),
        ));
    }
    Ok(trimmed)
}

fn mcp_store() -> DesktopResult<agentloop_sdk::mcp_store::FileMcpStore> {
    agentloop_sdk::mcp_store::FileMcpStore::with_default_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

/// Rebuild the engine service from the current provider config so a saved
/// MCP server change takes effect on the next session (existing sessions
/// keep running against the service they already captured). Errors are
/// swallowed on purpose: MCP servers are additive and the provider might not
/// be configured yet (`DesktopError::NotConfigured`), which must not block
/// saving/removing a server spec.
async fn rebuild_service_after_mcp_change(state: &AppState) {
    let cfg = state.config.lock().await.clone();
    match crate::compose::build_service(&cfg, state.store.clone()) {
        Ok(service) => *state.service.lock().await = Some(service),
        Err(DesktopError::NotConfigured) => {}
        Err(err) => {
            tracing::warn!(error = %err, "failed to rebuild engine service after MCP change");
        }
    }
}

#[tauri::command]
pub async fn mcp_list() -> DesktopResult<Vec<McpServerDto>> {
    let store = mcp_store()?;
    let mut servers = store
        .list()
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers.into_iter().map(mcp_config_to_dto).collect())
}

#[tauri::command]
pub async fn mcp_upsert(state: State<'_, AppState>, server: McpServerDto) -> DesktopResult<()> {
    validate_mcp_id(&server.id)?;
    if server.command.trim().is_empty() {
        return Err(DesktopError::Message("command is required".into()));
    }
    let config = mcp_dto_to_config(server);
    let store = mcp_store()?;
    store
        .upsert(config)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    rebuild_service_after_mcp_change(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn mcp_remove(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    store
        .remove(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    rebuild_service_after_mcp_change(&state).await;
    Ok(())
}

/// Connect to a saved server and list its tools — the "Test" button in the
/// UI. Talks to the MCP client directly (not through `McpManager`, which
/// keys server lookups off its own config snapshot) and never touches
/// `state.service`, so testing never disturbs the live engine or requires a
/// provider to be configured yet.
#[tauri::command]
pub async fn mcp_test(id: String) -> DesktopResult<Vec<String>> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    let server = store
        .get(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?
        .ok_or_else(|| DesktopError::Message(format!("server `{id}` not found")))?;

    let client = agentloop_sdk::mcp::RmcpToolClient::from_configs(std::slice::from_ref(&server));
    let tools = client
        .list_tools(&server, tokio_util::sync::CancellationToken::new())
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    client.shutdown().await;
    Ok(tools.into_iter().map(|tool| tool.name).collect())
}

// ---------------------------------------------------------------------------
// Memory: durable notes the `learning` plugin's `MemoryWrite` tool persists as
// `<name>.md` files under `~/.config/agentloop/memory` (loaded into every
// future session's system prompt — see `agentloop_prompts::load_memory_section`).
// The SDK/engine expose no list/read/delete API (`MemoryWrite` is write-only —
// see `agentloop_learning::MemoryWriteTool`), so these commands operate
// directly on the flat `<name>.md` file layout, which is stable and owned by
// this crate alone (`default_memory_dir` just joins path segments; no format
// to get out of sync with).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryDto {
    /// The note's name (file stem), e.g. `user-preferences`.
    pub id: String,
    /// First non-empty line of the note, used as a title in the list view.
    pub title: String,
    /// Full markdown body. `None` in `memory_list` (call `memory_get` to fetch it).
    pub content: Option<String>,
    /// Milliseconds since epoch, from the file's last-modified time.
    pub updated_at_ms: Option<u64>,
    /// Milliseconds since epoch when this entry expires and is purged.
    /// `None` = long-term / never expires. Sourced from the sidecar
    /// `expiry.json` file in the same directory (see the "Memory expiry"
    /// section below) — never from the `.md` file itself.
    pub expires_at_ms: Option<u64>,
}

fn memory_dir() -> DesktopResult<PathBuf> {
    agentloop_sdk::learning::default_memory_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

fn validate_memory_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("memory id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace)
    {
        return Err(DesktopError::Message(
            "memory id must not contain slashes or whitespace".into(),
        ));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(DesktopError::Message("invalid memory id".into()));
    }
    Ok(trimmed)
}

fn memory_title(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled memory".to_owned())
}

fn modified_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Memory expiry: short-term memories carry a TTL; long-term ones never
// expire. The engine's prompt loader (`agentloop_prompts::load_memory_section`)
// reads the raw `.md` file body verbatim into the system prompt, so any
// frontmatter/marker written into the file itself would leak straight into
// the model's context. To keep the `.md` files byte-for-byte "just the note"
// (untouched by this feature), expiry lives in a sidecar `expiry.json` file
// in the same directory, mapping `<entry id>` -> `<unix ms expiry>`. Entries
// absent from the map never expire.
// ---------------------------------------------------------------------------

const EXPIRY_SIDECAR_FILE: &str = "expiry.json";

type ExpiryMap = std::collections::BTreeMap<String, u64>;

fn expiry_sidecar_path(dir: &std::path::Path) -> PathBuf {
    dir.join(EXPIRY_SIDECAR_FILE)
}

/// Read a directory's `expiry.json`. Missing file or unparseable content is
/// treated as "no entries have an expiry" rather than an error — expiry is a
/// best-effort convenience layer, never something that should break listing
/// or deleting memories.
fn read_expiry_map(dir: &std::path::Path) -> ExpiryMap {
    let path = expiry_sidecar_path(dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return ExpiryMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_expiry_map(dir: &std::path::Path, map: &ExpiryMap) -> DesktopResult<()> {
    let path = expiry_sidecar_path(dir);
    if map.is_empty() {
        // Nothing left to track — remove the sidecar rather than leave an
        // empty `{}` file around.
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(DesktopError::Message(format!(
                    "cannot remove `{}`: {e}",
                    path.display()
                )));
            }
        }
    }
    let json = serde_json::to_string_pretty(map)
        .map_err(|e| DesktopError::Message(format!("cannot serialize expiry map: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", path.display())))
}

/// Set (or clear, when `expires_at_ms` is `None`) one entry's expiry in
/// `dir`'s `expiry.json`. Shared by both the global and per-project
/// `*_set_expiry` commands below.
fn set_expiry_in_dir(
    dir: &std::path::Path,
    id: &str,
    expires_at_ms: Option<u64>,
) -> DesktopResult<()> {
    let mut map = read_expiry_map(dir);
    match expires_at_ms {
        Some(ts) => {
            map.insert(id.to_owned(), ts);
        }
        None => {
            map.remove(id);
        }
    }
    write_expiry_map(dir, &map)
}

/// Delete every `.md` file in `dir` whose `expiry.json` entry is in the past,
/// dropping its expiry entry too. Called at the top of every `*_list` command
/// (self-cleaning listing) and from `prompt()` before assembling the
/// per-turn system prompt, so expired memories never reach the model even if
/// the Memory page was never opened. Best-effort: I/O errors are swallowed —
/// a purge failure must not block listing or prompting.
fn purge_expired_memories(dir: &std::path::Path) {
    let mut map = read_expiry_map(dir);
    if map.is_empty() {
        return;
    }
    let now = now_ms();
    let expired: Vec<String> = map
        .iter()
        .filter(|(_, &ts)| ts <= now)
        .map(|(id, _)| id.clone())
        .collect();
    if expired.is_empty() {
        return;
    }
    for id in &expired {
        let path = dir.join(format!("{id}.md"));
        let _ = std::fs::remove_file(&path);
        map.remove(id);
    }
    let _ = write_expiry_map(dir, &map);
}

/// List memory notes (title + metadata, no content — call `memory_get` for
/// the body). Sorted by most-recently-modified first.
#[tauri::command]
pub async fn memory_list() -> DesktopResult<Vec<MemoryEntryDto>> {
    let dir = memory_dir()?;
    purge_expired_memories(&dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot read memory directory `{}`: {e}",
                dir.display()
            )));
        }
    };

    let expiry = read_expiry_map(&dir);
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let updated_at_ms = std::fs::metadata(&path).ok().and_then(|m| modified_ms(&m));
        out.push(MemoryEntryDto {
            id: id.to_owned(),
            title: memory_title(&body),
            content: None,
            updated_at_ms,
            expires_at_ms: expiry.get(id).copied(),
        });
    }
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms).then(a.id.cmp(&b.id)));
    Ok(out)
}

/// Read one memory note's full content.
#[tauri::command]
pub async fn memory_get(id: String) -> DesktopResult<MemoryEntryDto> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    let path = dir.join(format!("{id}.md"));
    let body = tokio::fs::read_to_string(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DesktopError::Message(format!("memory `{id}` not found"))
        } else {
            DesktopError::Message(format!("cannot read memory `{id}`: {e}"))
        }
    })?;
    let updated_at_ms = tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| modified_ms(&m));
    let expires_at_ms = read_expiry_map(&dir).get(id).copied();
    Ok(MemoryEntryDto {
        id: id.to_owned(),
        title: memory_title(&body),
        content: Some(body),
        updated_at_ms,
        expires_at_ms,
    })
}

/// Delete a memory note. There is no engine/SDK API for this — `MemoryWrite`
/// is write-only — so this removes the `<name>.md` file directly; safe
/// because the on-disk layout is exactly one file per note with no index to
/// keep in sync. Also drops any `expiry.json` entry for the id.
#[tauri::command]
pub async fn memory_remove(id: String) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    let path = dir.join(format!("{id}.md"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot delete memory `{id}`: {e}"
            )));
        }
    }
    set_expiry_in_dir(&dir, id, None)
}

/// Set (or clear) a global memory entry's expiry. `expires_at_ms: None`
/// removes it from `expiry.json`, i.e. the entry never expires (long-term).
#[tauri::command]
pub async fn memory_set_expiry(id: String, expires_at_ms: Option<u64>) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    set_expiry_in_dir(&dir, id, expires_at_ms)
}

// ---------------------------------------------------------------------------
// Project memory: the same durable-notes mechanism as the global memory
// section above, scoped to one project instead of the user's home directory.
// Backing dir is `<cwd>/.agent/memory` (flat `<name>.md` layout, same shape
// as `default_memory_dir`). `cwd` is caller-supplied (the frontend has no
// other way to name "this project"), so it is canonicalized and required to
// be an existing directory before use, closing off path traversal.
//
// Reads only: writes still go to the global memory dir only (`MemoryWrite`
// has no per-project mode), so a note captured mid-session lands in
// `~/.config/agentloop/memory` even when working in a project — promoting a
// note to project scope is a manual file move for now. See `prompt()` below
// for how this is loaded into the system prompt alongside `cwd_notice`.
// ---------------------------------------------------------------------------

fn project_memory_dir(cwd: &str) -> DesktopResult<PathBuf> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("cwd is required".into()));
    }
    let path = PathBuf::from(trimmed);
    let canonical = path
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid cwd `{trimmed}`: {e}")))?;
    if !canonical.is_dir() {
        return Err(DesktopError::Message(format!(
            "cwd `{trimmed}` is not a directory"
        )));
    }
    Ok(canonical.join(".agent").join("memory"))
}

/// List a project's memory notes (title + metadata, no content — call
/// `project_memory_get` for the body). Sorted by most-recently-modified
/// first. Mirrors `memory_list` exactly but reads from `<cwd>/.agent/memory`.
#[tauri::command]
pub async fn project_memory_list(cwd: String) -> DesktopResult<Vec<MemoryEntryDto>> {
    let dir = project_memory_dir(&cwd)?;
    purge_expired_memories(&dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot read memory directory `{}`: {e}",
                dir.display()
            )));
        }
    };

    let expiry = read_expiry_map(&dir);
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let updated_at_ms = std::fs::metadata(&path).ok().and_then(|m| modified_ms(&m));
        out.push(MemoryEntryDto {
            id: id.to_owned(),
            title: memory_title(&body),
            content: None,
            updated_at_ms,
            expires_at_ms: expiry.get(id).copied(),
        });
    }
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms).then(a.id.cmp(&b.id)));
    Ok(out)
}

/// Read one project memory note's full content.
#[tauri::command]
pub async fn project_memory_get(cwd: String, id: String) -> DesktopResult<MemoryEntryDto> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    let path = dir.join(format!("{id}.md"));
    let body = tokio::fs::read_to_string(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DesktopError::Message(format!("memory `{id}` not found"))
        } else {
            DesktopError::Message(format!("cannot read memory `{id}`: {e}"))
        }
    })?;
    let updated_at_ms = tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| modified_ms(&m));
    let expires_at_ms = read_expiry_map(&dir).get(id).copied();
    Ok(MemoryEntryDto {
        id: id.to_owned(),
        title: memory_title(&body),
        content: Some(body),
        updated_at_ms,
        expires_at_ms,
    })
}

/// Delete a project memory note. Same rationale as `memory_remove`: no
/// engine/SDK API for this, so it removes the `<name>.md` file directly.
/// Also drops any `expiry.json` entry for the id.
#[tauri::command]
pub async fn project_memory_remove(cwd: String, id: String) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    let path = dir.join(format!("{id}.md"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot delete memory `{id}`: {e}"
            )));
        }
    }
    set_expiry_in_dir(&dir, id, None)
}

/// Set (or clear) a project memory entry's expiry — same semantics as
/// `memory_set_expiry` but scoped to `<cwd>/.agent/memory`.
#[tauri::command]
pub async fn project_memory_set_expiry(
    cwd: String,
    id: String,
    expires_at_ms: Option<u64>,
) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    set_expiry_in_dir(&dir, id, expires_at_ms)
}

// ---------------------------------------------------------------------------
// User identity: best-effort local display name for the sidebar footer.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIdentityDto {
    pub name: String,
}

/// Best-effort local display name for the sidebar footer: `git config
/// user.name` (run without a `cwd` so it reads the global config), falling
/// back to the `USER` env var, then a static "User" placeholder. Never fails.
#[tauri::command]
pub async fn user_identity(_state: State<'_, AppState>) -> DesktopResult<UserIdentityDto> {
    let git_name = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let name = git_name
        .or_else(|| std::env::var("USER").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "User".to_string());

    Ok(UserIdentityDto { name })
}

// ---------------------------------------------------------------------------
// Plan tab: Save to Workspace — writes the rendered plan markdown to a file
// inside the session's cwd. Traversal-hardened the same way as
// `project_memory_dir`/`validate_repo_relative_path` above: canonicalize the
// cwd, join the caller-supplied relative path, then verify the WRITTEN
// file's parent directory canonicalizes to somewhere still inside the
// canonical cwd — this catches `..` segments that a components() scan alone
// could miss once symlinks are involved (a plain "reject any ParentDir
// component" check, as `validate_repo_relative_path` does for git paths, is
// enough there because git resolves those paths itself; a raw filesystem
// write needs the stronger belt-and-suspenders canonicalize-and-prefix-check
// since we do the join ourselves).
// ---------------------------------------------------------------------------

/// Rejects absolute paths and any `..` path component before it's ever
/// joined onto a base directory — first line of defense, mirrors
/// `validate_repo_relative_path`'s component scan.
fn validate_relative_write_path(path: &str) -> DesktopResult<&str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("path is required".into()));
    }
    let as_path = std::path::Path::new(trimmed);
    if as_path.is_absolute() {
        return Err(DesktopError::Message(format!(
            "path must be relative to the session cwd, got absolute path: {trimmed}"
        )));
    }
    if as_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DesktopError::Message(format!(
            "path must not contain '..': {trimmed}"
        )));
    }
    Ok(trimmed)
}

/// Writes `content` to `relative_path` inside `session_id`'s cwd, creating
/// parent directories as needed (e.g. a not-yet-existing `plans/` folder).
/// Returns the absolute path written. Used by the Plan tab's "Save to
/// Workspace" menu item (`PlanToolbar`); the frontend passes
/// `plans/<slug>-<date>.md`.
#[tauri::command]
pub async fn save_text_file(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
    content: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = validate_relative_write_path(&relative_path)?;

    let canonical_cwd = meta
        .cwd
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;

    let target = canonical_cwd.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| DesktopError::Message("path has no parent directory".into()))?;

    std::fs::create_dir_all(parent)
        .map_err(|e| DesktopError::Message(format!("cannot create `{}`: {e}", parent.display())))?;

    // Re-canonicalize the parent now that it's guaranteed to exist, and
    // verify it's still inside the session cwd — belt-and-suspenders against
    // `..` traversal via symlinks that the component scan above can't see.
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid target directory: {e}")))?;
    if !canonical_parent.starts_with(&canonical_cwd) {
        return Err(DesktopError::Message(
            "resolved path escapes the session's working directory".into(),
        ));
    }

    std::fs::write(&target, content)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", target.display())))?;

    Ok(target.display().to_string())
}

/// (Re)spawn the routines cron-poll loop against the current engine service.
/// Cancels any previously running loop first — called after the engine is
/// (re)built (`save_provider_config`, and once at startup if configured).
pub async fn respawn_cron_loop(state: &AppState) {
    if let Some(token) = state.routine_cancel.lock().await.take() {
        token.cancel();
    }

    let service = match require_service(state).await {
        Ok(service) => service,
        Err(_) => return,
    };
    let store = match routine_store() {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(error = %e, "routines store unavailable, cron loop not started");
            return;
        }
    };

    let runner = Arc::new(RoutineRunner::new(Arc::new(service), Arc::new(store)));
    let token = CancellationToken::new();
    tauri::async_runtime::spawn(runner.spawn_cron_loop(token.clone()));
    *state.routine_cancel.lock().await = Some(token);
}

#[cfg(test)]
mod session_baseline_tests {
    use super::*;

    /// Minimal git repo fixture under a fresh temp dir (not the shared
    /// scratchpad — each test gets its own throwaway repo so runs don't
    /// interfere). Returns the repo root.
    fn init_repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "flex-session-baseline-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        dir
    }

    fn write(dir: &std::path::Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(dir: &std::path::Path, msg: &str) {
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-q", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Baseline capture + filtering should hide a file that was already
    /// dirty before the session started (pre-existing repo mess), while
    /// surfacing a file the "session" newly touched.
    #[test]
    fn baseline_filters_pre_existing_dirty_file_but_keeps_new_edit() {
        let dir = init_repo();

        // Committed baseline: two tracked files.
        write(&dir, "pre_dirty.txt", "original\n");
        write(&dir, "untouched.txt", "original\n");
        commit_all(&dir, "initial commit");

        // Simulate a pre-existing dirty file the user had before opening a
        // session in this repo.
        write(&dir, "pre_dirty.txt", "user's uncommitted edit\n");

        // Capture the session baseline now (mirrors create_session).
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert!(!baseline.head_sha.is_empty());
        assert!(baseline.files.contains_key("pre_dirty.txt"));

        // Now the "session" makes its own edit to a different file, plus a
        // brand-new untracked file.
        write(&dir, "untouched.txt", "session edit\n");
        write(&dir, "session_new.txt", "brand new\n");

        let cwd_str = dir.to_string_lossy().to_string();
        let all = git_status_full(&cwd_str).expect("git_status_full should succeed");
        let all_paths: Vec<_> = all.iter().map(|f| f.path.as_str()).collect();
        assert!(all_paths.contains(&"pre_dirty.txt"));
        assert!(all_paths.contains(&"untouched.txt"));
        assert!(all_paths.contains(&"session_new.txt"));

        // Reproduce git_status_since_baseline's filtering logic directly
        // (it otherwise requires a Tauri State<AppState>).
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|f| match baseline.files.get(&f.path) {
                None => true,
                Some(baseline_hash) => {
                    let current_hash =
                        hash_object(&dir, &f.path).unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .map(|f| f.path)
            .collect();

        assert!(
            !filtered.contains(&"pre_dirty.txt".to_string()),
            "pre-existing dirty file must be filtered out: {filtered:?}"
        );
        assert!(
            filtered.contains(&"untouched.txt".to_string()),
            "session's own edit must survive filtering: {filtered:?}"
        );
        assert!(
            filtered.contains(&"session_new.txt".to_string()),
            "session's new untracked file must survive filtering: {filtered:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A path already dirty at baseline time, but further modified since,
    /// must still show up (content hash differs from the recorded one).
    #[test]
    fn baseline_keeps_file_when_further_modified_after_capture() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");

        write(&dir, "a.txt", "v2 (dirty before session)\n");
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");

        // Session further edits the same file.
        write(&dir, "a.txt", "v3 (session edit)\n");

        let current_hash = hash_object(&dir, "a.txt").unwrap();
        let baseline_hash = baseline.files.get("a.txt").unwrap();
        assert_ne!(
            &current_hash, baseline_hash,
            "hash must change after the session's own edit"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Deleted-at-baseline paths are recorded with the sentinel so a later
    /// re-creation of the same path is treated as new content, not as
    /// "unchanged since baseline".
    #[test]
    fn baseline_records_deleted_sentinel() {
        let dir = init_repo();
        write(&dir, "gone.txt", "will be deleted\n");
        commit_all(&dir, "initial commit");

        std::fs::remove_file(dir.join("gone.txt")).unwrap();
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");

        assert_eq!(baseline.files.get("gone.txt").map(String::as_str), Some("deleted"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reproduces the "phantom session changes" bug: an untracked directory
    /// that already existed before the session started must not show up as
    /// a session change, while a directory newly created during the session
    /// still must.
    #[test]
    fn baseline_filters_pre_existing_untracked_dir_but_keeps_new_dir() {
        let dir = init_repo();
        write(&dir, "README.md", "hello\n");
        commit_all(&dir, "initial commit");

        // Pre-existing untracked directory (e.g. a build output dir the user
        // already had before opening a session in this repo).
        write(&dir, "public/index.html", "<html></html>\n");

        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert_eq!(
            baseline.files.get("public/").map(String::as_str),
            Some("dir"),
            "pre-existing untracked dir must be recorded with the dir sentinel: {:?}",
            baseline.files
        );

        // Session creates a brand-new untracked directory of its own.
        write(&dir, "src/new_module.rs", "// new\n");

        let cwd_str = dir.to_string_lossy().to_string();
        let all = git_status_full(&cwd_str).expect("git_status_full should succeed");
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|f| match baseline.files.get(&f.path) {
                None => true,
                Some(baseline_hash) if baseline_hash == "dir" => false,
                Some(baseline_hash) => {
                    let current_hash =
                        hash_object(&dir, &f.path).unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .map(|f| f.path)
            .collect();

        assert!(
            !filtered.contains(&"public/".to_string()),
            "pre-existing untracked dir must be filtered out: {filtered:?}"
        );
        assert!(
            filtered.contains(&"src/".to_string()),
            "newly created dir during the session must survive filtering: {filtered:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
