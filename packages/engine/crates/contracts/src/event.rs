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
use crate::ids::{
    MessageId, ModeSwitchId, PeerMessageId, PermissionRequestId, QuestionId, SessionId,
    ToolCallId, TurnId,
};
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
    /// Incremental output from a running exec-style tool (e.g. Bash).
    /// Ephemeral streaming data — the complete output still arrives in the
    /// final `ToolCallUpdated`.
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
    /// The full [`ToolCall`] record, re-emitted on every status transition.
    /// The log keeps the history; the transcript keeps the latest.
    ToolCallUpdated {
        call: ToolCall,
    },
    /// The agent's working plan changed (task list tools, ACP plan updates).
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
    /// Context compaction is about to call the summarizer. Ephemeral: not
    /// persisted — a UI can show "Compacting context…" until the following
    /// [`AgentEvent::CompactionBoundary`] (or turn end / error) lands.
    /// `strategy` matches the eventual boundary (`summarize_oldest`,
    /// `auto_summarize_oldest`, …).
    CompactionStarted {
        strategy: String,
    },
    /// Code index build is about to run (first build or a substantial update).
    /// Ephemeral: not persisted — a UI can show "Indexing repository…" until
    /// the following [`AgentEvent::IndexingCompleted`] (or turn end / error).
    /// `reason` is a short machine tag (`first_build`, `update`).
    IndexingStarted {
        reason: String,
    },
    /// Code index build finished. Persisted so the chat can show a settled
    /// "Indexed N files" card. Counts mirror `UpdateStats` from the index crate.
    IndexingCompleted {
        added: u32,
        changed: u32,
        removed: u32,
        unchanged: u32,
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
    /// A RETRYABLE provider/network failure (timeout, dropped connection,
    /// mid-stream cut, 5xx, rate limit) is about to be retried on the *same*
    /// model after a backoff sleep, instead of failing the turn or advancing
    /// the fallback chain. Emitted once per scheduled attempt, right before
    /// the sleep starts, so a UI can render e.g. "Reconnecting… attempt 3/10,
    /// retrying in 30s". Ephemeral: not persisted, since normal streaming
    /// resuming afterward is already visible via the next stream events.
    RetryScheduled {
        /// 1-indexed retry attempt about to be slept for (`1` = first retry
        /// after the initial call failed).
        attempt: u32,
        /// Total attempts allowed by the schedule, counting the initial call.
        max_attempts: u32,
        /// How long this attempt will sleep before retrying, in milliseconds.
        delay_ms: u64,
        /// Human-readable description of the failure that triggered the retry.
        error: String,
    },
    HookFired {
        point: HookPoint,
        outcome: HookOutcomeKind,
    },

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
    /// A per-turn snapshot of the working tree was captured (git-gated), so the
    /// session's file changes since the last turn can be rewound via `/undo`.
    SnapshotCreated {
        /// Opaque, restorable snapshot id (a git commit sha).
        snapshot_id: String,
        /// The turn this snapshot was taken at the end of.
        turn_id: TurnId,
    },
    /// The working tree was rewound to an earlier snapshot (`/undo`/`/redo`).
    /// An audit marker only — the append-only log is retained.
    SnapshotRestored {
        snapshot_id: String,
    },

    /// Peer-to-peer agent message (persisted on the recipient session log).
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
    /// Auto/router proposed a composer-mode switch; UI shows a veto window.
    /// Persisted so chat history can display the decision chip on resume.
    ModeSwitchProposed {
        id: ModeSwitchId,
        /// One of `"agent"`, `"plan"`, `"ask"`, `"debug"`.
        mode: String,
        reason: String,
        timeout_ms: u64,
    },
    /// The proposed mode switch was accepted and applied.
    ModeSwitchApplied {
        id: ModeSwitchId,
        mode: String,
    },
    /// The proposed mode switch was vetoed by the user or timed out.
    ModeSwitchRejected {
        id: ModeSwitchId,
        mode: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

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

/// Which stream an [`AgentEvent::ExecChunk`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecStream {
    Stdout,
    Stderr,
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
                | Self::ExecChunk { .. }
                | Self::SubagentEvent { .. }
                | Self::CompactionStarted { .. }
                | Self::IndexingStarted { .. }
                | Self::RetryScheduled { .. }
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
