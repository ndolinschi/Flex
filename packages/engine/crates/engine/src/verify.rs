//! Goal-progress verification via the `Verify` tool.

use agentloop_contracts::{
    AgentEvent, PromptInput, SessionId, ToolCallStatus, TurnOptions, VerificationVerdict,
};

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    /// Prompt the model to call `Verify` against `goal_prompt`, then read the
    /// resulting structured verdict back out of the session's own log (same
    /// extraction the `verifier` plugin's `Verify` tool already performs for
    /// the caller that spawned it — see `agentloop_loop::subagent`). Returns
    /// `None` when no completed `Verify` call is found (tool unavailable, or
    /// the model didn't call it).
    pub(crate) async fn verify_goal_progress(
        &self,
        session: &SessionId,
        goal_prompt: &str,
    ) -> EngineResult<Option<VerificationVerdict>> {
        let verify_prompt = format!(
            "Call the Verify tool now — rubric: \"{goal_prompt}\" is fully and correctly \
             done. List the files you changed (or the relevant output) as artifacts. Call \
             Verify exactly once; do no other work this turn."
        );
        self.prompt(
            session,
            PromptInput::text(verify_prompt),
            TurnOptions::default(),
        )
        .await?;
        let events = self.store.read(session, 0).await?;
        Ok(events.iter().rev().find_map(|stored| {
            let AgentEvent::ToolCallUpdated { call } = &stored.event else {
                return None;
            };
            if call.tool_name != agentloop_core::tool::VERIFIER_TOOL_NAME
                || call.status != ToolCallStatus::Completed
            {
                return None;
            }
            call.result
                .as_ref()
                .and_then(|output| output.structured.clone())
                .and_then(|value| serde_json::from_value::<VerificationVerdict>(value).ok())
        }))
    }
}
