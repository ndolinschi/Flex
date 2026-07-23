use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::capability::AgentCaps;
use crate::content::{ContentBlock, Role};
use crate::error::EngineError;
use crate::hook::{HookOutcomeKind, HookPoint};
use crate::ids::{
    MessageId, ModeSwitchId, PeerMessageId, PermissionRequestId, QuestionId, SessionId, ToolCallId,
    TurnId,
};
use crate::permission::{Answer, PermissionDecision, PermissionDecisionKind, Question};
use crate::session::{CompactionSummary, PlanEntry, SessionMeta, TurnSummary};
use crate::tool_call::ToolCall;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionEvent {
    pub session_id: SessionId,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    pub ts_ms: u64,
    pub payload: AgentEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    SessionCreated {
        meta: SessionMeta,
    },
    EngineInfo {
        agent_id: String,
        capabilities: AgentCaps,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        resolution_trace: Vec<String>,
    },
    TurnStarted {
        turn_id: TurnId,
    },
    TurnCompleted {
        turn_id: TurnId,
        summary: TurnSummary,
    },
    SessionError {
        error: EngineError,
    },

    MessageStarted {
        message_id: MessageId,
        role: Role,
    },
    MarkdownDelta {
        message_id: MessageId,
        text: String,
    },
    ThinkingDelta {
        message_id: MessageId,
        text: String,
    },
    TextSnapshot {
        message_id: MessageId,
        text: String,
    },
    ToolArgsDelta {
        call_id: ToolCallId,
        json_fragment: String,
    },
    ToolProgress {
        call_id: ToolCallId,
        note: String,
    },
    ExecChunk {
        call_id: ToolCallId,
        stream: ExecStream,
        text: String,
    },

    UserMessage {
        message_id: MessageId,
        content: Vec<ContentBlock>,
    },
    AssistantMessage {
        message_id: MessageId,
        content: Vec<ContentBlock>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<crate::session::TokenUsage>,
    },
    ToolCallUpdated {
        call: ToolCall,
    },
    PlanUpdated {
        entries: Vec<PlanEntry>,
    },

    PermissionRequested {
        id: PermissionRequestId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<ToolCallId>,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        options: Vec<PermissionDecisionKind>,
    },
    PermissionResolved {
        id: PermissionRequestId,
        decision: PermissionDecision,
    },
    QuestionRequested {
        id: QuestionId,
        questions: Vec<Question>,
    },
    QuestionResolved {
        id: QuestionId,
        answers: Vec<Answer>,
    },
    CommandExpanded {
        name: String,
        args: String,
    },
    CompactionBoundary {
        summary: CompactionSummary,
    },
    CompactionStarted {
        strategy: String,
    },
    IndexingStarted {
        reason: String,
    },
    IndexingCompleted {
        added: u32,
        changed: u32,
        removed: u32,
        unchanged: u32,
    },
    ModelFallback {
        from: crate::capability::ModelRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<crate::capability::ModelRef>,
        reason: EngineError,
    },
    RetryScheduled {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: String,
    },
    HookFired {
        point: HookPoint,
        outcome: HookOutcomeKind,
    },

    SubagentStarted {
        child_session: SessionId,
        task: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<ToolCallId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
    },
    SubagentEvent {
        child_session: SessionId,
        event: Box<AgentEvent>,
    },
    SubagentCompleted {
        child_session: SessionId,
        summary: TurnSummary,
    },

    WorkspaceProvisioned {
        workspace_id: String,
        path: std::path::PathBuf,
        base_ref: String,
    },
    WorkspaceIntegrated {
        workspace_id: String,
        outcome: crate::workspace::IntegrationOutcome,
    },
    WorkspaceDiscarded {
        workspace_id: String,
    },
    SnapshotCreated {
        snapshot_id: String,
        turn_id: TurnId,
    },
    SnapshotRestored {
        snapshot_id: String,
    },

    PeerMessage {
        id: PeerMessageId,
        from: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<SessionId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        about_path: Option<String>,
    },
    ModeSwitchProposed {
        id: ModeSwitchId,
        mode: String,
        reason: String,
        timeout_ms: u64,
    },
    ModeSwitchApplied {
        id: ModeSwitchId,
        mode: String,
    },
    ModeSwitchRejected {
        id: ModeSwitchId,
        mode: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    RoutingChanged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
        reason: String,
    },

    Gap {
        from_seq: u64,
    },
    Unknown {
        raw: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecStream {
    Stdout,
    Stderr,
}

impl AgentEvent {
    pub fn is_persistent(&self) -> bool {
        !matches!(
            self,
            Self::MessageStarted { .. }
                | Self::MarkdownDelta { .. }
                | Self::ThinkingDelta { .. }
                | Self::TextSnapshot { .. }
                | Self::ToolArgsDelta { .. }
                | Self::ToolProgress { .. }
                | Self::ExecChunk { .. }
                | Self::SubagentEvent { .. }
                | Self::CompactionStarted { .. }
                | Self::IndexingStarted { .. }
                | Self::RetryScheduled { .. }
                | Self::Gap { .. }
        )
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session_created",
            Self::EngineInfo { .. } => "engine_info",
            Self::TurnStarted { .. } => "turn_started",
            Self::TurnCompleted { .. } => "turn_completed",
            Self::SessionError { .. } => "session_error",
            Self::MessageStarted { .. } => "message_started",
            Self::MarkdownDelta { .. } => "markdown_delta",
            Self::ThinkingDelta { .. } => "thinking_delta",
            Self::TextSnapshot { .. } => "text_snapshot",
            Self::ToolArgsDelta { .. } => "tool_args_delta",
            Self::ToolProgress { .. } => "tool_progress",
            Self::ExecChunk { .. } => "exec_chunk",
            Self::UserMessage { .. } => "user_message",
            Self::AssistantMessage { .. } => "assistant_message",
            Self::ToolCallUpdated { .. } => "tool_call_updated",
            Self::PlanUpdated { .. } => "plan_updated",
            Self::PermissionRequested { .. } => "permission_requested",
            Self::PermissionResolved { .. } => "permission_resolved",
            Self::QuestionRequested { .. } => "question_requested",
            Self::QuestionResolved { .. } => "question_resolved",
            Self::CommandExpanded { .. } => "command_expanded",
            Self::CompactionBoundary { .. } => "compaction_boundary",
            Self::CompactionStarted { .. } => "compaction_started",
            Self::IndexingStarted { .. } => "indexing_started",
            Self::IndexingCompleted { .. } => "indexing_completed",
            Self::ModelFallback { .. } => "model_fallback",
            Self::RetryScheduled { .. } => "retry_scheduled",
            Self::HookFired { .. } => "hook_fired",
            Self::SubagentStarted { .. } => "subagent_started",
            Self::SubagentEvent { .. } => "subagent_event",
            Self::SubagentCompleted { .. } => "subagent_completed",
            Self::WorkspaceProvisioned { .. } => "workspace_provisioned",
            Self::WorkspaceIntegrated { .. } => "workspace_integrated",
            Self::WorkspaceDiscarded { .. } => "workspace_discarded",
            Self::SnapshotCreated { .. } => "snapshot_created",
            Self::SnapshotRestored { .. } => "snapshot_restored",
            Self::PeerMessage { .. } => "peer_message",
            Self::ModeSwitchProposed { .. } => "mode_switch_proposed",
            Self::ModeSwitchApplied { .. } => "mode_switch_applied",
            Self::ModeSwitchRejected { .. } => "mode_switch_rejected",
            Self::RoutingChanged { .. } => "routing_changed",
            Self::Gap { .. } => "gap",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn from_json_lenient(value: serde_json::Value) -> Self {
        match Self::deserialize(&value) {
            Ok(event) => event,
            Err(_) => Self::Unknown { raw: value },
        }
    }
}
