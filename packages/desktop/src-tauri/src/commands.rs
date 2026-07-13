//! Tauri commands — thin wrappers over `EngineService` + keychain config.

use std::path::PathBuf;

use std::sync::Arc;

use agentloop_channel::{RoutineSpec, RoutineStore, RoutineTrigger};
use agentloop_contracts::{
    Answer, BlobSource, CommandInfo, ContentBlock, Effort, GoalSpec, IntegrationOutcome,
    IsolationPolicy, Message, ModelRef, NewSessionParams, PermissionDecision, PermissionMode,
    PermissionRequestId, PromptInput, QuestionId, SessionEvent, SessionId, SessionMeta,
    SessionMetaPatch, TurnOptions, TurnSummary,
};
use agentloop_core::{BackgroundEntrySummary, ChatRequest, ProviderStreamEvent, WorkspaceStatus};
use agentloop_sdk::EngineService;
use agentloop_sdk::mcp::McpToolClient;
use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner, default_routines_dir};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::compose::build_service;
use crate::config::{
    ProviderConfig, ProviderConfigView, ProviderProfile, ProviderProfileInput, ProviderProfileView,
    SaveProviderConfigInput, persist_config,
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

#[tracing::instrument(level = "debug", skip_all, err)]
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
    state: State<'_, AppState>,
    input: SaveProviderConfigInput,
) -> DesktopResult<Vec<ModelInfoDto>> {
    let mut trial = state.config.lock().await.clone();
    apply_save_input(&mut trial, &input)?;
    let service = build_service(&trial, state.store.clone())?;
    list_models_from(&service).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
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
    let has_key = cfg.profile_keys.contains_key(&profile.id)
        || (profile.provider == "copilot"
            && agentloop_sdk::providers::copilot::CopilotConfig::discoverable());
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
    } else if built.provider != "ollama"
        && !(built.provider == "copilot"
            && agentloop_sdk::providers::copilot::CopilotConfig::discoverable())
        && !cfg.profile_keys.contains_key(&id)
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

fn apply_save_input(
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
    } else if id != "ollama"
        && !(id == "copilot" && agentloop_sdk::providers::copilot::CopilotConfig::discoverable())
        && !cfg.keys.contains_key(id)
    {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    /// `never` | `optional` | `required` — falls back to prefs.default_isolation.
    pub isolation: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
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
    // Persisted immediately (not just cached in memory) so the baseline
    // survives an app restart before the session is ever resumed — this is
    // the fix for the original "changes vanish when you reopen the chat"
    // bug, where the in-memory-only map was lost on restart and
    // `resume_session` re-captured from the already-dirty tree.
    if meta.base_cwd.is_none() {
        if let Some(baseline) = capture_session_baseline(&meta.cwd) {
            let mut baselines = state.session_baselines.lock().await;
            baselines.insert(id.to_string(), baseline);
            crate::state::save_session_baselines(&baselines);
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> DesktopResult<Vec<SessionMeta>> {
    let service = require_service(&state).await?;
    Ok(service.list_sessions().await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn session_meta(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<SessionMeta> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.session_meta(&id).await?)
}

/// One-shot, tool-free title suggestion for a session's first turn (reference-
/// style semantic auto-title). Reuses the session's own model via the
/// `Provider::stream_chat` primitive directly — bypassing the full
/// session/tool/event-stream loop entirely, since this is a single
/// throwaway completion with no persistence, no tools, and no transcript.
/// Fire-and-forget from the caller's perspective: any failure (no model set,
/// provider error, empty output) surfaces as an `Err` and the caller should
/// just keep the existing title.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn suggest_session_title(
    state: State<'_, AppState>,
    session_id: String,
    prompt_text: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;
    let model = meta
        .model
        .ok_or_else(|| DesktopError::Message("session has no model set".into()))?;

    let registry = service.provider_registry();
    let (provider, model_id) = registry
        .resolve(&model)
        .ok_or_else(|| DesktopError::Message(format!("no provider for model {model}")))?;

    let truncated: String = prompt_text.chars().take(2000).collect();
    let system = "Summarize the user's task as a short title of 2-5 words. \
        Title Case, no punctuation, no quotes, no trailing period. \
        Reply with the title only — nothing else."
        .to_string();
    let mut request = ChatRequest::new(model_id, vec![Message::user(truncated)]);
    request.system = Some(system);
    request.max_tokens = Some(32);

    let cancel = CancellationToken::new();
    let mut stream = provider
        .stream_chat(request, cancel)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event.map_err(|e| DesktopError::Message(e.to_string()))? {
            ProviderStreamEvent::MarkdownDelta { text: delta } => {
                text.push_str(&delta);
            }
            ProviderStreamEvent::MessageEnd { .. } => break,
            _ => {}
        }
    }

    let title = text
        .trim()
        .trim_matches(['"', '\'', '.'])
        .trim()
        .to_string();
    if title.is_empty() {
        // Model returned nothing usable — fall back to a prompt prefix so the
        // session still gets a meaningful name instead of staying "New Agent".
        let fallback: String = prompt_text
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(60)
            .collect();
        if fallback.is_empty() {
            return Err(DesktopError::Message("empty title generated".into()));
        }
        tracing::debug!("title model returned empty; using prompt prefix fallback");
        return Ok(fallback);
    }
    Ok(title)
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

    // Backfill a session baseline on resume ONLY for a genuinely
    // baseline-less legacy session — one created before this feature
    // existed (or before persistence was added), which therefore has no
    // entry in the persisted map at all. With persistence in place, a
    // session created via `create_session` always already has a persisted
    // baseline by the time it's resumed, so this branch should not fire for
    // it. Critically, this must NEVER re-capture over an *existing*
    // baseline: doing so on every resume was the original bug — the
    // in-memory-only map was lost on every app restart, so this branch
    // always looked baseline-less and re-captured from the tree as it stood
    // at resume time, silently swallowing the session's own prior edits.
    // Now that baselines are persisted (see `create_session` and
    // `crate::state::{load,save}_session_baselines`), that map survives
    // restart, so this only ever fires once per legacy session — after
    // which its baseline is persisted and stable forever after (that
    // session will show nothing until further edits, which is acceptable:
    // strictly better than swallowing edits on every single resume).
    if result.is_ok() {
        let mut baselines = state.session_baselines.lock().await;
        let has_baseline = baselines.contains_key(id.as_str());
        if !has_baseline {
            if let Ok(meta) = service.session_meta(&id).await {
                if meta.base_cwd.is_none() {
                    if let Some(baseline) = capture_session_baseline(&meta.cwd) {
                        baselines.insert(id.to_string(), baseline);
                        crate::state::save_session_baselines(&baselines);
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

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let id = SessionId::from(session_id);
    if let Some(handle) = state.subscriptions.lock().await.remove(id.as_str()) {
        handle.abort();
    }
    let service = require_service(&state).await?;
    Ok(service.delete_session(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
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
        let session_key = key;
        let mut stream = stream;
        while let Some(event) = stream.next().await {
            if let Err(err) = app.emit("session-event", &event) {
                tracing::warn!(
                    session_id = %session_key.as_str(),
                    error = %err,
                    "session-event emit failed; ending subscription relay"
                );
                break;
            }
        }
        tracing::debug!(
            session_id = %session_key.as_str(),
            "session subscription stream ended"
        );
    });

    state.subscriptions.lock().await.insert(session_id, handle);
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn unsubscribe_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    if let Some(handle) = state.subscriptions.lock().await.remove(&session_id) {
        tracing::debug!(session_id = %session_id, "unsubscribing session event relay");
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
    /// `#[serde(default)]` so a missing / omitted field becomes `None` (engine
    /// Default) rather than failing the whole invoke — matches `effort` /
    /// `composer_mode` below.
    #[serde(default)]
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
        (_, "md" | "markdown") => "text/markdown".into(),
        (_, "json") => "application/json".into(),
        (_, "xml" | "svg") => "application/xml".into(),
        (_, "html" | "htm") => "text/html".into(),
        (_, "css") => "text/css".into(),
        (_, "yaml" | "yml") => "text/yaml".into(),
        (_, "toml") => "text/toml".into(),
        (
            _,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "rs" | "txt" | "py" | "pyi" | "go"
            | "java" | "kt" | "kts" | "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "cs"
            | "rb" | "php" | "swift" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "sql" | "r"
            | "lua" | "vim" | "el" | "clj" | "scala" | "rsx" | "svelte" | "vue" | "astro"
            | "gradle" | "dockerfile" | "makefile" | "cmake" | "ini" | "cfg" | "conf"
            | "env" | "gitignore" | "dockerignore" | "editorconfig" | "lock",
        ) => "text/plain".into(),
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
                    .map(|s| s.trim_end_matches('/').to_owned())
            })
            .unwrap_or_else(|| "attachment".into());
        if att.kind == "directory" {
            let display = att.path.trim_end_matches('/');
            parts.push(ContentBlock::markdown(format!(
                "Referenced directory: `{display}/`"
            )));
            continue;
        }
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

/// Appended in Plan mode (`composer_mode == "plan"`). Forces investigate-first:
/// the plan must be grounded in the real codebase, not a generic checklist of
/// "go look at the code" steps.
const FLEX_PLAN_PROMPT: &str = "\
Plan mode: INVESTIGATE before you plan. First use your read-only tools \
(SearchCode/FindSymbol, Read, and RepoMap when available) to find the actual \
files, symbols, and current behavior relevant to the task. Ground the plan in \
what you found — cite concrete file paths (with line ranges) and quote the real \
current code or strings you intend to change. Do NOT hand back a generic \
checklist of investigative steps (e.g. \"locate the code\", \"identify the \
logic\") — that investigation is your job to do now, before answering. Present \
a concrete, grounded plan only after you have actually explored the code.";

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn prompt(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let result = prompt_inner(state, input).await;
    if let Err(err) = &result {
        let msg = err.to_string();
        if msg.contains("already in progress") {
            // Expected control-flow, not a fault: the frontend recognizes this
            // marker and requeues the message (see useComposerSend's
            // TURN_IN_PROGRESS_MARKER) — keep it out of the ERROR log.
            tracing::debug!(error = %msg, "prompt rejected: turn in progress");
        } else {
            tracing::error!(error = %msg, "prompt failed");
        }
    }
    result
}

/// Builds the per-turn system-prompt notice that pins the model to *this*
/// session's own working directory.
///
/// Bug #50: `EngineService` bakes a single `Working directory: {{cwd}}` line
/// into its system prompt once, at `build_service` time
/// (`packages/desktop/src-tauri/src/compose.rs`), from the global
/// `cfg.prefs.cwd` preference — not from any particular session. Since one
/// `EngineService` backs every session in the desktop app, that baked line
/// can name a different repo than the session actually being prompted (e.g.
/// the last-selected project at startup, while this turn's session was
/// created against a different repo). This notice is appended per-turn from
/// the *target* session's own `meta.cwd` (see `prompt_inner` below, which
/// always resolves `meta` from `input.session_id`, never from any global or
/// "active" session) and is worded to unambiguously override that stale
/// baked-in line — see `packages/engine/prompts/system/00-identity.md`,
/// which now explicitly defers to this notice.
fn session_cwd_notice(cwd: &std::path::Path) -> String {
    format!(
        "Session working directory: {}. This is the ONLY working directory for \
         this session: use it for all relative paths, project context, and any \
         question about where the user's project lives. It authoritatively \
         overrides the 'Working directory at engine startup' line stated earlier \
         in this prompt, which reflects a different session (or none) and MUST \
         be ignored for this turn.",
        cwd.display()
    )
}

#[tracing::instrument(level = "debug", skip_all)]
async fn prompt_inner(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id.clone());
    let meta = service.session_meta(&id).await.ok();
    let cwd_notice = meta.as_ref().map(|meta| session_cwd_notice(&meta.cwd));
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
    if input.composer_mode.as_deref() == Some("plan") {
        system_append = Some(match system_append {
            Some(existing) => format!("{existing}\n\n{FLEX_PLAN_PROMPT}"),
            None => FLEX_PLAN_PROMPT.to_owned(),
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

#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn set_turn_permission_mode(
    state: State<'_, AppState>,
    session_id: String,
    mode: Option<String>,
) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let parsed = match mode.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(parse_permission_mode(Some(raw)).ok_or_else(|| {
            DesktopError::Message(format!("unknown permission mode: {raw}"))
        })?),
    };
    Ok(service.set_turn_permission_mode(&id, parsed)?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn is_configured(state: State<'_, AppState>) -> DesktopResult<bool> {
    let cfg = state.config.lock().await;
    let has_service = state.service.lock().await.is_some();
    Ok(cfg.is_ready() && has_service)
}

// ---------------------------------------------------------------------------
// GitHub Copilot device-flow sign-in. The private `device_code` never leaves
// AppState — the frontend only sees a session id plus the public user code
// and verification URI. On success the token is written to the shared
// `~/.config/github-copilot/apps.json` that VS Code / JetBrains / Copilot CLI
// also use.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotAuthStatus {
    pub signed_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotAuthStart {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn copilot_auth_status() -> DesktopResult<CopilotAuthStatus> {
    Ok(CopilotAuthStatus {
        signed_in: agentloop_sdk::providers::copilot::CopilotConfig::discoverable(),
    })
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn copilot_auth_start(state: State<'_, AppState>) -> DesktopResult<CopilotAuthStart> {
    use crate::state::PendingCopilotAuth;
    use agentloop_sdk::providers::copilot::DeviceFlow;

    let auth = DeviceFlow::new()
        .start()
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let session_id = format!("copilot-auth-{}", uuid_like_suffix());
    let view = CopilotAuthStart {
        session_id: session_id.clone(),
        user_code: auth.user_code.clone(),
        verification_uri: auth.verification_uri.clone(),
        expires_in: auth.expires_in,
    };

    let mut pending = state.pending_copilot_auth.lock().await;
    // A new start cancels any prior in-flight wait so only one dialog can
    // own the poll loop at a time.
    for (_, prior) in pending.drain() {
        prior.cancel.cancel();
    }
    pending.insert(
        session_id,
        PendingCopilotAuth {
            auth,
            cancel: CancellationToken::new(),
        },
    );
    Ok(view)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn copilot_auth_wait(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<CopilotAuthStatus> {
    use agentloop_sdk::providers::copilot::{DeviceFlow, store_github_token};

    let session_id = session_id.trim().to_owned();
    let (auth, cancel) = {
        let pending = state.pending_copilot_auth.lock().await;
        let entry = pending.get(&session_id).ok_or_else(|| {
            DesktopError::Message("copilot sign-in session not found — start a new sign-in".into())
        })?;
        (entry.auth.clone(), entry.cancel.clone())
    };

    let result = DeviceFlow::new().poll(&auth, cancel).await;
    // Drop the session either way so a cancelled/failed wait can't be
    // retried against an expired device code.
    state.pending_copilot_auth.lock().await.remove(&session_id);

    match result {
        Ok(token) => {
            store_github_token(&token).map_err(|e| DesktopError::Message(e.to_string()))?;
            Ok(CopilotAuthStatus { signed_in: true })
        }
        Err(agentloop_core::ProviderError::Cancelled { .. }) => {
            Err(DesktopError::Message("sign-in cancelled".into()))
        }
        Err(err) => Err(DesktopError::Message(err.to_string())),
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn copilot_auth_cancel(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    let session_id = session_id.trim();
    let mut pending = state.pending_copilot_auth.lock().await;
    if let Some(entry) = pending.remove(session_id) {
        entry.cancel.cancel();
    }
    Ok(())
}

/// Whether `cwd` is inside a git repository at all (`git rev-parse
/// --git-dir` succeeds), regardless of whether it has any commits yet. Used
/// to gate the entire git chrome (branch pill, changes badge, commit bar,
/// Changes tab content) — a non-git folder should show none of it, while a
/// freshly `git init`-ed repo with an unborn HEAD legitimately keeps it (`git
/// status --porcelain` still works fine with no commits yet, so treating that
/// as a normal state rather than an error is correct here too).
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_is_repo(cwd: String) -> bool {
    crate::win_console::command("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Whether `cwd`'s repo has at least one configured remote (`git remote`
/// prints a non-empty name). Gates Commit vs Commit & Push in the UI — no
/// remote means push would fail with "No configured push destination", so
/// the chrome must not offer push.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_has_remote(cwd: String) -> bool {
    crate::win_console::command("git")
        .args(["remote"])
        .current_dir(cwd)
        .output()
        .map(|out| {
            out.status.success()
                && !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        })
        .unwrap_or(false)
}

/// Read-only current-branch lookup for the composer context bar.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_branch(cwd: String) -> Option<String> {
    let output = crate::win_console::command("git")
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_list_branches(cwd: String) -> DesktopResult<Vec<String>> {
    let output = crate::win_console::command("git")
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_checkout(cwd: String, branch: String) -> DesktopResult<()> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(DesktopError::Message("invalid branch name".into()));
    }
    let output = crate::win_console::command("git")
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

/// Max rows returned to the UI by [`git_status`] / [`git_status_since_baseline`].
/// A session that scaffolds a large project (e.g. `create-next-app`) can dirty
/// hundreds of untracked files; rendering all of them as list rows is what
/// makes the Changes panel jank. The UI shows a "+N more" indicator instead of
/// mounting every row, and [`GitStatusSummary`]'s totals are always computed
/// over the *full* set so the aggregate +/- badge stays correct regardless of
/// the cap.
const MAX_STATUS_FILES: usize = 300;

/// Wraps a (possibly truncated) file list with totals computed over the full,
/// untruncated set — the aggregate +/- badge and file count must reflect
/// every changed file even when only the first [`MAX_STATUS_FILES`] rows are
/// sent to the UI for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSummary {
    /// First `MAX_STATUS_FILES` entries only — render these as rows.
    pub files: Vec<GitFileStatus>,
    /// Total number of changed files (tracked + untracked), untruncated.
    pub total_count: usize,
    /// Sum of `added` across every changed file, untruncated.
    pub total_added: u32,
    /// Sum of `removed` across every changed file, untruncated.
    pub total_removed: u32,
    /// `true` when `files` was truncated (`total_count > files.len()`).
    pub truncated: bool,
}

fn summarize(mut files: Vec<GitFileStatus>) -> GitStatusSummary {
    let total_count = files.len();
    let mut total_added = 0u32;
    let mut total_removed = 0u32;
    for f in &files {
        total_added += f.added.unwrap_or(0);
        total_removed += f.removed.unwrap_or(0);
    }
    let truncated = total_count > MAX_STATUS_FILES;
    files.truncate(MAX_STATUS_FILES);
    GitStatusSummary {
        files,
        total_count,
        total_added,
        total_removed,
        truncated,
    }
}

/// Read-only working-tree status for the Changes panel. Non-git dirs yield
/// an empty summary (mirrors `git_branch`'s tolerance). Capped at
/// [`MAX_STATUS_FILES`] rows; see [`GitStatusSummary`] for how totals stay
/// accurate past the cap.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_status(cwd: String) -> DesktopResult<GitStatusSummary> {
    Ok(summarize(git_status_full(&cwd)?))
}

/// Shared implementation behind [`git_status`] and
/// [`git_status_since_baseline`]. Returns the full, untruncated list —
/// callers cap/summarize via [`summarize`].
fn git_status_full(cwd: &str) -> DesktopResult<Vec<GitFileStatus>> {
    let porcelain = match crate::win_console::command("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Ok(Vec::new()),
    };

    // Line counts per changed file; binary files report "-" and are skipped.
    let mut counts: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    if let Ok(out) = crate::win_console::command("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(cwd)
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let mut parts = line.split('\t');
                let (Some(a), Some(r), Some(path)) = (parts.next(), parts.next(), parts.next())
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status_since_baseline(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<GitStatusSummary> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    let cwd_str = cwd.to_string_lossy().to_string();
    let cwd_path = cwd.clone();

    if base_cwd.is_some() {
        let cwd_for_git = cwd_str.clone();
        return tokio::task::spawn_blocking(move || summarize(git_status_full(&cwd_for_git)?))
            .await
            .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?;
    }

    let baseline = {
        let baselines = state.session_baselines.lock().await;
        baselines
            .get(&session_id)
            .map(|b| (b.head_sha.clone(), b.files.clone()))
    };
    let Some((baseline_head, baseline_files)) = baseline else {
        let cwd_for_git = cwd_str.clone();
        return tokio::task::spawn_blocking(move || summarize(git_status_full(&cwd_for_git)?))
            .await
            .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?;
    };

    tokio::task::spawn_blocking(move || {
        if current_head_sha(&cwd_path) != baseline_head {
            return Ok(summarize(git_status_full(&cwd_str)?));
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
                        hash_object(&cwd_path, &f.path).unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .collect();
        Ok(summarize(filtered))
    })
    .await
    .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?
}

/// `git hash-object <path>` relative to `cwd`; used to detect whether a
/// dirty path's content has changed since baseline capture. Returns `None`
/// on any git failure (missing file, not a git repo, etc.) so callers can
/// treat the path as "unknown" rather than failing outright.
fn hash_object(cwd: &std::path::Path, path: &str) -> Option<String> {
    let out = crate::win_console::command("git")
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
    crate::win_console::command("git")
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
    let porcelain = crate::win_console::command("git")
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
    let tracked = crate::win_console::command("git")
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
        let untracked = crate::win_console::command("git")
            .args(["diff", "--no-index", "--", "/dev/null", path])
            .current_dir(dir)
            .output()
            .map_err(|e| DesktopError::Message(format!("git diff failed: {e}")))?;
        match untracked.status.code() {
            Some(0) | Some(1) => {
                text = String::from_utf8_lossy(&untracked.stdout).to_string();
            }
            _ => {
                let stderr = String::from_utf8_lossy(&untracked.stderr)
                    .trim()
                    .to_string();
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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

    let add = crate::win_console::command("git")
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

    let commit = crate::win_console::command("git")
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

    let sha = crate::win_console::command("git")
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_push(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let push = crate::win_console::command("git")
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
// Commit center: selective staging + commit/push/branch/PR flow for the
// Changes tab (spec #48). Same isolated-session restriction as
// `git_commit`/`git_push` above — isolated sessions integrate their worktree
// back into the base repo instead of committing directly here.
// ---------------------------------------------------------------------------

/// Push the current branch, creating the upstream on first push (`git push
/// -u origin <branch>`) instead of failing with "no upstream branch". Shared
/// by `git_commit_and_push` and `git_create_pr`.
fn push_current_branch(cwd: &std::path::Path) -> DesktopResult<()> {
    let push = crate::win_console::command("git")
        .args(["push"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git push failed: {e}")))?;
    if push.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
    // "no upstream" is reported on stderr by git; retry with `-u origin
    // <branch>` rather than string-matching the exact wording, which varies
    // by git version/locale — instead just always retry once with `-u` on
    // any push failure that looks like a missing-upstream case.
    if stderr.contains("has no upstream branch") || stderr.contains("--set-upstream") {
        let branch = git_branch(cwd.to_string_lossy().to_string())
            .ok_or_else(|| DesktopError::Message("could not determine current branch".into()))?;
        let retry = crate::win_console::command("git")
            .args(["push", "-u", "origin", &branch])
            .current_dir(cwd)
            .output()
            .map_err(|e| DesktopError::Message(format!("git push -u failed: {e}")))?;
        if retry.status.success() {
            return Ok(());
        }
        let retry_stderr = String::from_utf8_lossy(&retry.stderr).trim().to_string();
        return Err(DesktopError::Message(if retry_stderr.is_empty() {
            "git push -u failed".into()
        } else {
            retry_stderr
        }));
    }
    Err(DesktopError::Message(if stderr.is_empty() {
        "git push failed".into()
    } else {
        stderr
    }))
}

/// Stage only `paths` (`git add -- <paths>`) then commit. Shared staging +
/// commit body for every commit-center entry point below. Rejects isolated
/// sessions and empty message/paths up front — same contract as `git_commit`.
async fn commit_selected_paths(
    state: &State<'_, AppState>,
    session_id: &str,
    message: &str,
    paths: &[String],
) -> DesktopResult<(PathBuf, String)> {
    let message = message.trim();
    if message.is_empty() {
        return Err(DesktopError::Message("commit message is required".into()));
    }
    if paths.is_empty() {
        return Err(DesktopError::Message(
            "select at least one file to commit".into(),
        ));
    }
    let mut relative_paths = Vec::with_capacity(paths.len());
    for p in paths {
        relative_paths.push(validate_repo_relative_path(p)?.to_string());
    }

    let (cwd, base_cwd) = review_dirs(state, session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let mut add_cmd = crate::win_console::command("git");
    add_cmd.arg("add").arg("--").args(&relative_paths);
    let add = add_cmd
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

    let commit = crate::win_console::command("git")
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

    let sha = crate::win_console::command("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&cwd)
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
    Ok((cwd, String::from_utf8_lossy(&sha.stdout).trim().to_string()))
}

/// Stage exactly the selected files and commit — the Changes tab's per-file
/// checkbox selection, unlike `git_commit`'s `git add -A`.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_commit_paths(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let (_cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    Ok(sha)
}

/// Commit the selected files, then push (creating the upstream if this is
/// the branch's first push).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_commit_and_push(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let (cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    push_current_branch(&cwd)?;
    Ok(sha)
}

/// Create and check out a new local branch, then commit the selected files
/// to it. The branch is created off the current HEAD (`git checkout -b`).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_create_branch_and_commit(
    state: State<'_, AppState>,
    session_id: String,
    branch: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(DesktopError::Message("invalid branch name".into()));
    }

    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let checkout = crate::win_console::command("git")
        .args(["checkout", "-b", branch])
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git checkout -b failed: {e}")))?;
    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            format!("git checkout -b {branch} failed")
        } else {
            stderr
        }));
    }

    let (_cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    Ok(sha)
}

/// Commit the selected files, push the branch, then open a PR via `gh pr
/// create --fill` (or with an explicit title/body when given). Gracefully
/// degrades when the GitHub CLI isn't installed or isn't authenticated: the
/// branch is still pushed (so the commit is never stranded locally-only) and
/// the returned message tells the UI to show "GitHub CLI not available —
/// pushed the branch instead" rather than silently losing the PR step.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_create_pr(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
    title: Option<String>,
    body: Option<String>,
) -> DesktopResult<CreatePrOutcome> {
    let (cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    push_current_branch(&cwd)?;

    let gh_check = crate::win_console::command("gh")
        .args(["auth", "status"])
        .current_dir(&cwd)
        .output();
    let gh_available = matches!(&gh_check, Ok(out) if out.status.success());
    if !gh_available {
        return Ok(CreatePrOutcome {
            commit_sha: sha,
            pr_url: None,
            degraded_reason: Some("GitHub CLI not available — pushed the branch instead".into()),
        });
    }

    let mut pr_cmd = crate::win_console::command("gh");
    pr_cmd.arg("pr").arg("create");
    match (&title, &body) {
        (Some(t), Some(b)) if !t.trim().is_empty() => {
            pr_cmd.arg("--title").arg(t).arg("--body").arg(b);
        }
        (Some(t), None) if !t.trim().is_empty() => {
            pr_cmd.arg("--title").arg(t).arg("--body").arg("");
        }
        _ => {
            pr_cmd.arg("--fill");
        }
    }
    let pr = pr_cmd
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("gh pr create failed: {e}")))?;
    if !pr.status.success() {
        let stderr = String::from_utf8_lossy(&pr.stderr).trim().to_string();
        // The push already succeeded above, so degrade rather than error —
        // the commit/push is not lost even though the PR step failed (e.g.
        // a PR already exists for this branch, or `gh` isn't authenticated
        // for this repo's host).
        return Ok(CreatePrOutcome {
            commit_sha: sha,
            pr_url: None,
            degraded_reason: Some(if stderr.is_empty() {
                "gh pr create failed — pushed the branch instead".into()
            } else {
                format!("gh pr create failed — pushed the branch instead ({stderr})")
            }),
        });
    }
    let url = String::from_utf8_lossy(&pr.stdout).trim().to_string();
    Ok(CreatePrOutcome {
        commit_sha: sha,
        pr_url: (!url.is_empty()).then_some(url),
        degraded_reason: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrOutcome {
    pub commit_sha: String,
    pub pr_url: Option<String>,
    /// Set when the PR step itself was skipped/failed but the commit+push
    /// still succeeded (e.g. `gh` missing/unauthenticated) — the UI shows
    /// this as a non-fatal toast rather than treating the call as an error.
    pub degraded_reason: Option<String>,
}

/// One-shot, tool-free commit-message suggestion from a diff summary —
/// same throwaway-completion pattern as `suggest_session_title` (no tools,
/// no persistence, no transcript). Any failure (no model set, provider
/// error, empty output) surfaces as an `Err`; callers should just leave the
/// message box empty rather than block the commit flow on this.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn suggest_commit_message(
    state: State<'_, AppState>,
    session_id: String,
    diff_summary: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;
    let model = meta
        .model
        .ok_or_else(|| DesktopError::Message("session has no model set".into()))?;

    let registry = service.provider_registry();
    let (provider, model_id) = registry
        .resolve(&model)
        .ok_or_else(|| DesktopError::Message(format!("no provider for model {model}")))?;

    let truncated: String = diff_summary.chars().take(4000).collect();
    let system = "Write a concise, imperative-mood git commit message (like \"Fix\", \
        \"Add\", \"Update\") summarizing the diff below. One line, under 72 characters, \
        no trailing period, no quotes. Reply with the commit message only — nothing else."
        .to_string();
    let mut request = ChatRequest::new(model_id, vec![Message::user(truncated)]);
    request.system = Some(system);
    request.max_tokens = Some(64);

    let cancel = CancellationToken::new();
    let mut stream = provider
        .stream_chat(request, cancel)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event.map_err(|e| DesktopError::Message(e.to_string()))? {
            ProviderStreamEvent::MarkdownDelta { text: delta } => {
                text.push_str(&delta);
            }
            ProviderStreamEvent::MessageEnd { .. } => break,
            _ => {}
        }
    }

    let suggestion = text.trim().trim_matches(['"', '\'']).trim().to_string();
    if suggestion.is_empty() {
        return Err(DesktopError::Message(
            "empty commit message generated".into(),
        ));
    }
    Ok(suggestion)
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

fn normalize_path_slashes(s: &str) -> String {
    s.replace('\\', "/")
}

/// Strip `worktree` from an absolute `path`, returning a forward-slashed
/// relative path. Lexical first (so deleted files still resolve), then
/// canonicalize when both sides exist (symlinks / Windows drive casing).
fn strip_worktree_prefix(path: &std::path::Path, worktree: &std::path::Path) -> DesktopResult<PathBuf> {
    if let Ok(rel) = path.strip_prefix(worktree) {
        return Ok(rel.to_path_buf());
    }

    let path_s = normalize_path_slashes(&path.to_string_lossy());
    let mut root_s = normalize_path_slashes(&worktree.to_string_lossy());
    while root_s.ends_with('/') {
        root_s.pop();
    }
    if path_s == root_s {
        return Ok(PathBuf::new());
    }
    let prefix = format!("{root_s}/");
    if let Some(rest) = path_s.strip_prefix(&prefix) {
        return Ok(PathBuf::from(rest));
    }
    // Windows FS is case-insensitive — tool `file_path`s and SessionMeta.cwd
    // often disagree on drive-letter casing (`C:\` vs `c:\`).
    #[cfg(windows)]
    {
        let path_l = path_s.to_ascii_lowercase();
        let root_l = root_s.to_ascii_lowercase();
        if path_l == root_l {
            return Ok(PathBuf::new());
        }
        let prefix_l = format!("{root_l}/");
        if let Some(rest) = path_l.strip_prefix(&prefix_l) {
            // Preserve the caller's casing from the original path suffix.
            return Ok(PathBuf::from(&path_s[path_s.len() - rest.len()..]));
        }
    }

    if let (Ok(c_path), Ok(c_root)) = (path.canonicalize(), worktree.canonicalize()) {
        if let Ok(rel) = c_path.strip_prefix(&c_root) {
            return Ok(rel.to_path_buf());
        }
    }

    Err(DesktopError::Message(format!(
        "file is outside the session workspace (`{}`)",
        worktree.display()
    )))
}

/// Accept a repo-relative path *or* an absolute path under `worktree`
/// (Write/Edit tool inputs are always absolute). Returns a forward-slashed
/// relative path for `git … -- <path>`. Isolation is irrelevant — non-isolated
/// sessions still have absolute tool paths that must strip against `cwd`.
fn resolve_review_path(path: &str, worktree: &std::path::Path) -> DesktopResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("path is required".into()));
    }
    let as_path = std::path::Path::new(trimmed);
    let relative = if as_path.is_absolute() {
        strip_worktree_prefix(as_path, worktree)?
    } else {
        validate_repo_relative_path(trimmed)?;
        PathBuf::from(trimmed)
    };
    if relative.as_os_str().is_empty() {
        return Err(DesktopError::Message(
            "path must be a file inside the session workspace, not the workspace root".into(),
        ));
    }
    if relative
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DesktopError::Message(format!(
            "path must not contain '..': {trimmed}"
        )));
    }
    Ok(normalize_path_slashes(&relative.to_string_lossy()))
}

/// Two-letter `git status --porcelain` code for a single path (e.g. `"??"`,
/// `" M"`, `"D "`), or `None` if the path has no pending changes.
fn porcelain_code(dir: &std::path::Path, path: &str) -> DesktopResult<Option<String>> {
    let out = crate::win_console::command("git")
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
    let out = crate::win_console::command("git")
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_undo_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let (dir, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &dir)?;

    if let Some(code) = porcelain_code(&dir, &path)? {
        if code == "??" {
            let full = dir.join(&path);
            return std::fs::remove_file(&full).map_err(|e| {
                DesktopError::Message(format!(
                    "failed to delete untracked file `{}`: {e}",
                    full.display()
                ))
            });
        }
    }

    let checkout_from = |rev: &str| -> DesktopResult<std::process::Output> {
        crate::win_console::command("git")
            .args(["-C"])
            .arg(&dir)
            .args(["checkout", rev, "--", &path])
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_keep_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &worktree)?;
    let Some(base_dir) = base_cwd else {
        return Err(DesktopError::Message("session is not isolated".into()));
    };

    let src = worktree.join(&path);
    let dst = base_dir.join(&path);

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
#[tracing::instrument(level = "debug", skip_all, err)]
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

    let result = crate::win_console::command("git").args(&args).output();

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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_file_diff(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<String> {
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &worktree)?;

    let base_head = match &base_cwd {
        Some(base_dir) => base_head_sha(base_dir)?,
        None => "HEAD".to_string(),
    };
    diff_against_rev(&worktree, &base_head, &path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// Path relative to `cwd`, forward-slashed.
    pub path: String,
    /// Basename, shown as the primary label.
    pub name: String,
    /// True when the hit is a directory. Always `false` for `list_files`
    /// (files only — dirs are not @-mentionable and inflate walk cost).
    #[serde(default)]
    pub is_dir: bool,
}

/// Directory basenames we never descend into, even if a project forgot to
/// gitignore them — walking `node_modules` / build outputs is what made
/// composer `@` feel multi-second on ordinary apps.
const SKIP_DIR_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".output",
    ".turbo",
    ".cache",
    ".parcel-cache",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
    "Pods",
    ".svelte-kit",
    ".vercel",
    ".idea",
    ".vscode",
];

fn is_skipped_dir_name(name: &str) -> bool {
    SKIP_DIR_NAMES
        .iter()
        .any(|skip| name.eq_ignore_ascii_case(skip))
}

/// Rank a path against a lowercase needle. Lower is better; `None` = no match.
/// Basename prefix/contains beat full-path contains. Subsequence matching is
/// intentionally omitted — it matched almost everything and made ranking +
/// result lists feel random/laggy.
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
    None
}

/// Read-only fuzzy **file** search under `cwd` for composer @-mentions and
/// the Files explorer. Scoped to the session project folder only: respects
/// `.gitignore` / `.ignore` / `.git/exclude`, never descends into
/// `node_modules` (and other heavy dirs), returns files only (no folders).
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn list_files(cwd: String, query: String) -> Vec<FileHit> {
    let root = PathBuf::from(&cwd);
    if !root.is_dir() {
        return Vec::new();
    }
    let needle = query.trim().to_lowercase();

    // Bound the walk so huge repos can't stall an interactive keystroke.
    // Empty query is browse-only (shallow); typed queries may go deeper but
    // still stop early once we have enough strong hits.
    const MAX_HITS: usize = 40;
    const MAX_WALK_BROWSE: usize = 2_000;
    const MAX_WALK_SEARCH: usize = 8_000;
    let max_walk = if needle.is_empty() {
        MAX_WALK_BROWSE
    } else {
        MAX_WALK_SEARCH
    };

    let mut hits: Vec<(i32, FileHit)> = Vec::new();
    let mut walked = 0usize;
    let mut strong_hits = 0usize; // basename prefix/contains

    let mut builder = ignore::WalkBuilder::new(&root);
    builder
        .standard_filters(true) // hidden + gitignore + .ignore + exclude
        .parents(true)
        .follow_links(false)
        .filter_entry(|entry| {
            // Prune before descending — gitignore alone still lets a missing
            // ignore rule walk tens of thousands of node_modules files.
            if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if is_skipped_dir_name(&name) {
                    return false;
                }
            }
            true
        });
    // Bare `@` / empty Files search: only shallow files so we don't walk the
    // whole tree just to show a browse list.
    if needle.is_empty() {
        builder.max_depth(Some(3));
    }

    for entry in builder.build().flatten() {
        walked += 1;
        if walked > max_walk {
            break;
        }
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() {
            continue;
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_str.clone());
        let Some(score) = score_file(&rel_str, &name, &needle) else {
            continue;
        };
        if score <= 1 {
            strong_hits += 1;
        }
        hits.push((
            score,
            FileHit {
                path: rel_str,
                name,
                is_dir: false,
            },
        ));
        // Enough good basename matches — don't keep walking for weak path hits.
        if !needle.is_empty() && strong_hits >= MAX_HITS {
            break;
        }
        if hits.len() >= MAX_HITS * 4 {
            break;
        }
    }

    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.path.len().cmp(&b.1.path.len()))
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    hits.truncate(MAX_HITS);
    hits.into_iter().map(|(_, h)| h).collect()
}

#[cfg(test)]
mod list_files_ranking_tests {
    use super::{is_skipped_dir_name, score_file};

    #[test]
    fn skips_heavy_vendor_dirs() {
        assert!(is_skipped_dir_name("node_modules"));
        assert!(is_skipped_dir_name("NODE_MODULES"));
        assert!(is_skipped_dir_name(".git"));
        assert!(is_skipped_dir_name("target"));
        assert!(!is_skipped_dir_name("src"));
    }

    #[test]
    fn scores_basename_prefix_best() {
        assert_eq!(score_file("pkg/App.tsx", "App.tsx", "app"), Some(0));
        assert_eq!(score_file("pkg/MyApp.tsx", "MyApp.tsx", "app"), Some(1));
        assert_eq!(score_file("app/page.tsx", "page.tsx", "app"), Some(2));
        assert_eq!(score_file("pkg/page.tsx", "page.tsx", "zzz"), None);
    }

    #[test]
    fn empty_needle_is_browse_mode() {
        assert_eq!(score_file("a.ts", "a.ts", ""), Some(100));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfoDto {
    pub name: String,
    pub description: String,
    pub args_hint: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn is_isolated(state: State<'_, AppState>, session_id: String) -> DesktopResult<bool> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.is_isolated(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn integrate_session(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<IntegrationOutcome> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.integrate_session(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn discard_session(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    Ok(service.discard_session(&id).await?)
}

#[tracing::instrument(level = "debug", skip_all, err)]
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
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_list() -> DesktopResult<Vec<RoutineDto>> {
    let store = routine_store()?;
    let mut specs = RoutineStore::list(&store)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs.into_iter().map(routine_spec_to_dto).collect())
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn routines_remove(id: String) -> DesktopResult<()> {
    let id = validate_routine_id(&id)?;
    let store = routine_store()?;
    RoutineStore::remove(&store, id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))
}

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
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
                let stop_reason = serde_json::to_value(record.outcome.stop_reason)
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
    /// Non-secret environment variables (persisted in the MCP TOML file).
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Secret environment values to write into the encrypted secrets store.
    /// On upsert: non-empty values overwrite; empty string keeps the existing
    /// secret (configure dialog). On list responses this map is always empty
    /// — see [`configured_secret_env`].
    #[serde(default)]
    pub secret_env: std::collections::BTreeMap<String, String>,
    /// Secret positional-arg values appended after `args` at resolve time
    /// (e.g. a Postgres connection string). Same keep-if-empty semantics as
    /// `secret_env` when `has_secret_args` is already true; omitted/`None` on
    /// list. Frontend sends these only for catalog installs that declare
    /// secret `argKeys`.
    #[serde(default)]
    pub secret_args: Option<Vec<String>>,
    /// Env key names that currently have a stored secret (values never
    /// returned). Populated on list; ignored on upsert.
    #[serde(default)]
    pub configured_secret_env: Vec<String>,
    /// Whether a secret positional-args suffix is stored for this server.
    #[serde(default)]
    pub has_secret_args: bool,
    pub enabled: bool,
}

fn mcp_dto_to_config(dto: &McpServerDto) -> agentloop_sdk::McpServerConfig {
    agentloop_sdk::McpServerConfig {
        name: dto.id.clone(),
        display_name: None,
        enabled: dto.enabled,
        transport: agentloop_sdk::McpServerTransport::Stdio(agentloop_sdk::StdioServerConfig {
            command: dto.command.clone(),
            args: dto.args.clone(),
            // Only non-secret env lands in the TOML file. Secrets are merged
            // back at resolve time (compose / mcp_test) from the encrypted store.
            env: dto.env.clone(),
            cwd: None,
        }),
        tool_name_prefix: None,
    }
}

fn mcp_config_to_dto(config: agentloop_sdk::McpServerConfig) -> McpServerDto {
    let (command, args, mut env) = match config.transport {
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

    // Migrate any plaintext credential-looking env vars still sitting in the
    // TOML (installed before secret storage landed) into the encrypted store,
    // then strip them from the DTO so the renderer never sees the values.
    let mut migrated_secrets = std::collections::BTreeMap::new();
    let secret_keys: Vec<String> = env
        .keys()
        .filter(|k| crate::config::is_likely_secret_env_name(k))
        .cloned()
        .collect();
    for key in secret_keys {
        if let Some(value) = env.remove(&key) {
            if !value.trim().is_empty() {
                migrated_secrets.insert(key, value);
            }
        }
    }
    if !migrated_secrets.is_empty() {
        match crate::config::upsert_mcp_server_secrets(
            &config.name,
            &migrated_secrets,
            false,
            None,
        ) {
            Ok(()) => {
                // Rewrite the TOML without the migrated secrets so they don't
                // linger on disk in plaintext (sync write — avoids nesting
                // `block_on` inside the async `mcp_list` path).
                if let Some(dir) = agentloop_sdk::mcp_store::default_mcp_dir() {
                    let cleaned = agentloop_sdk::McpServerConfig {
                        name: config.name.clone(),
                        display_name: config.display_name.clone(),
                        enabled: config.enabled,
                        transport: agentloop_sdk::McpServerTransport::Stdio(
                            agentloop_sdk::StdioServerConfig {
                                command: command.clone(),
                                args: args.clone(),
                                env: env.clone(),
                                cwd: None,
                            },
                        ),
                        tool_name_prefix: config.tool_name_prefix.clone(),
                    };
                    match toml::to_string_pretty(&cleaned) {
                        Ok(content) => {
                            let path = dir.join(format!("{}.toml", config.name));
                            if let Err(err) = std::fs::write(&path, content) {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %err,
                                    "failed to rewrite MCP TOML after secret migration"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                server = %config.name,
                                error = %err,
                                "failed to serialize MCP TOML after secret migration"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    server = %config.name,
                    error = %err,
                    "failed to migrate plaintext MCP env secrets into encrypted store"
                );
                // Put them back so the server still works this session even if
                // the secrets write failed.
                for (k, v) in migrated_secrets {
                    env.insert(k, v);
                }
            }
        }
    }

    let configured_secret_env =
        crate::config::list_mcp_configured_secret_env(&config.name).unwrap_or_default();
    let has_secret_args =
        crate::config::mcp_has_secret_args_suffix(&config.name).unwrap_or(false);

    McpServerDto {
        id: config.name,
        command,
        args,
        env,
        secret_env: std::collections::BTreeMap::new(),
        secret_args: None,
        configured_secret_env,
        has_secret_args,
        enabled: config.enabled,
    }
}

fn validate_mcp_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("server id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
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

#[tracing::instrument(level = "debug", skip_all, err)]
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_upsert(state: State<'_, AppState>, server: McpServerDto) -> DesktopResult<()> {
    validate_mcp_id(&server.id)?;
    if server.command.trim().is_empty() {
        return Err(DesktopError::Message("command is required".into()));
    }

    // Split credential-looking keys out of the plaintext `env` map as a
    // safety net (manual "Add server" form, or a catalog client that forgot
    // to use `secretEnv`). Catalog installs should already send them in
    // `secret_env`.
    let mut plaintext_env = server.env.clone();
    let mut secret_env = server.secret_env.clone();
    let leaked: Vec<String> = plaintext_env
        .keys()
        .filter(|k| crate::config::is_likely_secret_env_name(k))
        .cloned()
        .collect();
    for key in leaked {
        if let Some(value) = plaintext_env.remove(&key) {
            secret_env.entry(key).or_insert(value);
        }
    }

    let args_suffix = server.secret_args.as_deref();
    crate::config::upsert_mcp_server_secrets(
        &server.id,
        &secret_env,
        /* replace_env */ false,
        args_suffix,
    )?;

    let dto_for_toml = McpServerDto {
        env: plaintext_env,
        secret_env: std::collections::BTreeMap::new(),
        secret_args: None,
        configured_secret_env: Vec::new(),
        has_secret_args: false,
        ..server
    };
    let config = mcp_dto_to_config(&dto_for_toml);
    let store = mcp_store()?;
    store
        .upsert(config)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    rebuild_service_after_mcp_change(&state).await;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_remove(state: State<'_, AppState>, id: String) -> DesktopResult<()> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    store
        .remove(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;
    if let Err(err) = crate::config::clear_mcp_server_secrets(id) {
        tracing::warn!(server = %id, error = %err, "failed to clear MCP secrets on remove");
    }
    rebuild_service_after_mcp_change(&state).await;
    Ok(())
}

/// Connect to a saved server and list its tools — the "Test" button in the
/// UI. Talks to the MCP client directly (not through `McpManager`, which
/// keys server lookups off its own config snapshot) and never touches
/// `state.service`, so testing never disturbs the live engine or requires a
/// provider to be configured yet.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn mcp_test(id: String) -> DesktopResult<Vec<String>> {
    let id = validate_mcp_id(&id)?;
    let store = mcp_store()?;
    let server = store
        .get(id)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?
        .ok_or_else(|| DesktopError::Message(format!("server `{id}` not found")))?;
    let server = crate::compose::resolve_mcp_server_secrets(server);

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
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn user_identity(_state: State<'_, AppState>) -> DesktopResult<UserIdentityDto> {
    let git_name = crate::win_console::command("git")
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
// inside the session's cwd. Traversal-hardened: canonicalize the cwd, join
// the (resolved) relative path, then verify the written file's parent still
// sits inside the canonical cwd. Absolute Write/Edit-style paths are
// accepted via [`resolve_review_path`] and stripped to repo-relative first.
// ---------------------------------------------------------------------------

/// Hard cap for the Files (Monaco) editor — keeps the UI responsive and
/// rejects accidental binary dumps. Matches ~ Cursor's soft ceiling for
/// opening source in the side editor.
const READ_TEXT_MAX_BYTES: u64 = 1_500_000;

/// Reads a UTF-8 text file relative to `session_id`'s cwd. Same path
/// sanitation as [`save_text_file`]. Rejects files larger than
/// [`READ_TEXT_MAX_BYTES`] and non-UTF-8 content.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn read_text_file(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;
    let cwd = meta.cwd.clone();

    tokio::task::spawn_blocking(move || {
        let canonical_cwd = cwd
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;
        let target = canonical_cwd.join(&relative);

        let canonical_target = target.canonicalize().map_err(|e| {
            DesktopError::Message(format!("cannot open `{}`: {e}", target.display()))
        })?;
        if !canonical_target.starts_with(&canonical_cwd) {
            return Err(DesktopError::Message(
                "resolved path escapes the session's working directory".into(),
            ));
        }
        if !canonical_target.is_file() {
            return Err(DesktopError::Message(format!(
                "`{}` is not a file",
                relative
            )));
        }

        let meta = std::fs::metadata(&canonical_target)
            .map_err(|e| DesktopError::Message(format!("cannot stat `{}`: {e}", relative)))?;
        if meta.len() > READ_TEXT_MAX_BYTES {
            return Err(DesktopError::Message(format!(
                "`{relative}` is too large to open in the editor ({} bytes, max {})",
                meta.len(),
                READ_TEXT_MAX_BYTES
            )));
        }

        let bytes = std::fs::read(&canonical_target)
            .map_err(|e| DesktopError::Message(format!("cannot read `{relative}`: {e}")))?;
        if bytes.contains(&0) {
            return Err(DesktopError::Message(format!(
                "`{relative}` looks binary — open it in an external editor"
            )));
        }
        String::from_utf8(bytes).map_err(|_| {
            DesktopError::Message(format!(
                "`{relative}` is not valid UTF-8 — open it in an external editor"
            ))
        })
    })
    .await
    .map_err(|e| DesktopError::Message(format!("read join: {e}")))?
}

/// Writes `content` to `relative_path` inside `session_id`'s cwd, creating
/// parent directories as needed (e.g. a not-yet-existing `plans/` folder).
/// Returns the absolute path written. Used by the Plan tab's "Save to
/// Workspace" menu item (`PlanToolbar`); the frontend passes
/// `plans/<slug>-<date>.md`.
#[tracing::instrument(level = "debug", skip_all, err)]
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

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;

    let canonical_cwd = meta
        .cwd
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;

    let target = canonical_cwd.join(&relative);
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

/// Extensions accepted by `write_temp_blob` — kept in sync with the
/// composer's paste/drop image filter (`composerAttachments.ts`'s
/// `extForMimeType`) and the file-picker's image filter (`Composer.tsx`'s
/// `handlePick`).
const TEMP_BLOB_ALLOWED_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// Hard cap on a pasted/dropped image blob, matching the size a user could
/// plausibly paste from a screenshot tool — generous enough for real
/// screenshots, small enough to keep the temp dir from filling up.
const TEMP_BLOB_MAX_BYTES: usize = 20 * 1024 * 1024;

/// Persists a pasted/dropped image blob (raw bytes from the composer's
/// clipboard/drag handler — see `composerAttachments.ts::attachImageBlob`) to
/// a uniquely-named file in the OS temp dir and returns the absolute path.
/// This is the only way to turn an in-memory clipboard blob into a
/// `PromptAttachment.path` the engine can read (`build_prompt_input` maps
/// attachments straight to `BlobSource::Path`) — there is no in-memory/base64
/// attachment path today.
///
/// `ext` is validated against an allowlist (rejects anything but
/// png/jpg/jpeg/gif/webp) and `bytes` is capped at `TEMP_BLOB_MAX_BYTES` to
/// keep this from being used to dump arbitrary/oversized files into temp.
/// Callers are responsible for cleanup — these are ordinary temp files, not
/// tracked or garbage-collected by the engine.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn write_temp_blob(bytes: Vec<u8>, ext: String) -> DesktopResult<String> {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if !TEMP_BLOB_ALLOWED_EXTS.contains(&ext.as_str()) {
        return Err(DesktopError::Message(format!(
            "unsupported image extension `{ext}` (expected one of: {})",
            TEMP_BLOB_ALLOWED_EXTS.join(", ")
        )));
    }
    if bytes.is_empty() {
        return Err(DesktopError::Message("blob is empty".into()));
    }
    if bytes.len() > TEMP_BLOB_MAX_BYTES {
        return Err(DesktopError::Message(format!(
            "image is too large ({} bytes, max {})",
            bytes.len(),
            TEMP_BLOB_MAX_BYTES
        )));
    }

    let file_name = format!(
        "flex-paste-{}-{}.{ext}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let target = std::env::temp_dir().join(file_name);
    std::fs::write(&target, &bytes)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", target.display())))?;

    Ok(target.display().to_string())
}

/// Absolute path of the backend's rolling debug log file (see
/// `lib.rs::init_tracing`), for the Settings Diagnostics section's "copy
/// log path" / "open logs folder" affordance.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn debug_log_path() -> String {
    crate::debug::log_file_path()
}

/// App version baked into the desktop crate (mirrors `tauri.conf.json`).
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Writes a diagnostics bundle (frontend payload + backend log tail +
/// version/OS metadata) into the app log directory. Does **not** require an
/// active session — unlike `save_text_file` / the older debug-log export —
/// so Settings → Diagnostics works on a fresh install. Returns the absolute
/// path written.
///
/// Remote crash reporting (Sentry DSN etc.) is deliberately not wired: keep
/// the payload local until a DSN + privacy review land. The frontend's
/// opt-in "crash reporting" toggle only controls whether uncaught errors
/// are retained in the in-memory crash ring included in `frontend_payload`.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn export_diagnostics_bundle(
    app: tauri::AppHandle,
    frontend_payload: String,
) -> DesktopResult<String> {
    use std::io::{Read, Seek, SeekFrom};
    use tauri::Manager;

    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let log_path = crate::debug::log_file_path();
    let backend_tail = {
        const MAX_TAIL: u64 = 256 * 1024;
        match std::fs::File::open(&log_path) {
            Ok(mut f) => {
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                if len > MAX_TAIL {
                    let _ = f.seek(SeekFrom::End(-(MAX_TAIL as i64)));
                }
                let mut buf = String::new();
                let _ = f.read_to_string(&mut buf);
                buf
            }
            Err(_) => "(backend log unavailable)".to_string(),
        }
    };

    let stamp = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    };

    let body = format!(
        "# Desktop diagnostics export — {stamp}\n\
         version: {version}\n\
         os: {os}/{arch}\n\
         backend_log: {log_path}\n\
         \n\
         ## Frontend payload\n\
         {frontend_payload}\n\
         \n\
         ## Backend log (tail ≤256KiB)\n\
         {backend_tail}\n"
    );

    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| DesktopError::Message(format!("cannot resolve app log dir: {e}")))?;
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| DesktopError::Message(format!("cannot create log dir: {e}")))?;

    let target = log_dir.join(format!("diagnostics-{stamp}.txt"));
    std::fs::write(&target, body)
        .map_err(|e| DesktopError::Message(format!("cannot write {}: {e}", target.display())))?;

    Ok(target.display().to_string())
}

/// Poll the on-disk code index for `cwd` without rebuilding. Status lives
/// in app-data (`agentloop/index/<hash>`), never in the user's repo — no
/// AgentEvent is emitted (avoids schema churn).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn index_status(cwd: String) -> DesktopResult<agentloop_sdk::index::IndexStatus> {
    let path = PathBuf::from(cwd.trim());
    if path.as_os_str().is_empty() {
        return Err(DesktopError::Message("cwd is required".to_owned()));
    }
    tokio::task::spawn_blocking(move || agentloop_sdk::index::status_for(&path))
        .await
        .map_err(|e| DesktopError::Message(format!("index_status worker failed: {e}")))?
        .map_err(|e| DesktopError::Message(e.to_string()))
}

/// Force a (re)build of the code index for `cwd`. Returns status + update
/// stats so Settings can show progress after a rebuild click.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn index_rebuild(cwd: String) -> DesktopResult<IndexRebuildResult> {
    let path = PathBuf::from(cwd.trim());
    if path.as_os_str().is_empty() {
        return Err(DesktopError::Message("cwd is required".to_owned()));
    }
    let (status, stats) =
        tokio::task::spawn_blocking(move || agentloop_sdk::index::rebuild_with_stats(&path))
            .await
            .map_err(|e| DesktopError::Message(format!("index_rebuild worker failed: {e}")))?
            .map_err(DesktopError::Message)?;
    Ok(IndexRebuildResult { status, stats })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRebuildResult {
    pub status: agentloop_sdk::index::IndexStatus,
    pub stats: agentloop_sdk::index::UpdateStats,
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
mod resolve_review_path_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn keeps_repo_relative_paths() {
        let root = PathBuf::from("/repo");
        assert_eq!(
            resolve_review_path("packages/desktop/src/App.tsx", &root).unwrap(),
            "packages/desktop/src/App.tsx"
        );
    }

    #[test]
    fn strips_absolute_path_under_worktree() {
        let root = PathBuf::from("/repo");
        assert_eq!(
            resolve_review_path("/repo/packages/desktop/src/App.tsx", &root).unwrap(),
            "packages/desktop/src/App.tsx"
        );
    }

    #[test]
    fn rejects_absolute_path_outside_worktree() {
        let root = PathBuf::from("/repo");
        let err = resolve_review_path("/other/file.rs", &root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("outside the session workspace"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_parent_dir_segments() {
        let root = PathBuf::from("/repo");
        assert!(resolve_review_path("../secret", &root).is_err());
    }
}

    /// `summarize` must cap rendered rows at `MAX_STATUS_FILES` while keeping
    /// totals (count/added/removed) computed over the *full*, untruncated
    /// set — otherwise the aggregate +/- badge would silently undercount once
    /// a session's changes exceed the row cap (e.g. after scaffolding a
    /// project with hundreds of new files).
    #[test]
    fn summarize_caps_rows_but_keeps_full_totals() {
        let n = MAX_STATUS_FILES + 50;
        let files: Vec<GitFileStatus> = (0..n)
            .map(|i| GitFileStatus {
                path: format!("file_{i}.txt"),
                status: "?".to_string(),
                added: Some(1),
                removed: Some(0),
            })
            .collect();

        let summary = summarize(files);
        assert_eq!(summary.files.len(), MAX_STATUS_FILES);
        assert_eq!(summary.total_count, n);
        assert_eq!(summary.total_added, n as u32);
        assert_eq!(summary.total_removed, 0);
        assert!(summary.truncated);
    }

    /// Below the cap, nothing is truncated and totals match the row count.
    #[test]
    fn summarize_untruncated_when_under_cap() {
        let files: Vec<GitFileStatus> = (0..5)
            .map(|i| GitFileStatus {
                path: format!("file_{i}.txt"),
                status: "M".to_string(),
                added: Some(2),
                removed: Some(1),
            })
            .collect();

        let summary = summarize(files);
        assert_eq!(summary.files.len(), 5);
        assert_eq!(summary.total_count, 5);
        assert_eq!(summary.total_added, 10);
        assert_eq!(summary.total_removed, 5);
        assert!(!summary.truncated);
    }

    #[test]
    fn git_has_remote_false_without_remotes() {
        let dir = std::env::temp_dir().join(format!(
            "flex-git-has-remote-none-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let out = crate::win_console::command("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(
            !git_has_remote(dir.to_string_lossy().into_owned()),
            "fresh init must report no remotes"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn git_has_remote_true_with_origin() {
        let dir = std::env::temp_dir().join(format!(
            "flex-git-has-remote-origin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let init = crate::win_console::command("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(init.status.success());
        let add = crate::win_console::command("git")
            .args(["remote", "add", "origin", "https://example.com/repo.git"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(add.status.success());
        assert!(
            git_has_remote(dir.to_string_lossy().into_owned()),
            "configured origin must count as a push remote"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod prompt_cwd_tests {
    use super::*;

    /// Regression for bug #50: a session's per-turn cwd notice must always
    /// name *that session's own* `meta.cwd`, never another session's (or the
    /// engine-wide startup default's). Two sessions created against different
    /// repos must each get a notice mentioning only their own path.
    #[test]
    fn session_cwd_notice_reflects_only_its_own_session() {
        let test_flex = std::path::Path::new("/Users/example/Documents/Projects/TestFlex");
        let test_next = std::path::Path::new("/Users/example/Documents/Apps/TestNext");

        let flex_notice = session_cwd_notice(test_flex);
        let next_notice = session_cwd_notice(test_next);

        assert!(flex_notice.contains("/Users/example/Documents/Projects/TestFlex"));
        assert!(!flex_notice.contains("/Users/example/Documents/Apps/TestNext"));

        assert!(next_notice.contains("/Users/example/Documents/Apps/TestNext"));
        assert!(!next_notice.contains("/Users/example/Documents/Projects/TestFlex"));

        assert_ne!(flex_notice, next_notice);
    }

    /// The notice must explicitly call out and override the engine-wide
    /// startup line from `00-identity.md` (`Working directory at engine
    /// startup: {{cwd}}`) — otherwise the model has two same-weight, possibly
    /// conflicting cwd claims in one prompt and may pick the wrong one, which
    /// is exactly what happened in bug #50's live repro.
    #[test]
    fn session_cwd_notice_overrides_the_engine_startup_line() {
        let notice = session_cwd_notice(std::path::Path::new("/repo"));
        assert!(notice.contains("Working directory at engine startup"));
        assert!(notice.to_lowercase().contains("overrides"));
    }
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
            let out = crate::win_console::command("git")
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
        crate::win_console::command("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
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

        assert_eq!(
            baseline.files.get("gone.txt").map(String::as_str),
            Some("deleted")
        );

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

    /// Persistence round-trip: saving a baseline map to disk and loading it
    /// back must yield an equivalent map. This is the core of the app-restart
    /// fix — `AppState::new` calls `load_session_baselines()` on startup, so
    /// whatever `save_session_baselines` last wrote must come back intact.
    #[test]
    fn baseline_persistence_round_trips() {
        let mut files = std::collections::HashMap::new();
        files.insert("a.txt".to_string(), "abc123".to_string());
        files.insert("gone.txt".to_string(), "deleted".to_string());
        files.insert("public/".to_string(), "dir".to_string());
        let mut baselines = std::collections::HashMap::new();
        baselines.insert(
            "session-1".to_string(),
            crate::state::SessionBaseline {
                head_sha: "deadbeef".to_string(),
                files,
            },
        );

        // Round-trip through the same JSON (de)serialization
        // save/load_session_baselines use, without touching the real
        // per-user data dir (keeps this test hermetic).
        let raw = serde_json::to_string_pretty(&baselines).unwrap();
        let loaded: std::collections::HashMap<String, crate::state::SessionBaseline> =
            serde_json::from_str(&raw).unwrap();

        assert_eq!(loaded.len(), 1);
        let loaded_baseline = &loaded["session-1"];
        assert_eq!(loaded_baseline.head_sha, "deadbeef");
        assert_eq!(
            loaded_baseline.files.get("a.txt").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(
            loaded_baseline.files.get("gone.txt").map(String::as_str),
            Some("deleted")
        );
        assert_eq!(
            loaded_baseline.files.get("public/").map(String::as_str),
            Some("dir")
        );
    }
}

/// Commit-center git-mutation commands (`git_commit_paths`,
/// `git_commit_and_push`, `git_create_branch_and_commit`, `git_create_pr`)
/// plus `write_temp_blob`. These are `#[tauri::command]`s that take
/// `State<'_, AppState>`, so the harness below builds a real (mocked-runtime)
/// `tauri::App`, `.manage()`s an `AppState` wired to an in-memory
/// `EngineService`, and reads the `State` back off it — the standard way to
/// unit test a Tauri command per `tauri::test::mock_app`.
#[cfg(test)]
mod commit_center_tests {
    use std::path::Path;

    use agentloop_core::{Agent, AgentError, EventStream};
    use agentloop_session::MemoryStore;
    use async_trait::async_trait;
    use tauri::Manager;

    use super::*;

    /// Minimal git repo fixture under a fresh temp dir, scoped to this
    /// module's own tests so it has no cross-module test dependency.
    fn init_repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "flex-commit-center-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let out = crate::win_console::command("git")
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

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(dir: &Path, msg: &str) {
        crate::win_console::command("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["commit", "-q", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// `git status --porcelain` lines for a repo dir, for asserting which
    /// paths are (not) still dirty after a commit.
    fn status_lines(dir: &Path) -> Vec<String> {
        let out = crate::win_console::command("git")
            .args(["status", "--porcelain"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect()
    }

    fn current_branch(dir: &Path) -> String {
        let out = crate::win_console::command("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Test-only `Agent` stub. None of the commit-center commands under test
    /// call anything on `Agent` (they only read `SessionMeta` via
    /// `EngineService::session_meta`, which goes straight to the
    /// `SessionStore`), so every method just panics if ever invoked — that
    /// would indicate the test started exercising a code path these tests
    /// don't intend to cover, not a real production concern.
    struct StubAgent;

    #[async_trait]
    impl Agent for StubAgent {
        fn info(&self) -> agentloop_contracts::AgentInfo {
            unimplemented!("StubAgent::info not exercised by commit-center tests")
        }

        fn capabilities(&self) -> agentloop_contracts::AgentCaps {
            unimplemented!("StubAgent::capabilities not exercised by commit-center tests")
        }

        async fn create_session(&self, _params: NewSessionParams) -> Result<SessionId, AgentError> {
            unimplemented!("StubAgent::create_session not exercised by commit-center tests")
        }

        async fn resume_session(&self, _id: &SessionId) -> Result<(), AgentError> {
            unimplemented!("StubAgent::resume_session not exercised by commit-center tests")
        }

        async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError> {
            unimplemented!("StubAgent::list_sessions not exercised by commit-center tests")
        }

        fn events(&self, _session: &SessionId) -> Result<EventStream, AgentError> {
            unimplemented!("StubAgent::events not exercised by commit-center tests")
        }

        async fn prompt(
            &self,
            _session: &SessionId,
            _input: PromptInput,
            _opts: TurnOptions,
        ) -> Result<TurnSummary, AgentError> {
            unimplemented!("StubAgent::prompt not exercised by commit-center tests")
        }

        async fn cancel(&self, _session: &SessionId) -> Result<(), AgentError> {
            unimplemented!("StubAgent::cancel not exercised by commit-center tests")
        }

        async fn respond_permission(
            &self,
            _session: &SessionId,
            _id: PermissionRequestId,
            _decision: PermissionDecision,
        ) -> Result<(), AgentError> {
            unimplemented!("StubAgent::respond_permission not exercised by commit-center tests")
        }
    }

    /// Builds an `AppState` whose `EngineService` has exactly one session
    /// (id `"s1"`), pointed at `cwd`, non-isolated (`base_cwd: None`) — the
    /// shape every commit-center command expects for a plain (non-isolated)
    /// repo session.
    fn state_with_session(cwd: &Path) -> AppState {
        let session_store: Arc<dyn agentloop_core::SessionStore> = Arc::new(MemoryStore::new());
        let now = agentloop_contracts::now_ms();
        let meta = SessionMeta {
            id: SessionId::from("s1".to_string()),
            title: None,
            agent_id: "native".to_string(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: cwd.to_path_buf(),
            model: None,
            fallback_models: Vec::new(),
            mode: None,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        // Seed synchronously. `MemoryStore::create` never actually awaits
        // (it just locks a std Mutex), so a bare `futures::executor::block_on`
        // resolves it immediately without needing (or conflicting with) a
        // tokio runtime — this helper is called from inside `#[tokio::test]`
        // bodies, where spinning up a second tokio runtime would panic.
        futures::executor::block_on(session_store.create(meta)).expect("seed session meta");

        let engine = EngineService::new(Arc::new(StubAgent), session_store);

        let jsonl_dir = std::env::temp_dir().join(format!(
            "flex-commit-center-jsonl-{}-{}",
            std::process::id(),
            agentloop_contracts::now_ms()
        ));
        let jsonl_store =
            Arc::new(agentloop_session::JsonlStore::open(&jsonl_dir).expect("open jsonl store"));

        AppState::new(jsonl_store, ProviderConfig::default(), Some(engine))
    }

    /// Wraps `state_with_session` in a mocked-runtime Tauri app and hands
    /// back a real `State<AppState>`, the only way `#[tauri::command]`
    /// functions taking `State` can be called directly from a unit test.
    fn mock_state_with_session(cwd: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let app_state = state_with_session(cwd);
        tauri::test::mock_builder()
            .manage(app_state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock app")
    }

    #[tokio::test]
    async fn git_commit_paths_stages_only_selected_paths() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        write(&dir, "b.txt", "v1\n");
        commit_all(&dir, "initial commit");

        // Two files get dirtied; only one is selected for commit.
        write(&dir, "a.txt", "v2 (to be committed)\n");
        write(&dir, "b.txt", "v2 (left dirty)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_commit_paths(
            state,
            "s1".to_string(),
            "commit a only".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("commit should succeed");
        assert!(!sha.is_empty());

        let status = status_lines(&dir);
        assert!(
            status.iter().all(|l| !l.contains("a.txt")),
            "a.txt must no longer be dirty after being committed: {status:?}"
        );
        assert!(
            status.iter().any(|l| l.contains("b.txt")),
            "b.txt must still be dirty (not selected for commit): {status:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_paths_rejects_empty_message() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_commit_paths(
            state,
            "s1".to_string(),
            "   ".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("empty/whitespace-only message must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_paths_rejects_empty_paths() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_commit_paths(state, "s1".to_string(), "msg".to_string(), vec![])
            .await
            .expect_err("empty path list must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_and_push_commits_locally_when_no_remote() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        // No remote configured: the commit step must still land even though
        // the push step is guaranteed to fail.
        let err = git_commit_and_push(
            state,
            "s1".to_string(),
            "commit then push".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("push must fail with no remote configured");
        assert!(matches!(err, DesktopError::Message(_)));

        let log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let log_text = String::from_utf8_lossy(&log.stdout);
        assert!(
            log_text.contains("commit then push"),
            "commit must have landed locally even though push failed: {log_text}"
        );
        assert!(
            status_lines(&dir).iter().all(|l| !l.contains("a.txt")),
            "a.txt must be committed (clean) despite the push failure"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_and_push_pushes_new_branch_to_bare_remote_with_no_upstream() {
        // A local bare repo stands in for `origin` so the real push +
        // no-upstream `-u` retry path in `push_current_branch` gets
        // exercised end to end, not just the local-commit half.
        let remote_dir = init_repo();
        // `git init` alone can't produce a bare repo via our helper, so
        // reinitialize as bare directly.
        std::fs::remove_dir_all(&remote_dir).ok();
        std::fs::create_dir_all(&remote_dir).unwrap();
        let out = crate::win_console::command("git")
            .args(["init", "--bare", "-q"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        assert!(out.status.success());

        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        crate::win_console::command("git")
            .args(["remote", "add", "origin"])
            .arg(&remote_dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        // Push the initial commit once first so the bare remote has the
        // branch's history; still no upstream tracking ref is set, so the
        // *next* push exercises the "no upstream" -u retry path.
        crate::win_console::command("git")
            .args(["push", "origin", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();

        write(&dir, "a.txt", "v2 (to push)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_commit_and_push(
            state,
            "s1".to_string(),
            "push to bare remote".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("commit+push against a real bare remote should succeed");
        assert!(!sha.is_empty());

        let branch = current_branch(&dir);
        let remote_log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1", &branch])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        let remote_log_text = String::from_utf8_lossy(&remote_log.stdout);
        assert!(
            remote_log_text.contains("push to bare remote"),
            "bare remote must have received the pushed commit: {remote_log_text}"
        );

        // Upstream tracking must now be set (the `-u` retry path ran).
        let upstream = crate::win_console::command("git")
            .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            upstream.status.success(),
            "an upstream tracking branch must be configured after the -u retry: {}",
            String::from_utf8_lossy(&upstream.stderr)
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&remote_dir).ok();
    }

    #[tokio::test]
    async fn git_create_branch_and_commit_creates_and_checks_out_branch() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_create_branch_and_commit(
            state,
            "s1".to_string(),
            "feature/my-branch".to_string(),
            "commit on new branch".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("branch create + commit should succeed");
        assert!(!sha.is_empty());

        assert_eq!(current_branch(&dir), "feature/my-branch");

        let branches = crate::win_console::command("git")
            .args(["branch", "--list", "feature/my-branch"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&branches.stdout).contains("feature/my-branch"),
            "new branch must exist in the branch list"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_create_branch_and_commit_rejects_invalid_branch_name() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");
        let branch_before = current_branch(&dir);

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_create_branch_and_commit(
            state,
            "s1".to_string(),
            "".to_string(),
            "msg".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("empty branch name must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        // Must not have switched branches on the rejected attempt.
        assert_eq!(current_branch(&dir), branch_before);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_create_pr_degrades_when_gh_is_unavailable() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2 (to push)\n");

        // A bare repo stands in for `origin` so the push half of
        // `git_create_pr` has somewhere to succeed against — `gh` itself is
        // hidden via PATH below, which is what should trigger the degraded
        // (non-error) outcome.
        let remote_dir = init_repo();
        std::fs::remove_dir_all(&remote_dir).ok();
        std::fs::create_dir_all(&remote_dir).unwrap();
        crate::win_console::command("git")
            .args(["init", "--bare", "-q"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["remote", "add", "origin"])
            .arg(&remote_dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["push", "-u", "origin", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();
        write(&dir, "a.txt", "v3 (to push again)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        // Simulate "gh absent" by restricting PATH for this process to just
        // wherever `git` itself resolves from (so `git add`/`commit`/`push`
        // still work), excluding every other PATH entry — in particular
        // wherever `gh` lives (e.g. Homebrew's bin). `git_create_pr` shells
        // out to `gh auth status`/`gh pr create` via `std::process::Command`,
        // which resolves through PATH, so this reliably makes gh
        // "not available" without depending on whether the host actually has
        // gh installed.
        //
        // This mutates the process-wide `PATH` env var, which is safe here
        // only because no other test in this suite invokes `gh`, and `git`
        // remains resolvable throughout the narrowed window (serialized by
        // `PATH_MUTATION_GUARD` against any future PATH-mutating test). A
        // `tokio::sync::Mutex` (not `std::sync::Mutex`) is required here
        // since the guard is held across the `git_create_pr(..).await` below.
        static PATH_MUTATION_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _path_guard = PATH_MUTATION_GUARD.lock().await;

        let git_path = crate::win_console::command("which")
            .arg("git")
            .output()
            .ok()
            .and_then(|out| {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                path.rsplit_once('/').map(|(dir, _)| dir.to_string())
            })
            .expect("resolve git's directory via `which git`");
        let original_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", &git_path);
        }

        let result = git_create_pr(
            state,
            "s1".to_string(),
            "commit for pr".to_string(),
            vec!["a.txt".to_string()],
            None,
            None,
        )
        .await;

        unsafe {
            std::env::set_var("PATH", original_path);
        }

        let outcome = result.expect("degraded gh path must still return Ok, not Err");
        assert!(!outcome.commit_sha.is_empty());
        assert!(outcome.pr_url.is_none());
        assert!(
            outcome.degraded_reason.is_some(),
            "missing gh must surface a degraded_reason rather than silently succeeding"
        );

        // The commit + push must have gone through even though the PR step
        // was skipped.
        let remote_log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&remote_log.stdout).contains("commit for pr"),
            "push must have landed on the remote despite gh being unavailable"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&remote_dir).ok();
    }

    #[test]
    fn write_temp_blob_accepts_allowlisted_extensions() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        for ext in ["png", "jpg", "jpeg", "gif", "webp"] {
            let path = rt
                .block_on(write_temp_blob(vec![1, 2, 3, 4], ext.to_string()))
                .unwrap_or_else(|e| panic!("ext `{ext}` should be accepted: {e}"));
            assert!(
                Path::new(&path).exists(),
                "returned path must exist on disk: {path}"
            );
            let contents = std::fs::read(&path).unwrap();
            assert_eq!(contents, vec![1, 2, 3, 4]);
            std::fs::remove_file(&path).ok();
        }
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_disallowed_extension() {
        let err = write_temp_blob(vec![1, 2, 3], "exe".to_string())
            .await
            .expect_err("non-image extension must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_empty_bytes() {
        let err = write_temp_blob(Vec::new(), "png".to_string())
            .await
            .expect_err("empty blob must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_oversized_blob() {
        // Just over the 20MB cap — one byte over is enough to prove the
        // boundary check, no need to allocate something wastefully larger.
        let bytes = vec![0u8; 20 * 1024 * 1024 + 1];
        let err = write_temp_blob(bytes, "png".to_string())
            .await
            .expect_err("oversized blob must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }
}
