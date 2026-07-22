//! The `Agent` trait — the universal unit of the engine.
//!
//! The native loop, every external-agent delegator, and every subagent are
//! `Arc<dyn Agent>`. Semantics deliberately mirror ACP: `prompt` resolves at
//! end-of-turn while events flow on a subscription, and permissions are an
//! event plus a reply — which makes both the ACP delegator and a future ACP
//! server near-mechanical mappings.

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

/// A live subscription to a session's events (enveloped, seq-stamped).
pub type EventStream = Pin<Box<dyn Stream<Item = SessionEvent> + Send + 'static>>;

/// Failures of agent operations.
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
    /// Normalize into the wire-level [`EngineError`].
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

/// Something that can run agentic turns: the native loop, a delegated
/// external agent, or a subagent — interchangeable behind this interface.
#[async_trait]
pub trait Agent: Send + Sync {
    fn info(&self) -> AgentInfo;

    fn capabilities(&self) -> AgentCaps;

    async fn create_session(&self, params: NewSessionParams) -> Result<SessionId, AgentError>;

    /// Reattach to a persisted session (natively when the backing agent
    /// supports it, else by seed-history replay).
    async fn resume_session(&self, id: &SessionId) -> Result<(), AgentError>;

    async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError>;

    /// Subscribe to a session's live events. Subscribe *before* prompting to
    /// see the whole turn; missed history is available from the session store
    /// (a lagging subscriber receives a `Gap` event and re-syncs).
    fn events(&self, session: &SessionId) -> Result<EventStream, AgentError>;

    /// Run one full agentic turn. Resolves when the turn ends (idle, error,
    /// or cancelled) while deltas/items stream via [`Agent::events`].
    async fn prompt(
        &self,
        session: &SessionId,
        input: PromptInput,
        opts: TurnOptions,
    ) -> Result<TurnSummary, AgentError>;

    /// Interrupt the in-flight turn. Idempotent; no-op when idle.
    /// Cancellation is not an error: the turn completes with
    /// `TurnStopReason::Cancelled`.
    async fn cancel(&self, session: &SessionId) -> Result<(), AgentError>;

    /// Update the permission mode for an in-flight native turn. Default: no-op
    /// (delegated agents manage their own policy).
    fn set_turn_permission_mode(
        &self,
        _session: &SessionId,
        _mode: Option<PermissionMode>,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    /// Resolve a pending `PermissionRequested` event.
    async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> Result<(), AgentError>;

    /// Resolve a pending `QuestionRequested` event (`AskUserQuestion`).
    /// Default: not supported.
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

    /// Summarize conversation history and record a compaction boundary so
    /// future turns send the summary instead of the compacted prefix.
    /// Default: not supported.
    async fn compact(
        &self,
        _session: &SessionId,
        _opts: TurnOptions,
    ) -> Result<CompactionSummary, AgentError> {
        Err(AgentError::Other(
            "this agent implementation does not support context compaction".to_owned(),
        ))
    }

    /// Resolve a pending `ModeSwitchProposed` event (`SwitchMode` tool).
    ///
    /// `allow = true` → the switch is applied (`ModeSwitchApplied` will be
    /// emitted by the tool). `allow = false` → the user vetoed it
    /// (`ModeSwitchRejected`). Default: not supported (the `SwitchMode` tool
    /// is disabled by default; enable via `EngineConfig::enable_switch_mode`).
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
