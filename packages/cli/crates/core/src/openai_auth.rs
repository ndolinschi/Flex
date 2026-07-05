//! OpenAI ChatGPT OAuth orchestration for the `/connect` wizard.

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use agentloop_core::ProviderError;
use agentloop_provider_openai::{
    OpenAiOAuthMethod, OpenAiOAuthTokens, start_oauth, store_oauth_tokens,
};

/// Progress of one OpenAI OAuth attempt.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum OpenAiOAuthEvent {
    /// Authorization URL and instructions to show the user.
    Started {
        url: String,
        instructions: String,
        method: OpenAiOAuthMethod,
    },
    /// Waiting for the user to finish in the browser or on the device page.
    Waiting,
    /// Tokens stored and ready to use.
    Succeeded,
}

/// Why OpenAI OAuth failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenAiAuthError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("OpenAI sign-in cancelled")]
    Cancelled,
}

/// Run browser or headless ChatGPT OAuth. Progress is sent on `events`.
pub async fn login_openai(
    method: OpenAiOAuthMethod,
    events: mpsc::Sender<OpenAiOAuthEvent>,
    cancel: CancellationToken,
) -> Result<OpenAiOAuthTokens, OpenAiAuthError> {
    let started = start_oauth(method).await?;
    let _ = events
        .send(OpenAiOAuthEvent::Started {
            url: started.url.clone(),
            instructions: started.instructions.clone(),
            method: started.method,
        })
        .await;
    let _ = events.send(OpenAiOAuthEvent::Waiting).await;
    let tokens = match started.complete(cancel.clone()).await {
        Ok(tokens) => tokens,
        Err(ProviderError::Cancelled { .. }) => return Err(OpenAiAuthError::Cancelled),
        Err(err) => return Err(err.into()),
    };
    let _path = store_oauth_tokens(&tokens)?;
    let _ = events.send(OpenAiOAuthEvent::Succeeded).await;
    Ok(tokens)
}
