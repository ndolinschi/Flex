use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use agentloop_contracts::{
    AgentCaps, AgentInfo, Answer, CompactionSummary, EngineError, ModeSwitchId, NewSessionParams,
    PermissionDecision, PermissionMode, PermissionRequestId, PromptInput, QuestionId, SessionEvent,
    SessionId, SessionMeta, TurnOptions, TurnSummary,
};

use crate::provider::ProviderError;
use crate::store::StoreError;
use crate::tool::ToolError;

pub type EventStream = Pin<Box<dyn Stream<Item = SessionEvent> + Send + 'static>>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentError {
    #[error("session {0} not found")]
    SessionNotFound(SessionId),
    #[error("a turn is already in progress for session {0}")]
    TurnInProgress(SessionId),
    #[error("no pending permission request {0}")]
    UnknownPermissionRequest(PermissionRequestId),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("tool runtime failure: {0}")]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Engine(#[from] Box<EngineError>),
    #[error("{0}")]
    Other(String),
}

impl AgentError {
    pub fn to_engine_error(&self) -> EngineError {
        use agentloop_contracts::ErrorCode;
        match self {
            Self::Provider(err) => err.to_engine_error(),
            Self::Engine(err) => (**err).clone(),
            Self::SessionNotFound(_) => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
            Self::TurnInProgress(_) => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
            Self::UnknownPermissionRequest(_) => {
                EngineError::engine(ErrorCode::InvalidRequest, self.to_string())
            }
            _ => EngineError::engine(ErrorCode::Unknown, self.to_string()),
        }
    }
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn info(&self) -> AgentInfo;

    fn capabilities(&self) -> AgentCaps;

    async fn create_session(&self, params: NewSessionParams) -> Result<SessionId, AgentError>;

    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError>;

    async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError>;

    fn events(&self, session: &SessionId) -> Result<EventStream, AgentError>;

    async fn prompt(
        &self,
        session: &SessionId,
        input: PromptInput,
        opts: TurnOptions,
    ) -> Result<TurnSummary, AgentError>;

    async fn cancel(&self, session: &SessionId) -> Result<(), AgentError>;

    fn set_turn_permission_mode(
        &self,
        _session: &SessionId,
        _mode: Option<PermissionMode>,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> Result<(), AgentError>;

    async fn respond_question(
        &self,
        _session: &SessionId,
        id: QuestionId,
        _answers: Vec<Answer>,
    ) -> Result<(), AgentError> {
        Err(AgentError::Other(format!(
            "this agent implementation does not support user questions (pending id {id})"
        )))
    }

    async fn compact(
        &self,
        _session: &SessionId,
        _opts: TurnOptions,
    ) -> Result<CompactionSummary, AgentError> {
        Err(AgentError::Other(
            "this agent implementation does not support context compaction".to_owned(),
        ))
    }

    async fn respond_mode_switch(
        &self,
        _session: &SessionId,
        id: ModeSwitchId,
        _allow: bool,
    ) -> Result<(), AgentError> {
        Err(AgentError::Other(format!(
            "this agent implementation does not support mode-switch responses \
             (pending id {id}; enable_switch_mode must be set in EngineConfig)"
        )))
    }
}
