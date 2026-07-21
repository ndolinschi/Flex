//! `VerifiedMemoryGateHook`: require a passing `Verify` verdict before
//! `SkillSave`/`MemoryWrite` commit — the bridge between the independent
//! verifier (`agentloop_core::tool::VERIFIER_TOOL_NAME`) and self-learning
//! memory, so learned skills/notes can't compound on unverified self-report.
//! Opt-in; off unless a `LearningPlugin` is built with
//! [`LearningPlugin::require_verified_memory`].

use async_trait::async_trait;

use agentloop_contracts::HookPoint;
use agentloop_contracts::{ToolCallStatus, VerdictOutcome, VerificationVerdict};
use agentloop_core::hook::{HookContext, HookData, HookOutcome};
use agentloop_core::tool::VERIFIER_TOOL_NAME;
use agentloop_core::{Hook, HookError};

const GATED_TOOLS: [&str; 2] = ["SkillSave", "MemoryWrite"];

/// `PreToolUse`: blocks `SkillSave`/`MemoryWrite` unless the most recent
/// completed `Verify` call in this session's own log reported
/// [`VerdictOutcome::Pass`]. Reads the session's log through
/// `HookContext.store`; if the caller gave no store, it cannot confirm a
/// verdict and blocks (fails closed).
#[derive(Debug, Default)]
pub struct VerifiedMemoryGateHook;

impl VerifiedMemoryGateHook {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Hook for VerifiedMemoryGateHook {
    fn interests(&self) -> &[HookPoint] {
        &[HookPoint::PreToolUse]
    }

    async fn on(
        &self,
        _point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        let HookData::ToolUse { call } = &ctx.data else {
            return Ok(HookOutcome::Continue);
        };
        if !GATED_TOOLS.contains(&call.tool_name.as_str()) {
            return Ok(HookOutcome::Continue);
        }

        let Some(store) = &ctx.store else {
            return Ok(HookOutcome::Block {
                reason: "cannot save to memory: no session log access to confirm a Verify \
                         verdict"
                    .to_owned(),
            });
        };
        let Ok(events) = store.read(ctx.session_id, 0).await else {
            return Ok(HookOutcome::Block {
                reason: "cannot save to memory: failed to read this session's log to confirm a \
                         Verify verdict"
                    .to_owned(),
            });
        };
        let last_verdict = events.iter().rev().find_map(|stored| {
            let agentloop_contracts::AgentEvent::ToolCallUpdated { call } = &stored.event else {
                return None;
            };
            if call.tool_name != VERIFIER_TOOL_NAME || call.status != ToolCallStatus::Completed {
                return None;
            }
            call.result
                .as_ref()
                .and_then(|output| output.structured.clone())
                .and_then(|value| serde_json::from_value::<VerificationVerdict>(value).ok())
        });

        match last_verdict {
            Some(verdict) if verdict.outcome == VerdictOutcome::Pass => Ok(HookOutcome::Continue),
            Some(_) => Ok(HookOutcome::Block {
                reason: "this session's most recent Verify call did not pass — fix the issues \
                         it found, or call Verify again once they're resolved, before saving to \
                         memory"
                    .to_owned(),
            }),
            None => Ok(HookOutcome::Block {
                reason: "this session requires a passing Verify verdict before saving to \
                         memory — call the Verify tool with a rubric and the relevant \
                         artifacts, then retry"
                    .to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentloop_contracts::{
        AgentEvent, SessionId, ToolCall, ToolCallOrigin, ToolCallTiming, ToolOutput,
    };
    use agentloop_core::SessionStore;
    use agentloop_session::MemoryStore;

    use super::*;

    fn skill_save_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            session_id: SessionId::from("s"),
            turn_id: "t".into(),
            message_id: "m".into(),
            tool_name: "SkillSave".to_owned(),
            input: serde_json::json!({}),
            read_only: false,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Pending,
            timing: ToolCallTiming::default(),
            result: None,
        }
    }

    fn verify_call(outcome: VerdictOutcome) -> ToolCall {
        let verdict = VerificationVerdict {
            outcome,
            findings: vec!["because".to_owned()],
            confidence: None,
        };
        ToolCall {
            id: "call-0".into(),
            session_id: SessionId::from("s"),
            turn_id: "t".into(),
            message_id: "m".into(),
            tool_name: VERIFIER_TOOL_NAME.to_owned(),
            input: serde_json::json!({}),
            read_only: true,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Completed,
            timing: ToolCallTiming::default(),
            result: Some(ToolOutput {
                content: Vec::new(),
                is_error: false,
                structured: Some(serde_json::to_value(&verdict).unwrap()),
            }),
        }
    }

    async fn store_with_events(events: Vec<AgentEvent>) -> Arc<dyn SessionStore> {
        let store = MemoryStore::new();
        let id = SessionId::from("s");
        store
            .create(agentloop_contracts::SessionMeta {
                id: id.clone(),
                title: None,
                agent_id: "native".to_owned(),
                parent_id: None,
                role: None,
                depth: 0,
                provider_session_id: None,
                cwd: std::path::PathBuf::from("."),
                model: None,
                fallback_models: Vec::new(),
                mode: None,
                isolation: None,
                workspace_id: None,
                executor: None,
                base_cwd: None,
                reuse_workspace_id: None,
                created_at_ms: 1,
                updated_at_ms: 1,
            })
            .await
            .unwrap();
        store.append(&id, &events).await.unwrap();
        Arc::new(store)
    }

    #[tokio::test]
    async fn blocks_when_no_verify_call_happened() {
        let session = SessionId::from("s");
        let store = store_with_events(Vec::new()).await;
        let mut call = skill_save_call();
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: None,
            data: HookData::ToolUse { call: &mut call },
            store: Some(store),
            events: None,
        };
        let outcome = VerifiedMemoryGateHook::new()
            .on(HookPoint::PreToolUse, &mut ctx)
            .await
            .unwrap();
        assert!(matches!(outcome, HookOutcome::Block { .. }));
    }

    #[tokio::test]
    async fn allows_when_the_most_recent_verify_call_passed() {
        let session = SessionId::from("s");
        let store = store_with_events(vec![AgentEvent::ToolCallUpdated {
            call: verify_call(VerdictOutcome::Pass),
        }])
        .await;
        let mut call = skill_save_call();
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: None,
            data: HookData::ToolUse { call: &mut call },
            store: Some(store),
            events: None,
        };
        let outcome = VerifiedMemoryGateHook::new()
            .on(HookPoint::PreToolUse, &mut ctx)
            .await
            .unwrap();
        assert_eq!(outcome, HookOutcome::Continue);
    }

    #[tokio::test]
    async fn blocks_when_the_most_recent_verify_call_failed() {
        let session = SessionId::from("s");
        let store = store_with_events(vec![AgentEvent::ToolCallUpdated {
            call: verify_call(VerdictOutcome::Fail),
        }])
        .await;
        let mut call = skill_save_call();
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: None,
            data: HookData::ToolUse { call: &mut call },
            store: Some(store),
            events: None,
        };
        let outcome = VerifiedMemoryGateHook::new()
            .on(HookPoint::PreToolUse, &mut ctx)
            .await
            .unwrap();
        assert!(matches!(outcome, HookOutcome::Block { .. }));
    }

    #[tokio::test]
    async fn ignores_unrelated_tools() {
        let session = SessionId::from("s");
        let store = store_with_events(Vec::new()).await;
        let mut call = ToolCall {
            tool_name: "Read".to_owned(),
            ..skill_save_call()
        };
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: None,
            data: HookData::ToolUse { call: &mut call },
            store: Some(store),
            events: None,
        };
        let outcome = VerifiedMemoryGateHook::new()
            .on(HookPoint::PreToolUse, &mut ctx)
            .await
            .unwrap();
        assert_eq!(outcome, HookOutcome::Continue);
    }
}
