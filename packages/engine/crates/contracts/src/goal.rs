use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session::{TokenUsage, TurnSummary};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GoalSpec {
    pub prompt: String,
    pub max_iterations: u32,
    pub max_identical_failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub require_verification: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GoalStopReason {
    Achieved,
    Escalate,
    Parked,
    MaxIterations,
    IdenticalFailureCeiling,
    TokenBudgetExceeded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GoalOutcome {
    pub stop_reason: GoalStopReason,
    pub iterations: u32,
    pub total_usage: TokenUsage,
    pub turns: Vec<TurnSummary>,
}
