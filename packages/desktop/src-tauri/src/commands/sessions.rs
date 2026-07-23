
use super::common::{parse_isolation, require_service};
use super::git::capture_session_baseline;
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub isolation: Option<String>,
    pub reuse_workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionBaselineReady {
    session_id: String,
}

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
                return;
            }
        }

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
        use crate::event_coalesce::{EventCoalescer, FLUSH_INTERVAL_MS};
        use std::time::Duration;

        let session_key = key;
        let mut stream = stream;
        let mut coalescer = EventCoalescer::new();
        // Deadline for the current pending batch (set when the first delta lands).
        let mut flush_deadline: Option<tokio::time::Instant> = None;

        loop {
            let next_event = if let Some(deadline) = flush_deadline {
                tokio::select! {
                    item = stream.next() => item,
                    _ = tokio::time::sleep_until(deadline) => {
                        if let Some(pending) = coalescer.flush() {
                            if let Err(err) = app.emit("session-event", &pending) {
                                tracing::warn!(
                                    session_id = %session_key.as_str(),
                                    error = %err,
                                    "session-event emit failed; ending subscription relay"
                                );
                                break;
                            }
                        }
                        flush_deadline = None;
                        continue;
                    }
                }
            } else {
                stream.next().await
            };

            let Some(event) = next_event else {
                if let Some(pending) = coalescer.flush() {
                    if let Err(err) = app.emit("session-event", &pending) {
                        tracing::warn!(
                            session_id = %session_key.as_str(),
                            error = %err,
                            "session-event emit failed on stream end flush"
                        );
                    }
                }
                break;
            };

            let ready = coalescer.push(event);
            let emitted_any = !ready.is_empty();
            let mut emit_failed = false;
            for ready_event in ready {
                if let Err(err) = app.emit("session-event", &ready_event) {
                    tracing::warn!(
                        session_id = %session_key.as_str(),
                        error = %err,
                        "session-event emit failed; ending subscription relay"
                    );
                    emit_failed = true;
                    break;
                }
            }
            if emit_failed {
                break;
            }

            if coalescer.has_pending() {
                // Arm (or re-arm after a forced flush started a new batch) the
                // 16ms window; keep the existing deadline while deltas merge.
                if flush_deadline.is_none() || emitted_any {
                    flush_deadline = Some(
                        tokio::time::Instant::now()
                            + Duration::from_millis(FLUSH_INTERVAL_MS),
                    );
                }
            } else {
                flush_deadline = None;
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
