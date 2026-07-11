//! The engine service error type and its mapping to a wire `EngineError`.

use agentloop_contracts::{EngineError, ErrorCode, SessionId};
use agentloop_core::{AgentError, ExecError, ProviderError, StoreError, WorkspaceError};
use agentloop_loop::roles::RoleError;
use agentloop_mcp::McpBridgeError;
use agentloop_prompts::{CommandError, PromptError};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineServiceError {
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Prompt(#[from] PromptError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Skill(#[from] agentloop_prompts::SkillError),
    #[error(transparent)]
    Mcp(#[from] McpBridgeError),
    #[error(transparent)]
    Role(#[from] RoleError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error("session {0} is not isolated")]
    NotIsolated(SessionId),
    #[error("no workspace backend is configured")]
    NoWorkspaceBackend,
    #[error(
        "provider `{0}` is not available in this build; supported runtime providers: `openai`, `anthropic`, `gemini`, `ollama`, or a provider configured in the client's config"
    )]
    UnsupportedProvider(String),
    #[error("custom provider `{0}` conflicts with a built-in provider id")]
    CustomProviderConflict(String),
    #[error("custom provider `{id}` is invalid: {message}")]
    CustomProviderInvalid { id: String, message: String },
}

impl EngineServiceError {
    pub fn to_engine_error(&self) -> EngineError {
        match self {
            Self::Agent(err) => err.to_engine_error(),
            Self::Provider(err) => err.to_engine_error(),
            Self::Store(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::Prompt(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Command(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Skill(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Mcp(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Role(err) => EngineError::engine(ErrorCode::InvalidRequest, err.to_string()),
            Self::Workspace(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::Exec(err) => EngineError::engine(ErrorCode::Unknown, err.to_string()),
            Self::NotIsolated(_) | Self::NoWorkspaceBackend => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
            Self::UnsupportedProvider(_)
            | Self::CustomProviderConflict(_)
            | Self::CustomProviderInvalid { .. } => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
        }
    }
}

pub type EngineResult<T> = Result<T, EngineServiceError>;
