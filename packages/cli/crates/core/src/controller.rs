//! Session lifecycle around one [`EngineService`].
//!
//! The controller owns the (service, session) pair and exposes exactly the
//! operations a client loop needs. Two rules it encodes:
//!
//! - **Subscribe before prompting.** The event stream must be attached right
//!   after session creation so no turn output is missed.
//! - **Errors render from the event stream.** Turn failures surface as
//!   `SessionError`/`TurnCompleted` events; the `Err` from
//!   [`SessionController::prompt`]
//!   is only the turn-terminal signal and should not be rendered a second
//!   time.

use agentloop_contracts::{
    Answer, CompactionSummary, Hello, NewSessionParams, PermissionDecision, PermissionRequestId,
    PromptInput, QuestionId, SessionEvent, SessionId, Transcript, TurnOptions, TurnSummary,
};
use agentloop_core::EventStream;
use agentloop_engine::{EngineService, EngineServiceError};

/// One live session on one service.
pub struct SessionController {
    service: EngineService,
    session: SessionId,
}

impl SessionController {
    /// Create a session and subscribe to it, returning the live event stream
    /// alongside the controller.
    pub async fn open(
        service: EngineService,
        params: NewSessionParams,
    ) -> Result<(Self, EventStream), EngineServiceError> {
        let session = service.create_session(params).await?;
        let events = service.subscribe(&session)?;
        Ok((Self { service, session }, events))
    }

    /// Resume an existing session and subscribe to it. The caller re-renders
    /// history from [`Self::transcript`].
    pub async fn resume(
        service: EngineService,
        session: SessionId,
    ) -> Result<(Self, EventStream), EngineServiceError> {
        service.resume_session(&session).await?;
        let events = service.subscribe(&session)?;
        Ok((Self { service, session }, events))
    }

    /// The session this controller drives.
    pub fn session_id(&self) -> &SessionId {
        &self.session
    }

    /// The underlying service (for capability checks and model listing).
    pub fn service(&self) -> &EngineService {
        &self.service
    }

    /// The agent's handshake: capabilities, commands, model discovery.
    pub fn hello(&self) -> Hello {
        self.service.hello()
    }

    /// Run one turn. Resolves at end-of-turn while output streams via events;
    /// run it on a spawned task, never on a render path.
    pub async fn prompt(
        &self,
        input: PromptInput,
        opts: TurnOptions,
    ) -> Result<TurnSummary, EngineServiceError> {
        self.service.prompt(&self.session, input, opts).await
    }

    /// Summarize conversation history and record a compaction boundary.
    pub async fn compact(
        &self,
        opts: TurnOptions,
    ) -> Result<CompactionSummary, EngineServiceError> {
        self.service.compact(&self.session, opts).await
    }

    /// Gracefully interrupt the running turn (idempotent; the pending
    /// [`Self::prompt`] still resolves, with a cancelled summary).
    pub async fn cancel(&self) -> Result<(), EngineServiceError> {
        self.service.cancel(&self.session).await
    }

    /// Answer a permission request raised by a tool call.
    pub async fn respond_permission(
        &self,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> Result<(), EngineServiceError> {
        self.service
            .respond_permission(&self.session, id, decision)
            .await
    }

    /// Answer a structured question raised by the agent.
    pub async fn respond_question(
        &self,
        id: QuestionId,
        answers: Vec<Answer>,
    ) -> Result<(), EngineServiceError> {
        self.service
            .respond_question(&self.session, id, answers)
            .await
    }

    /// Persisted events from `from_seq` — the re-sync path after a
    /// [`agentloop_contracts::AgentEvent::Gap`].
    pub async fn replay(&self, from_seq: u64) -> Result<Vec<SessionEvent>, EngineServiceError> {
        self.service.replay(&self.session, from_seq).await
    }

    /// The materialized transcript, for full history rebuilds.
    pub async fn transcript(&self) -> Result<Transcript, EngineServiceError> {
        self.service.session_items(&self.session).await
    }
}
