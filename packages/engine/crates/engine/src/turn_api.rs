//! Turn-facing API: hello, prompt, compact, cancel, permissions, questions,
//! and mode-switch replies.

use agentloop_contracts::{
    Answer, CompactionSummary, Hello, ModeSwitchId, PermissionDecision, PermissionMode,
    PermissionRequestId, PromptInput, QuestionId, SessionId, TurnOptions, TurnSummary,
};
use agentloop_core::{AgentError, EventStream};

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    pub fn hello(&self) -> Hello {
        let mut caps = self.agent.capabilities();
        if caps.commands.is_empty() {
            caps.commands = self.commands.infos();
        }
        Hello::new(caps)
    }
    pub fn subscribe(&self, session: &SessionId) -> EngineResult<EventStream> {
        Ok(self.agent.events(session)?)
    }

    pub async fn prompt(
        &self,
        session: &SessionId,
        input: PromptInput,
        opts: TurnOptions,
    ) -> EngineResult<TurnSummary> {
        let input = self.commands.expand_input(input);
        Ok(self.agent.prompt(session, input, opts).await?)
    }

    /// Summarize conversation history and record a compaction boundary.
    pub async fn compact(
        &self,
        session: &SessionId,
        opts: TurnOptions,
    ) -> EngineResult<CompactionSummary> {
        Ok(self.agent.compact(session, opts).await?)
    }

    pub async fn cancel(&self, session: &SessionId) -> EngineResult<()> {
        Ok(self.agent.cancel(session).await?)
    }
    /// Push a permission-mode change into an in-flight native turn.
    pub fn set_turn_permission_mode(
        &self,
        session: &SessionId,
        mode: Option<PermissionMode>,
    ) -> EngineResult<()> {
        Ok(self.agent.set_turn_permission_mode(session, mode)?)
    }

    pub async fn respond_permission(
        &self,
        session: &SessionId,
        id: PermissionRequestId,
        decision: PermissionDecision,
    ) -> EngineResult<()> {
        Ok(self.agent.respond_permission(session, id, decision).await?)
    }

    pub async fn respond_question(
        &self,
        session: &SessionId,
        id: QuestionId,
        answers: Vec<Answer>,
    ) -> EngineResult<()> {
        Ok(self.agent.respond_question(session, id, answers).await?)
    }

    /// Resolve a pending `ModeSwitchProposed` event.
    ///
    /// `allow = true` applies the switch; `allow = false` vetoes it.
    /// Returns an error when `enable_switch_mode` is false (the `SwitchMode`
    /// tool is not registered) or no proposal with `id` is pending.
    pub async fn respond_mode_switch(
        &self,
        session: &SessionId,
        id: ModeSwitchId,
        allow: bool,
    ) -> EngineResult<()> {
        if self.pending_mode_switches.is_none() {
            return Err(AgentError::Other(
                "mode-switch tool is disabled; set enable_switch_mode in EngineConfig".to_owned(),
            )
            .into());
        }
        Ok(self.agent.respond_mode_switch(session, id, allow).await?)
    }
}
