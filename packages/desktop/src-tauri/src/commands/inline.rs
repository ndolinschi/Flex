//! Composer / Prompt-tab inline (ghost-text) completion.

use super::common::require_service;
use super::prelude::*;
use super::routines::respawn_cron_loop;

pub const INLINE_COMPLETION_NOT_CONFIGURED: &str = "inline_completion_not_configured";

/// Read desktop inline-completion prefs (ghost-text model + enable flag).
#[tauri::command]
pub async fn get_inline_completion_prefs(
    state: State<'_, AppState>,
) -> DesktopResult<InlineCompletionPrefs> {
    let cfg = state.config.lock().await;
    Ok(cfg.prefs.inline_completion.clone())
}

/// Persist inline-completion prefs and rebuild the engine when the chosen
/// provider differs from the active chat profile (so it lands in the registry).
#[tauri::command]
pub async fn save_inline_completion_prefs(
    app: AppHandle,
    state: State<'_, AppState>,
    mut prefs: InlineCompletionPrefs,
) -> DesktopResult<InlineCompletionPrefs> {
    if let (Some(provider_id), Some(model_id)) = (&prefs.provider_id, &mut prefs.model_id) {
        *model_id = normalize_inline_model_id(provider_id, model_id);
    }
    let mut cfg = state.config.lock().await.clone();
    cfg.prefs.inline_completion = prefs.clone();
    persist_config(&cfg)?;
    *state.config.lock().await = cfg.clone();

    if prefs.is_configured() && cfg.is_ready() {
        match build_service(&cfg, state.store.clone(), app.clone()) {
            Ok(service) => {
                *state.service.lock().await = Some(service);
                respawn_cron_loop(&state).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "inline completion prefs saved; engine rebuild failed");
            }
        }
    }
    Ok(prefs)
}

/// One-shot ghost-text continuation for the composer / Prompt tab.
/// Tool-free `stream_chat` (same pattern as `suggest_session_title`).
/// Returns only the continuation text; empty string when the model yields nothing.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn complete_prompt_inline(
    state: State<'_, AppState>,
    prefix: String,
    suffix: Option<String>,
) -> DesktopResult<String> {
    let prefs = {
        let cfg = state.config.lock().await;
        cfg.prefs.inline_completion.clone()
    };
    if !prefs.enabled || !prefs.is_configured() {
        return Err(DesktopError::Message(
            INLINE_COMPLETION_NOT_CONFIGURED.into(),
        ));
    }
    let model_ref = prefs
        .model_ref()
        .ok_or_else(|| DesktopError::Message(INLINE_COMPLETION_NOT_CONFIGURED.into()))?;

    let service = require_service(&state).await?;
    stream_inline_completion(&service, &model_ref, &prefix, suffix.as_deref()).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInlineCompletionInput {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInlineCompletionResult {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

/// Probe inline-completion connectivity without persisting prefs. Rebuilds a
/// throwaway engine snapshot so Ollama (or another non-active provider) can be
/// registered via `all_providers` before the user saves.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn check_inline_completion_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CheckInlineCompletionInput,
) -> DesktopResult<CheckInlineCompletionResult> {
    let provider_id = input.provider_id.trim().to_string();
    let model_id = normalize_inline_model_id(&provider_id, input.model_id.trim());
    if provider_id.is_empty() || model_id.is_empty() {
        return Ok(CheckInlineCompletionResult {
            ok: false,
            message: "Pick a provider and model.".into(),
            sample: None,
        });
    }

    let mut cfg = state.config.lock().await.clone();
    if !cfg.is_ready() {
        return Ok(CheckInlineCompletionResult {
            ok: false,
            message: "Configure a provider under Settings → Models first.".into(),
            sample: None,
        });
    }

    cfg.prefs.inline_completion = InlineCompletionPrefs {
        enabled: true,
        provider_id: Some(provider_id.clone()),
        model_id: Some(model_id.clone()),
        setup_dismissed: false,
    };

    let service = match build_service(&cfg, state.store.clone(), app.clone()) {
        Ok(service) => service,
        Err(err) => {
            return Ok(CheckInlineCompletionResult {
                ok: false,
                message: err.to_string(),
                sample: None,
            });
        }
    };

    let model_ref = format!("{provider_id}/{model_id}");
    let prefix = "Please help me write a prompt to";
    match stream_inline_completion(&service, &model_ref, prefix, None).await {
        Ok(sample) if sample.trim().is_empty() => {
            // Still install so Refresh models can list Ollama tags next.
            *state.service.lock().await = Some(service);
            respawn_cron_loop(&state).await;
            Ok(CheckInlineCompletionResult {
                ok: false,
                message:
                    "Connected, but the model returned an empty completion — try another model."
                        .into(),
                sample: None,
            })
        }
        Ok(sample) => {
            *state.service.lock().await = Some(service);
            respawn_cron_loop(&state).await;
            Ok(CheckInlineCompletionResult {
                ok: true,
                message: "Connection OK — inline completions should work after you save.".into(),
                sample: Some(sample),
            })
        }
        Err(err) => {
            // Provider registered but call failed (daemon down, missing model, …).
            // Keep the rebuilt registry so Refresh models can surface tags.
            *state.service.lock().await = Some(service);
            respawn_cron_loop(&state).await;
            Ok(CheckInlineCompletionResult {
                ok: false,
                message: err.to_string(),
                sample: None,
            })
        }
    }
}

pub(crate) async fn stream_inline_completion(
    service: &EngineService,
    model_ref: &str,
    prefix: &str,
    suffix: Option<&str>,
) -> DesktopResult<String> {
    let registry = service.provider_registry();
    let model = ModelRef(model_ref.to_owned());
    let (provider, model_id) = registry.resolve(&model).ok_or_else(|| {
        DesktopError::Message(format!(
            "no provider for inline completion model {model_ref} — pick a connected model in Settings → Tools"
        ))
    })?;

    let prefix: String = prefix.chars().take(4000).collect();
    if prefix.trim().is_empty() {
        return Ok(String::new());
    }
    let suffix = suffix.unwrap_or_default();
    let suffix: String = suffix.chars().take(500).collect();

    let system = "You are an inline autocomplete for an AI agent prompt editor. \
        Continue the user's draft prompt. Reply with ONLY the continuation text \
        that should be inserted at the cursor — no quotes, no markdown fences, \
        no explanation. Prefer completing the current line or the next short phrase. \
        Keep the continuation under ~40 tokens."
        .to_string();

    let user = if suffix.trim().is_empty() {
        format!("Complete this prompt draft:\n\n{prefix}")
    } else {
        format!(
            "Complete the prompt draft at the cursor (marked «CURSOR»). \
             Return only text to insert at «CURSOR».\n\n{prefix}«CURSOR»{suffix}"
        )
    };

    let mut request = ChatRequest::new(model_id, vec![Message::user(user)]);
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

    Ok(sanitize_inline_completion(&text))
}

/// Strip fences/quotes models sometimes wrap around a bare continuation.
pub(crate) fn sanitize_inline_completion(raw: &str) -> String {
    let mut t = raw.trim().to_string();
    if t.starts_with("```") {
        // Drop opening fence line (` ``` ` or ` ```lang `), then closing fence.
        let after_fence = t.find('\n').map(|i| &t[i + 1..]).unwrap_or("").to_string();
        t = after_fence
            .rsplit_once("```")
            .map(|(before, _)| before.trim().to_string())
            .unwrap_or_else(|| after_fence.trim().to_string());
    }
    t = t.trim_matches(['"', '\'', '`']).trim().to_string();
    if t.starts_with('\n') && !t.starts_with("\n\n") {
        t = t.trim_start_matches('\n').to_string();
    }
    t
}

#[cfg(test)]
mod inline_completion_tests {
    use super::sanitize_inline_completion;

    #[test]
    fn sanitize_strips_quotes_and_fences() {
        assert_eq!(sanitize_inline_completion("\"hello world\""), "hello world");
        assert_eq!(
            sanitize_inline_completion("```\nadd tests\n```"),
            "add tests"
        );
    }
}
