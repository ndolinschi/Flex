//! Verification verdicts: the structured outcome of an independent verifier
//! grading artifacts against a rubric — "maker is never the grader". The
//! verifier sees only the rubric and the artifacts, never the reasoning that
//! produced them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an independent verifier concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VerdictOutcome {
    /// The artifacts satisfy the rubric.
    Pass,
    /// The artifacts do not satisfy the rubric.
    Fail,
    /// The verifier could not reach a confident conclusion either way.
    Inconclusive,
}

/// The structured result of one verification, reported via the
/// `SubmitVerdict` tool and carried on the `Verify` call's `ToolOutput` so
/// callers can act on it without parsing prose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationVerdict {
    pub outcome: VerdictOutcome,
    /// Specific observations backing the outcome, one per finding.
    pub findings: Vec<String>,
    /// Self-reported confidence in `[0.0, 1.0]`, when the verifier gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}
