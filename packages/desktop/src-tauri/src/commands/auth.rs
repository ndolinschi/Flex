//! Copilot and ChatGPT device-flow authentication.

use super::prelude::*;
use super::providers::{chatgpt_oauth_discoverable, uuid_like_suffix};

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
    use agentloop_sdk::providers::copilot::{store_github_token, DeviceFlow};

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

// ---------------------------------------------------------------------------
// ChatGPT Plus/Pro subscription OAuth (Codex CLI headless device flow).
// Tokens land in `~/.config/agentloop/openai-auth.json` and unlock the
// native `chatgpt` provider (Codex Responses backend).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatgptAuthStatus {
    pub signed_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatgptAuthStart {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn chatgpt_auth_status() -> DesktopResult<ChatgptAuthStatus> {
    Ok(ChatgptAuthStatus {
        signed_in: chatgpt_oauth_discoverable(),
    })
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn chatgpt_auth_start(state: State<'_, AppState>) -> DesktopResult<ChatgptAuthStart> {
    use crate::state::PendingChatgptAuth;
    use agentloop_sdk::providers::openai::{start_oauth, OpenAiOAuthMethod};

    let started = start_oauth(OpenAiOAuthMethod::Headless)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let session_id = format!("chatgpt-auth-{}", uuid_like_suffix());
    let view = ChatgptAuthStart {
        session_id: session_id.clone(),
        user_code: started.user_code.clone(),
        verification_uri: started.verification_uri.clone(),
    };

    let mut pending = state.pending_chatgpt_auth.lock().await;
    for (_, prior) in pending.drain() {
        prior.cancel.cancel();
    }
    pending.insert(
        session_id,
        PendingChatgptAuth {
            start: started,
            cancel: CancellationToken::new(),
        },
    );
    Ok(view)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn chatgpt_auth_wait(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<ChatgptAuthStatus> {
    use agentloop_sdk::providers::openai::store_oauth_tokens;

    let session_id = session_id.trim().to_owned();
    let (started, cancel) = {
        let mut pending = state.pending_chatgpt_auth.lock().await;
        let entry = pending.remove(&session_id).ok_or_else(|| {
            DesktopError::Message("ChatGPT sign-in session not found — start a new sign-in".into())
        })?;
        (entry.start, entry.cancel)
    };

    match started.complete(cancel).await {
        Ok(tokens) => {
            store_oauth_tokens(&tokens).map_err(|e| DesktopError::Message(e.to_string()))?;
            Ok(ChatgptAuthStatus { signed_in: true })
        }
        Err(agentloop_core::ProviderError::Cancelled { .. }) => {
            Err(DesktopError::Message("sign-in cancelled".into()))
        }
        Err(err) => Err(DesktopError::Message(err.to_string())),
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn chatgpt_auth_cancel(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<()> {
    let session_id = session_id.trim();
    let mut pending = state.pending_chatgpt_auth.lock().await;
    if let Some(entry) = pending.remove(session_id) {
        entry.cancel.cancel();
    }
    Ok(())
}
