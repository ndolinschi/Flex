//! Session metadata, token accounting, turn summaries, plans, compaction.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::capability::ModelRef;
use crate::ids::{SessionId, TurnId};

/// Descriptor of one session (the append-only event log it names is stored
/// separately by a `SessionStore`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMeta {
    pub id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Which agent implementation owns this session
    /// (`"native"`, `"claude-code"`, ...).
    pub agent_id: String,
    /// Set for subagent sessions; links the session tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionId>,
    /// The role this session serves (e.g. `searcher`); `None` = main.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Spawn-tree depth of this session: 0 for a root/main session, else the
    /// parent session's depth + 1. Used to enforce a role's `max_depth` so
    /// subagents cannot spawn indefinitely deep trees.
    #[serde(default)]
    pub depth: u8,
    /// The backing agent's own session id, for native resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

/// Partial update to a [`SessionMeta`]. `None` fields are left unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMetaPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Token accounting for one model call or aggregated over a turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
}

impl TokenUsage {
    /// Accumulate another usage report into this one.
    pub fn add(&mut self, other: &TokenUsage) {
        fn merge(a: &mut Option<u64>, b: Option<u64>) {
            if let Some(v) = b {
                *a = Some(a.unwrap_or(0) + v);
            }
        }
        self.input += other.input;
        self.output += other.output;
        merge(&mut self.cache_read, other.cache_read);
        merge(&mut self.cache_write, other.cache_write);
        merge(&mut self.reasoning, other.reasoning);
    }
}

/// Why a single model response stopped (provider level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Cancelled,
}

/// Why a whole turn ended (loop level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TurnStopReason {
    /// The model finished without requesting more tools.
    EndTurn,
    MaxTokens,
    /// The loop hit its per-turn iteration bound.
    MaxIterations,
    Refusal,
    Cancelled,
    Error,
}

/// Aggregated result of one turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnSummary {
    pub turn_id: TurnId,
    pub stop_reason: TurnStopReason,
    pub usage: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    pub num_model_calls: u32,
    pub num_tool_calls: u32,
    pub duration_ms: u64,
}

/// Status of one plan/task entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

/// One entry of the agent's working plan (task list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanEntry {
    pub content: String,
    pub status: PlanStatus,
}

/// Record of a context compaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompactionSummary {
    /// Markdown summary that replaces the compacted prefix when building
    /// model context.
    pub summary_markdown: String,
    /// Which strategy produced it (`"summarize_oldest"`, `"truncate"`, ...).
    pub strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_before: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_after: Option<u64>,
}
