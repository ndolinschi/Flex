//! Desktop shell errors — mapped to strings for Tauri invoke.

use agentloop_sdk::EngineServiceError;

#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    #[error(transparent)]
    Engine(#[from] EngineServiceError),
    #[error("engine is not configured — save a provider first")]
    NotConfigured,
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("session store error: {0}")]
    Store(String),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("{0}")]
    Message(String),
}

impl serde::Serialize for DesktopError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type DesktopResult<T> = Result<T, DesktopError>;
