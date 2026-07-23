use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{SessionId, TurnId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CheckpointLabel {
    TurnCompleted,
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointRef {
    pub session_id: SessionId,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    pub ts_ms: u64,
    pub label: CheckpointLabel,
}
