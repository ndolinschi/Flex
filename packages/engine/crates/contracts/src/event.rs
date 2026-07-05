//! The canonical event stream — Layer B of the unified stream format.
//!
//! Every producer (native providers, delegated agents, subagents) normalizes
//! into this one vocabulary. Events come in two persistence classes:
//!
//! - **Streaming deltas** are ephemeral: broadcast to live subscribers, never
//!   persisted. A consumer that missed them re-syncs from the materialized
//!   items that follow.
//! - **Materialized items and control-plane events** are persisted: they form
//!   the session's append-only log, which is the ground truth for transcripts,
//!   resumption, and observability.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::capability::AgentCaps;
use crate::content::{ContentBlock, Role};
use crate::error::EngineError;
use crate::hook::{HookOutcomeKind, HookPoint};
use crate::ids::{MessageId, PermissionRequestId, QuestionId, SessionId, ToolCallId, TurnId};
use crate::permission::{Answer, PermissionDecision, PermissionDecisionKind, Question};
use crate::session::{CompactionSummary, PlanEntry, SessionMeta, TurnSummary};
use crate::tool_call::ToolCall;

/// Envelope around one event as it travels the wire or sits in the log.
///
/// `seq` is assigned by the session store (monotonic per session, gapless for
/// persisted events); adapters and transports never mint sequence numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionEvent {
    pub session_id: SessionId,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// Unix epoch milliseconds.
    pub ts_ms: u64,
    pub payload: AgentEvent,
}

/// One canonical event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    // ── lifecycle (persisted) ───────────────────────────────────────────────
    SessionCreated {
        meta: SessionMeta,
    },
    /// Emitted once per session start: which implementation is serving it,
    /// what it can do, and how it was selected.
    EngineInfo {
        agent_id: String,
        capabilities: AgentCaps,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_session_id: Option<String>,
        /// Human-readable trace of how this agent/provider was resolved.
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

    // ── streaming deltas (ephemeral — broadcast, never persisted) ──────────
    MessageStarted {
        message_id: MessageId,
        role: Role,
    },
    /// A fragment of markdown text appended to the open message.
    MarkdownDelta {
        message_id: MessageId,
        text: String,
    },
    /// A fragment of reasoning text.
    ThinkingDelta {
        message_id: MessageId,
        text: String,
    },
    /// Full-text snapshot superseding all earlier text of the message.
    /// Emitted by snapshot-only sources instead of deltas.
    TextSnapshot {
        message_id: MessageId,
        text: String,
    },
    /// A fragment of a tool call's JSON arguments, as the model streams them.
    ToolArgsDelta {
        call_id: ToolCallId,
        json_fragment: String,
    },
    /// Progress note from a running tool.
    ToolProgress {
        call_id: ToolCallId,
        note: String,
    },

    // ── materialized items (persisted — the durable log) ───────────────────
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
    /// The full [`ToolCall`] record, re-emitted on every status transition.
    /// The log keeps the history; the transcript keeps the latest.
    ToolCallUpdated {
        call: ToolCall,
    },
    /// The agent's working plan changed (task list tools, ACP plan updates).
    PlanUpdated {
        entries: Vec<PlanEntry>,
    },

    // ── control plane (persisted) ───────────────────────────────────────────
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
    /// The agent asked the user structured questions (`AskUserQuestion`).
    QuestionRequested {
        id: QuestionId,
        questions: Vec<Question>,
    },
    QuestionResolved {
        id: QuestionId,
        answers: Vec<Answer>,
    },
    /// A slash command was resolved into this turn's prompt.
    CommandExpanded {
        name: String,
        args: String,
    },
    CompactionBoundary {
        summary: CompactionSummary,
    },
    /// The turn's model was switched mid-work (provider failure, rate limit).
    ModelFallback {
        from: crate::capability::ModelRef,
        /// The next model being tried; `None` = the chain is exhausted and
        /// the turn is about to fail.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<crate::capability::ModelRef>,
        reason: EngineError,
    },
    HookFired {
        point: HookPoint,
        outcome: HookOutcomeKind,
    },

    // ── composition ─────────────────────────────────────────────────────────
    SubagentStarted {
        child_session: SessionId,
        task: String,
        /// The Task tool call that spawned the child, when tool-driven.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<ToolCallId>,
        /// The child's role (e.g. `searcher`, `worker`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
    },
    /// Live relay of a child event into the parent stream (ephemeral — the
    /// child persists its own log).
    SubagentEvent {
        child_session: SessionId,
        event: Box<AgentEvent>,
    },
    SubagentCompleted {
        child_session: SessionId,
        summary: TurnSummary,
    },

    // ── environment isolation (persisted) ─────────────────────────────────────
    /// A session's tools were redirected into an isolated working copy
    /// (e.g. a git worktree) branched from `base_ref`.
    WorkspaceProvisioned {
        workspace_id: String,
        /// The isolated working-copy root the session's tools now operate in.
        path: std::path::PathBuf,
        /// The base commit/ref the workspace was branched from.
        base_ref: String,
    },
    /// An isolated workspace was integrated back into its base tree.
    WorkspaceIntegrated {
        workspace_id: String,
        outcome: crate::workspace::IntegrationOutcome,
    },
    /// An isolated workspace was discarded without integrating.
    WorkspaceDiscarded {
        workspace_id: String,
    },

    // ── transport hygiene ───────────────────────────────────────────────────
    /// A live subscriber lagged and missed events; re-sync from the store
    /// starting at `from_seq`. Never persisted.
    Gap {
        from_seq: u64,
    },
    /// An event this build doesn't know. Preserved verbatim — never dropped,
    /// never a crash.
    Unknown {
        raw: serde_json::Value,
    },
}

impl AgentEvent {
    /// Whether this event belongs in the durable session log.
    pub fn is_persistent(&self) -> bool {
        !matches!(
            self,
            Self::MessageStarted { .. }
                | Self::MarkdownDelta { .. }
                | Self::ThinkingDelta { .. }
                | Self::TextSnapshot { .. }
                | Self::ToolArgsDelta { .. }
                | Self::ToolProgress { .. }
                | Self::SubagentEvent { .. }
                | Self::Gap { .. }
        )
    }

    /// Stable name of the event kind, for logging and metric labels.
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
            Self::ModelFallback { .. } => "model_fallback",
            Self::HookFired { .. } => "hook_fired",
            Self::SubagentStarted { .. } => "subagent_started",
            Self::SubagentEvent { .. } => "subagent_event",
            Self::SubagentCompleted { .. } => "subagent_completed",
            Self::WorkspaceProvisioned { .. } => "workspace_provisioned",
            Self::WorkspaceIntegrated { .. } => "workspace_integrated",
            Self::WorkspaceDiscarded { .. } => "workspace_discarded",
            Self::Gap { .. } => "gap",
            Self::Unknown { .. } => "unknown",
        }
    }

    /// Deserialize leniently: an unrecognized or malformed event becomes
    /// [`AgentEvent::Unknown`] instead of an error. Transports and stores use
    /// this at trust boundaries so newer peers never break older consumers.
    pub fn from_json_lenient(value: serde_json::Value) -> Self {
        match Self::deserialize(&value) {
            Ok(event) => event,
            Err(_) => Self::Unknown { raw: value },
        }
    }
}
