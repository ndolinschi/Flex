use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, ModeSwitchId, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PendingMap, PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError,
};

use crate::fs::schema_of;

const DEFAULT_TIMEOUT_MS: u64 = 2_000;
const MAX_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SwitchModeInput {
    mode: String,
    reason: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub struct SwitchModeTool {
    pending: Arc<PendingMap<ModeSwitchId, bool>>,
}

impl SwitchModeTool {
    pub fn new(pending: Arc<PendingMap<ModeSwitchId, bool>>) -> Self {
        Self { pending }
    }
}

#[async_trait]
impl Tool for SwitchModeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "SwitchMode".to_owned(),
            description: "Propose a composer mode switch (agent → plan, plan → agent, etc.). \
                          The user sees a brief veto window; if they don't react within \
                          `timeout_ms` the switch is auto-applied. Use this when the task \
                          has changed shape — e.g. switch to `plan` when you need the user \
                          to approve a multi-file refactor before execution begins, or back \
                          to `agent` when execution can proceed autonomously. Valid modes: \
                          `\"agent\"`, `\"plan\"`, `\"ask\"`, `\"debug\"`. Keep `reason` \
                          short and user-facing — it appears in the veto notification."
                .to_owned(),
            input_schema: schema_of::<SwitchModeInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: SwitchModeInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`SwitchMode` input must be {{\"mode\": \"agent|plan|ask|debug\", \
                 \"reason\": \"...\", \"timeout_ms\": <optional>}}: {err}."
            ))
        })?;

        let mode = input.mode.trim().to_lowercase();
        if !matches!(mode.as_str(), "agent" | "plan" | "ask" | "debug") {
            return Err(ToolError::InvalidInput(format!(
                "`mode` must be one of \"agent\", \"plan\", \"ask\", or \"debug\"; \
                 got \"{mode}\"."
            )));
        }
        if input.reason.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`reason` cannot be empty — it is shown to the user in the veto notification."
                    .to_owned(),
            ));
        }

        let timeout_ms = input
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let id = ModeSwitchId::generate();

        ctx.events.emit(AgentEvent::ModeSwitchProposed {
            id: id.clone(),
            mode: mode.clone(),
            reason: input.reason.clone(),
            timeout_ms,
        });

        let wait = self
            .pending
            .wait(id.clone(), Duration::from_millis(timeout_ms));

        let allowed = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = wait => result.unwrap_or(true),
        };

        if allowed {
            ctx.events.emit(AgentEvent::ModeSwitchApplied {
                id,
                mode: mode.clone(),
            });
            Ok(ToolOutput {
                content: vec![ToolResultBlock::markdown(format!(
                    "Mode switch to `{mode}` applied."
                ))],
                is_error: false,
                structured: Some(serde_json::json!({ "outcome": "applied", "mode": mode })),
            })
        } else {
            ctx.events.emit(AgentEvent::ModeSwitchRejected {
                id,
                mode: mode.clone(),
                reason: None,
            });
            Ok(ToolOutput {
                content: vec![ToolResultBlock::markdown(format!(
                    "Mode switch to `{mode}` was rejected by the user. \
                     Continue in the current mode."
                ))],
                is_error: false,
                structured: Some(serde_json::json!({ "outcome": "rejected", "mode": mode })),
            })
        }
    }
}

#[cfg(test)]
mod switch_mode_tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{ModeSwitchId, SessionId, ToolCallId, TurnId};
    use agentloop_core::{EventSink, PendingMap, Tool, ToolContext, ToolError};

    use super::SwitchModeTool;

    fn make_ctx(session_id: &str) -> ToolContext {
        let (sink, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId(session_id.to_owned()),
            turn_id: TurnId::generate(),
            call_id: ToolCallId::generate(),
            cwd: PathBuf::from("/tmp"),
            cancel: CancellationToken::new(),
            events: sink,
        }
    }

    #[tokio::test]
    async fn timeout_auto_applies() {
        let pending: Arc<PendingMap<ModeSwitchId, bool>> = Arc::new(PendingMap::new());
        let tool = SwitchModeTool::new(pending);

        let input = serde_json::json!({
            "mode": "plan",
            "reason": "needs approval",
            "timeout_ms": 50
        });

        let output = tool
            .run(make_ctx("session-a"), input)
            .await
            .expect("tool should succeed on timeout");
        assert!(!output.is_error);
        let structured = output.structured.unwrap();
        assert_eq!(structured["outcome"], "applied");
        assert_eq!(structured["mode"], "plan");
    }

    #[tokio::test]
    async fn explicit_reject_via_event_sink() {
        let pending: Arc<PendingMap<ModeSwitchId, bool>> = Arc::new(PendingMap::new());
        let tool = Arc::new(SwitchModeTool::new(pending.clone()));

        let (sink, mut rx) = EventSink::channel();
        let ctx = ToolContext {
            session_id: SessionId("session-b".to_owned()),
            turn_id: TurnId::generate(),
            call_id: ToolCallId::generate(),
            cwd: PathBuf::from("/tmp"),
            cancel: CancellationToken::new(),
            events: sink,
        };

        let input = serde_json::json!({
            "mode": "ask",
            "reason": "clarification needed",
            "timeout_ms": 2000
        });

        let tool_clone = tool.clone();
        let pending_clone = pending.clone();
        let handle = tokio::spawn(async move { tool_clone.run(ctx, input).await });

        let proposed_id = loop {
            if let Some(agentloop_contracts::AgentEvent::ModeSwitchProposed { id, .. }) =
                rx.recv().await
            {
                break id;
            }
        };

        pending_clone.resolve(&proposed_id, false);

        let output = handle
            .await
            .expect("task should not panic")
            .expect("tool should return Ok");
        assert!(!output.is_error);
        let structured = output.structured.unwrap();
        assert_eq!(structured["outcome"], "rejected");
        assert_eq!(structured["mode"], "ask");
    }

    #[tokio::test]
    async fn invalid_mode_is_rejected() {
        let pending: Arc<PendingMap<ModeSwitchId, bool>> = Arc::new(PendingMap::new());
        let tool = SwitchModeTool::new(pending);

        let input = serde_json::json!({
            "mode": "turbo",
            "reason": "go fast"
        });

        let result = tool.run(make_ctx("session-c"), input).await;
        assert!(
            matches!(result, Err(ToolError::InvalidInput(_))),
            "expected InvalidInput for unknown mode, got: {result:?}"
        );
    }
}
