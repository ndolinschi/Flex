//! `ToolCall` — the first-class record of one tool invocation.
//!
//! Not a fire-and-forget future: a tracked entity with identity, arguments,
//! a status machine, timing, and result. The loop emits
//! [`crate::event::AgentEvent::ToolCallUpdated`] on every status transition,
//! so the session log holds the full lifecycle history and the transcript
//! shows the final state. Delegator adapters synthesize the same records from
//! external agents' tool activity (`origin: External`), which makes "what ran,
//! with what arguments, for how long, allowed by whom" answerable identically
//! for native and delegated runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::content::ToolResultBlock;
use crate::ids::{MessageId, PermissionRequestId, SessionId, ToolCallId, TurnId};

/// Who initiated the call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "origin", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallOrigin {
    /// Requested by the model through native tool use.
    Model,
    /// Injected by a hook.
    Hook,
    /// Issued on behalf of a subagent.
    Subagent,
    /// Observed from an external (delegated) agent.
    External { agent_id: String },
}

/// Lifecycle state of a tool call.
///
/// ```text
/// Pending ──► AwaitingPermission ──► Running ──► Completed
///    │                │                 │  └───► Failed
///    │                └───► Denied      └──────► Cancelled
///    └─────► Running | Denied | Cancelled
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallStatus {
    /// Parsed from model output, queued for execution.
    Pending,
    /// Waiting on a user permission decision.
    AwaitingPermission { request_id: PermissionRequestId },
    /// Executing.
    Running,
    /// Finished. The result may still carry `is_error: true` (a *tool-level*
    /// error fed back to the model, e.g. "file not found").
    Completed,
    /// Engine-level failure: panic, timeout, I/O error in the tool runtime.
    Failed { error: String },
    /// Refused by the permission policy, a hook, or the user.
    Denied {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The turn was interrupted while this call was pending or running.
    Cancelled,
}

impl ToolCallStatus {
    /// Terminal states admit no further transitions.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed { .. } | Self::Denied { .. } | Self::Cancelled
        )
    }

    /// Whether the status machine admits `self -> next`.
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::AwaitingPermission { .. })
                | (Self::Pending, Self::Running)
                | (Self::Pending, Self::Denied { .. })
                | (Self::Pending, Self::Cancelled)
                | (Self::AwaitingPermission { .. }, Self::Running)
                | (Self::AwaitingPermission { .. }, Self::Denied { .. })
                | (Self::AwaitingPermission { .. }, Self::Cancelled)
                | (Self::Running, Self::Completed)
                | (Self::Running, Self::Failed { .. })
                | (Self::Running, Self::Cancelled)
        )
    }
}

/// Wall-clock milestones of one call. All fields are unix epoch milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallTiming {
    /// When the call was admitted (status `Pending`).
    pub queued_at_ms: u64,
    /// Time spent waiting for a permission decision, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_wait_ms: Option<u64>,
    /// When execution started (status `Running`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    /// When a terminal state was reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
}

impl ToolCallTiming {
    /// Execution duration (started -> finished), if both are known.
    pub fn duration_ms(&self) -> Option<u64> {
        match (self.started_at_ms, self.finished_at_ms) {
            (Some(start), Some(end)) => Some(end.saturating_sub(start)),
            _ => None,
        }
    }

    /// End-to-end latency (queued -> finished), if finished.
    pub fn total_ms(&self) -> Option<u64> {
        self.finished_at_ms
            .map(|end| end.saturating_sub(self.queued_at_ms))
    }
}

/// What a tool returned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolOutput {
    pub content: Vec<ToolResultBlock>,
    /// Tool-level error: fed back to the model as a failed result, never a
    /// loop failure.
    #[serde(default)]
    pub is_error: bool,
    /// Machine-readable payload for consumers that don't want to parse text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultBlock::markdown(text)],
            is_error: false,
            structured: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultBlock::markdown(text)],
            is_error: true,
            structured: None,
        }
    }

    /// Flatten the result content to displayable text (non-text blocks become
    /// placeholders). Used by the markdown projection and seed-history text.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        for block in &self.content {
            if !out.is_empty() {
                out.push('\n');
            }
            match block {
                ToolResultBlock::Markdown { text } => out.push_str(text),
                ToolResultBlock::Image { media_type, .. } => {
                    out.push_str(&format!("[image: {media_type}]"));
                }
                ToolResultBlock::Json { value } => {
                    out.push_str(
                        &serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
                    );
                }
            }
        }
        out
    }
}

/// The durable, inspectable record of one tool invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    pub id: ToolCallId,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    /// The assistant message that requested this call.
    pub message_id: MessageId,
    pub tool_name: String,
    pub input: serde_json::Value,
    /// Read-only calls may execute concurrently; mutating calls run
    /// sequentially. Also feeds the permission policy.
    pub read_only: bool,
    pub origin: ToolCallOrigin,
    pub status: ToolCallStatus,
    pub timing: ToolCallTiming,
    /// Present once `Completed` or `Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolOutput>,
}
