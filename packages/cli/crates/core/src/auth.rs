//! GitHub Copilot sign-in orchestration.
//!
//! Drives the provider crate's device flow end to end: request a user code,
//! poll until the user approves on github.com, persist the OAuth token where
//! the provider discovers it, then verify Copilot access by exercising the
//! real token exchange once. UI-agnostic — progress surfaces as typed
//! [`LoginEvent`]s on a channel; the caller renders them (TUI modal or
//! headless prints).

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use agentloop_core::{Provider, ProviderError};
use agentloop_provider_copilot::{CopilotConfig, CopilotProvider, DeviceFlow, store_github_token};

/// Whether GitHub Copilot credentials are discoverable (env vars or editor
/// sign-in on disk).
pub fn has_copilot_credentials() -> bool {
    CopilotConfig::discoverable()
}

/// Progress of one login attempt, in emission order.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LoginEvent {
    /// Show `user_code` and send the user to `verification_uri`.
    CodeReady {
        /// The short code the user types on github.com.
        user_code: String,
        /// Where the user enters the code.
        verification_uri: String,
        /// Seconds until the code expires.
        expires_in: u64,
    },
    /// Waiting for the user to approve on github.com.
    Polling,
    /// Token stored; confirming Copilot access via the token exchange.
    Verifying,
    /// Signed in; the native service should be rebuilt to pick up copilot.
    Succeeded,
}

/// Why a login attempt failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuthError {
    /// The device flow or verification failed.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// The user cancelled (Esc / Ctrl-C) while polling.
    #[error("login cancelled")]
    Cancelled,
}

/// Run the full Copilot device-flow login.
///
/// Progress is reported on `events` (best effort — a dropped receiver never
/// fails the login). Trip `cancel` to abort while polling.
pub async fn login_copilot(
    events: mpsc::Sender<LoginEvent>,
    cancel: CancellationToken,
) -> Result<(), AuthError> {
    let flow = DeviceFlow::new();
    let authorization = flow.start().await?;
    let _ = events
        .send(LoginEvent::CodeReady {
            user_code: authorization.user_code.clone(),
            verification_uri: authorization.verification_uri.clone(),
            expires_in: authorization.expires_in,
        })
        .await;

    let _ = events.send(LoginEvent::Polling).await;
    let token = match flow.poll(&authorization, cancel.clone()).await {
        Ok(token) => token,
        Err(ProviderError::Cancelled { .. }) => return Err(AuthError::Cancelled),
        Err(err) => return Err(err.into()),
    };

    let path = store_github_token(&token)?;
    tracing::info!(target: "auth", "stored GitHub token at {}", path.display());

    // Confirm the account actually has Copilot access: constructing from the
    // stored token and listing models exercises the real token exchange.
    let _ = events.send(LoginEvent::Verifying).await;
    let provider = CopilotProvider::from_env()?;
    provider.list_models().await?;

    let _ = events.send(LoginEvent::Succeeded).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn has_copilot_credentials_false_without_token() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_home = dir.path().join("config");
        temp_env::with_vars(
            [
                ("COPILOT_GITHUB_TOKEN", None::<&str>),
                ("GH_COPILOT_TOKEN", None::<&str>),
                ("XDG_CONFIG_HOME", Some(config_home.to_str().expect("utf8"))),
            ],
            || {
                assert!(!has_copilot_credentials());
            },
        );
    }

    #[test]
    fn has_copilot_credentials_true_from_env() {
        temp_env::with_var("COPILOT_GITHUB_TOKEN", Some("gho_test"), || {
            assert!(has_copilot_credentials());
        });
    }

    #[test]
    fn has_copilot_credentials_true_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let copilot_dir = dir.path().join("github-copilot");
        std::fs::create_dir_all(&copilot_dir).expect("mkdir");
        let apps = copilot_dir.join("apps.json");
        std::fs::write(
            &apps,
            r#"{"github.com:Iv1.test":{"oauth_token":"gho_disk"}}"#,
        )
        .expect("write apps.json");
        let config_home = dir.path().to_str().expect("utf8");
        temp_env::with_vars(
            [
                ("COPILOT_GITHUB_TOKEN", None::<&str>),
                ("GH_COPILOT_TOKEN", None::<&str>),
                ("XDG_CONFIG_HOME", Some(config_home)),
            ],
            || {
                assert!(has_copilot_credentials());
            },
        );
        let _ = PathBuf::from(config_home);
    }
}
