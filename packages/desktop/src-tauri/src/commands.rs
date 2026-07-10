//! Tauri commands — thin wrappers over `EngineService` + keychain config.

use std::path::PathBuf;

use std::sync::Arc;

use agentloop_channel::{RoutineSpec, RoutineStore, RoutineTrigger};
use agentloop_contracts::{
    Answer, BlobSource, CommandInfo, ContentBlock, GoalSpec, IntegrationOutcome, IsolationPolicy,
    ModelRef, NewSessionParams, PermissionDecision, PermissionMode, PermissionRequestId,
    PromptInput, QuestionId, SessionEvent, SessionId, SessionMeta, SessionMetaPatch, TurnOptions,
    TurnSummary,
};
use agentloop_core::WorkspaceStatus;
use agentloop_sdk::EngineService;
use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner, default_routines_dir};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::compose::build_service;
use crate::config::{
    ProviderConfig, ProviderConfigView, SaveProviderConfigInput, persist_config,
};
use crate::error::{DesktopError, DesktopResult};
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
    Ok(service.session_meta(&id).await?)
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
    Ok(service.resume_session(&id).await?)
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

#[tauri::command]
pub async fn prompt(
    state: State<'_, AppState>,
    input: PromptCommandInput,
) -> DesktopResult<TurnSummary> {
    let service = require_service(&state).await?;
    let id = SessionId::from(input.session_id.clone());
    let opts = TurnOptions {
        model: input.model.clone().map(ModelRef),
        permission_mode: parse_permission_mode(input.permission_mode.as_deref()),
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

/// Read-only current-branch lookup for the composer context bar.
#[tauri::command]
pub fn git_branch(cwd: String) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&cwd)
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
        .current_dir(&cwd)
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
        .current_dir(&cwd)
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
    let porcelain = match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&cwd)
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
        .current_dir(&cwd)
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

/// Unified diff for one file (read-only, capped) for the Changes panel.
#[tauri::command]
pub fn git_diff(cwd: String, path: String) -> DesktopResult<String> {
    const MAX_DIFF_BYTES: usize = 200 * 1024;
    let path = path.trim();
    if path.is_empty() || path.starts_with('-') {
        return Err(DesktopError::Message("invalid path".into()));
    }

    let tracked = std::process::Command::new("git")
        .args(["diff", "HEAD", "--", path])
        .current_dir(&cwd)
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
            .current_dir(&cwd)
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

    if text.len() > MAX_DIFF_BYTES {
        let mut cut = MAX_DIFF_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… diff truncated …\n");
    }
    Ok(text)
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
