//! Goal-loop orchestration: repeated turns until a stop rule fires.

use agentloop_contracts::{
    GoalOutcome, GoalSpec, GoalStopReason, PromptInput, SessionId, TokenUsage, TurnOptions,
    TurnStopReason, VerdictOutcome,
};

use crate::EngineResult;
use crate::service::EngineService;

/// Per-category failure tally for [`EngineService::run_goal`]. Deliberately
/// coarse — grouped by [`TurnStopReason`], not by error message — since a
/// `TurnSummary` carries no error text (that lives in a separate
/// `SessionError` event); this is the signal actually available without
/// scanning the log for prose to fuzzy-match.
#[derive(Debug, Default)]
struct FailureCounts {
    error: u32,
    max_iterations: u32,
    refusal: u32,
}

impl FailureCounts {
    /// Record `stop_reason` if it is failure-like, returning the updated
    /// count for its category (`0` for a non-failure stop reason).
    fn record(&mut self, stop_reason: TurnStopReason) -> u32 {
        match stop_reason {
            TurnStopReason::Error => {
                self.error += 1;
                self.error
            }
            TurnStopReason::MaxIterations => {
                self.max_iterations += 1;
                self.max_iterations
            }
            TurnStopReason::Refusal => {
                self.refusal += 1;
                self.refusal
            }
            _ => 0,
        }
    }
}

impl EngineService {
    /// Drive repeated turns on `session` toward `goal`, stopping at the
    /// first applicable rule (see `agentloop_contracts::goal` for the
    /// stop-reason vocabulary — `Parked` is reserved and never returned
    /// here). Each iteration after the first re-states the goal rather than
    /// repeating the original prompt verbatim, since a single-turn "continue"
    /// nudge is what actually drives repeated turns forward.
    pub async fn run_goal(&self, session: &SessionId, goal: GoalSpec) -> EngineResult<GoalOutcome> {
        let mut turns = Vec::new();
        let mut total_usage = TokenUsage::default();
        let mut failures = FailureCounts::default();
        let mut next_prompt = goal.prompt.clone();
        let mut iterations = 0u32;

        loop {
            if iterations >= goal.max_iterations {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::MaxIterations,
                    iterations,
                    total_usage,
                    turns,
                });
            }
            if let Some(budget) = goal.token_budget {
                if total_usage.input + total_usage.output >= budget {
                    return Ok(GoalOutcome {
                        stop_reason: GoalStopReason::TokenBudgetExceeded,
                        iterations,
                        total_usage,
                        turns,
                    });
                }
            }

            let summary = self
                .prompt(
                    session,
                    PromptInput::text(next_prompt.clone()),
                    TurnOptions::default(),
                )
                .await?;
            iterations += 1;
            total_usage.add(&summary.usage);
            turns.push(summary.clone());

            if summary.stop_reason == TurnStopReason::Cancelled {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::Cancelled,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            if failures.record(summary.stop_reason) >= goal.max_identical_failures {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::IdenticalFailureCeiling,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            if goal.require_verification {
                match self.verify_goal_progress(session, &goal.prompt).await? {
                    Some(verdict) if verdict.outcome == VerdictOutcome::Pass => {
                        return Ok(GoalOutcome {
                            stop_reason: GoalStopReason::Achieved,
                            iterations,
                            total_usage,
                            turns,
                        });
                    }
                    Some(verdict) => {
                        next_prompt = format!(
                            "An independent verifier checked this against the goal and found \
                             issues:\n{}\n\nAddress them, then continue.",
                            verdict.findings.join("\n")
                        );
                        continue;
                    }
                    // The verifier plugin is disabled, or the model didn't
                    // call Verify — fall through to the weaker signal below
                    // rather than loop forever on a check that can't run.
                    None => {}
                }
            } else if summary.stop_reason == TurnStopReason::EndTurn && summary.num_tool_calls == 0
            {
                return Ok(GoalOutcome {
                    stop_reason: GoalStopReason::Achieved,
                    iterations,
                    total_usage,
                    turns,
                });
            }

            next_prompt = format!(
                "Continue working toward this goal:\n{}\n\nIf you believe it's fully \
                 complete, say so explicitly and stop calling tools.",
                goal.prompt
            );
        }
    }
}
