//! Hook wire types (data only — the `Hook` trait lives in the core crate).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where in the loop a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HookPoint {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    Stop,
    SubagentStart,
    SubagentStop,
    PreCompact,
    SessionEnd,
}

/// What a hook did (as recorded in the event stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HookOutcomeKind {
    Continue,
    Block,
    Mutated,
}
