//! Session CRUD, subscribe, and title suggestion.

use super::common::{parse_isolation, require_service};
use super::git::capture_session_baseline;
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    /// `never` | `optional` | `required` — falls back to prefs.default_isolation.
    pub isolation: Option<String>,
    /// Attach an existing worktree (see `list_workspaces`) on the first
    /// prompt instead of provisioning a new one. Ignored when the resolved
    /// isolation policy doesn't want a workspace.
    pub reuse_workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionBaselineReady {
    session_id: String,
}

/// Capture + persist a session baseline off the create/resume hot path.
/// Changes falls back to full-repo status until this finishes; the UI
/// refetches git-status when `session-baseline-ready` fires.
pub(crate) fn schedule_session_baseline(app: tauri::AppHandle, session_id: String, cwd: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };

        {
            let baselines = state.session_baselines.lock().await;
            if baselines.contains_key(&session_id) {
                return;
            }
        }
        {
            let mut inflight = state.baseline_inflight.lock().await;
            if !inflight.insert(session_id.clone()) {
                // Another create/subscribe path already scheduled this id.
                return;
            }
        }

        // Missing / deleted project folders are common (stale recents). Don't
        // shell out to git — one clear warn, then stop.
        if !cwd.is_dir() {
            tracing::warn!(
                session_id = %session_id,
                cwd = %cwd.display(),
                "skipping session baseline: cwd missing or not a directory"
            );
            let mut inflight = state.baseline_inflight.lock().await;
            inflight.remove(&session_id);
            return;
        }

        let cwd_for_block = cwd.clone();
        let baseline =
            match tokio::task::spawn_blocking(move || capture_session_baseline(&cwd_for_block))
                .await
            {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        session_id = %session_id,
                        "baseline capture join failed"
                    );
                    None
                }
            };

        match baseline {
            Some(baseline) => {
                let inserted = {
                    let mut baselines = state.session_baselines.lock().await;
                    // Never overwrite an existing baseline (resume race / double schedule).
                    match baselines.entry(session_id.clone()) {
                        std::collections::hash_map::Entry::Vacant(slot) => {
                            slot.insert(baseline);
                            crate::state::save_session_baselines(&baselines);
                            true
                        }
                        std::collections::hash_map::Entry::Occupied(_) => false,
                    }
                };
                if inserted {
                    if let Err(err) = app.emit(
                        "session-baseline-ready",
                        &SessionBaselineReady {
                            session_id: session_id.clone(),
                        },
                    ) {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %err,
                            "session-baseline-ready emit failed"
                        );
                    }
                }
            }
            None => {
                tracing::warn!(
                    session_id = %session_id,
                    cwd = %cwd.display(),
                    "failed to capture session baseline; Changes panel will show full repo status"
                );
            }
        }

        let mut inflight = state.baseline_inflight.lock().await;
        inflight.remove(&session_id);
    });
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn create_session(
    app: tauri::AppHandle,
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
            reuse_workspace_id: input.reuse_workspace_id.filter(|s| !s.is_empty()),
            ..NewSessionParams::default()
        })
        .await?;
    let meta = service.session_meta(&id).await?;

    // Only non-isolated sessions need a baseline: isolated sessions get a
    // clean private worktree, so their `git_status` is already scoped to
    // this session's own changes. Non-fatal on any git failure — the
    // Changes panel just falls back to the full-repo view for this session.
    // Capture runs in the background so create_session returns immediately;
    // the baseline is still persisted once ready (survives restart before
    // resume — same guarantee as the previous synchronous path).
    if meta.base_cwd.is_none() {
        schedule_session_baseline(app, id.to_string(), meta.cwd.clone());
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

/// Stable error prefix the UI maps to the completion setup modal.

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn resume_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
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
    // Capture is deferred so resume returns without blocking on git.
    if result.is_ok() {
        let has_baseline = {
            let baselines = state.session_baselines.lock().await;
            baselines.contains_key(id.as_str())
        };
        if !has_baseline {
            if let Ok(meta) = service.session_meta(&id).await {
                if meta.base_cwd.is_none() {
                    schedule_session_baseline(app, id.to_string(), meta.cwd.clone());
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
    // UI paints the chat immediately and resumes in the background
    // (SessionSidebar / draft New Agent). Subscribe often races ahead of
    // resume and would otherwise ERROR with "no live handle". Auto-resume
    // here so the event relay attaches without forcing callers to serialize.
    let stream = match service.subscribe(&id) {
        Ok(stream) => stream,
        Err(err) => {
            let message = err.to_string();
            if !message.contains("no live handle") {
                return Err(DesktopError::from(err));
            }
            tracing::debug!(
                session_id = %session_id,
                "subscribe before resume; auto-resuming session"
            );
            if let Err(resume_err) = service.resume_session(&id).await {
                // Match resume_session's actionable "workspace missing" mapping.
                if let Ok(meta) = service.session_meta(&id).await {
                    if !meta.cwd.exists() {
                        return Err(DesktopError::Message(format!(
                            "workspace missing: {} ({resume_err})",
                            meta.cwd.display()
                        )));
                    }
                }
                return Err(DesktopError::from(resume_err));
            }
            // Backfill baseline for legacy sessions the same way resume_session
            // does when subscribe was the first attach path.
            let has_baseline = {
                let baselines = state.session_baselines.lock().await;
                baselines.contains_key(id.as_str())
            };
            if !has_baseline {
                if let Ok(meta) = service.session_meta(&id).await {
                    if meta.base_cwd.is_none() {
                        schedule_session_baseline(app.clone(), id.to_string(), meta.cwd.clone());
                    }
                }
            }
            service.subscribe(&id)?
        }
    };

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
