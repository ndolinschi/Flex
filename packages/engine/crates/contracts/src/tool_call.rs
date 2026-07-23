use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::content::ToolResultBlock;
use crate::ids::{MessageId, PermissionRequestId, SessionId, ToolCallId, TurnId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "origin", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallOrigin {
    Model,
    Hook,
    Subagent,
    External { agent_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallStatus {
    Pending,
    AwaitingPermission {
        request_id: PermissionRequestId,
    },
    Running,
    Completed,
    Failed {
        error: String,
    },
    Denied {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Cancelled,
}

impl ToolCallStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed { .. } | Self::Denied { .. } | Self::Cancelled
        )
    }

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallTiming {
    pub queued_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_wait_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
}

impl ToolCallTiming {
    pub fn duration_ms(&self) -> Option<u64> {
        match (self.started_at_ms, self.finished_at_ms) {
            (Some(start), Some(end)) => Some(end.saturating_sub(start)),
            _ => None,
        }
    }

    pub fn total_ms(&self) -> Option<u64> {
        self.finished_at_ms
            .map(|end| end.saturating_sub(self.queued_at_ms))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolOutput {
    pub content: Vec<ToolResultBlock>,
    #[serde(default)]
    pub is_error: bool,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    pub id: ToolCallId,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub message_id: MessageId,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub read_only: bool,
    pub origin: ToolCallOrigin,
    pub status: ToolCallStatus,
    pub timing: ToolCallTiming,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolOutput>,
}
