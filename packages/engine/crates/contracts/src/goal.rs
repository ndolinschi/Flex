//! Goal-loops: drive repeated turns on one session toward a stated outcome,
//! bounded by explicit stop-rules instead of running until someone notices.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session::{TokenUsage, TurnSummary};

/// A goal to pursue across possibly many turns on one session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GoalSpec {
    /// The outcome to pursue, sent as the first turn's prompt.
    pub prompt: String,
    /// Hard ceiling on turns for this goal (distinct from a single turn's
    /// own `max_iterations`).
    pub max_iterations: u32,
    /// Stop once the same failure category repeats this many times in a
    /// row's worth of turns (tracked per category, not literal streaks).
    pub max_identical_failures: u32,
    /// Cumulative input+output token budget for the whole goal; `None` = no
    /// budget stop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    /// Confirm "done" with an independent `Verify` call (see
    /// `agentloop_core::tool::VERIFIER_TOOL_NAME`) instead of trusting the
    /// model's own turn-ending silence. Requires the `verifier` plugin to be
    /// enabled on the engine driving this session — with it absent, every
    /// verification attempt reports `Inconclusive` and the loop falls back
    /// to the weaker no-tool-calls signal.
    #[serde(default)]
    pub require_verification: bool,
}

/// Why a goal loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GoalStopReason {
    /// The model believes it's done (weak signal), or an independent
    /// `Verify` call confirmed it (strong signal, when
    /// `GoalSpec.require_verification` is set).
    Achieved,
    /// Reserved for a future "needs a human, cannot continue autonomously"
    /// signal; `run_goal` does not produce this yet. `AskUserQuestion`
    /// itself already blocks synchronously for an answer inside the turn
    /// that calls it, so a plain post-hoc scan for that event does not
    /// distinguish "escalation needed" from "asked and answered, normal
    /// flow" — a real signal needs visibility into whether an answer is
    /// still pending when the loop is about to continue, not just whether a
    /// question was ever asked.
    Escalate,
    /// Reserved for a future explicit "pause, resume later" signal; `run_goal`
    /// does not produce this yet.
    Parked,
    /// Hit `GoalSpec.max_iterations` without reaching a stop condition.
    MaxIterations,
    /// The same failure category repeated `GoalSpec.max_identical_failures`
    /// times.
    IdenticalFailureCeiling,
    /// Hit `GoalSpec.token_budget`.
    TokenBudgetExceeded,
    /// A turn was cancelled (external `cancel()` call).
    Cancelled,
}

/// Aggregated result of a goal loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GoalOutcome {
    pub stop_reason: GoalStopReason,
    pub iterations: u32,
    pub total_usage: TokenUsage,
    /// One summary per turn the loop ran, in order.
    pub turns: Vec<TurnSummary>,
}
