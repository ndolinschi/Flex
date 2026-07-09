//! Checkpoints: named pointers into a session's append-only log.
//!
//! A checkpoint is not separate storage — it is a `seq` the log already
//! contains, labeled so callers can enumerate resumable points without
//! replaying the whole log. Restoring one is `reduce()` over `read(0)`
//! truncated at `seq`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{SessionId, TurnId};

/// Why a checkpoint was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CheckpointLabel {
    /// Recorded after a turn's final event landed in the log.
    TurnCompleted,
    /// Recorded after a compaction boundary landed in the log.
    Compaction,
}

/// A labeled pointer at `seq` in one session's log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointRef {
    pub session_id: SessionId,
    /// The seq of the last event in the batch that earned this checkpoint.
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// Unix epoch milliseconds.
    pub ts_ms: u64,
    pub label: CheckpointLabel,
}
