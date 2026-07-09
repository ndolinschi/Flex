//! `Verify` + `SubmitVerdict`: an independent verifier — "maker is never the
//! grader".
//!
//! `Verify` spawns a fresh `verifier`-role subagent seeded with only a
//! rubric and a list of artifact paths; the input schema has no field for
//! "what I did" or "why", so the caller cannot smuggle in the maker's
//! reasoning. This crate ships only the descriptor — the loop intercepts
//! calls by name and runs the subagent, same as `Agent`.
//!
//! `SubmitVerdict` is what the verifier calls to report its outcome: a real,
//! executable, role-restricted tool whose only effect is producing a
//! structured [`VerificationVerdict`] on the result.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock, VerificationVerdict};
use agentloop_core::tool::{SUBMIT_VERDICT_TOOL_NAME, VERIFIER_TOOL_NAME};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

/// JSON Schema for `T`, generated via `schemars`.
fn schema_of<T: JsonSchema>() -> serde_json::Value {
    let schema = schemars::schema_for!(T);
    serde_json::to_value(schema).unwrap_or_else(|_| serde_json::json!({}))
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VerifyInput {
    /// What must be true for the artifacts to pass — a rubric, not a
    /// transcript of what was done or why.
    rubric: String,
    /// Paths (relative to the working directory) the verifier may read.
    /// This is the *only* context it gets about the work under review.
    artifacts: Vec<String>,
    /// Optional model override; defaults to the `verifier` role's own chain.
    #[serde(default)]
    model: Option<String>,
}

/// The `Verify` descriptor. `run` is never reached in a correct build — the
/// loop intercepts calls by name and runs a `verifier`-role subagent instead.
struct VerifyTool;

#[async_trait]
impl Tool for VerifyTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: VERIFIER_TOOL_NAME.to_owned(),
            description: "Grade artifacts against a rubric using an independent verifier — a \
                fresh subagent that never sees your reasoning, only the rubric and the \
                artifacts you list. Use this before declaring work done, and before saving \
                anything to memory: an independent check catches mistakes self-review misses.\n\n\
                Write `rubric` as a checklist of what must be true, not a description of what \
                you did. List every file the verifier needs in `artifacts` — it starts with no \
                other context and cannot see this conversation. The result carries a \
                structured verdict (pass/fail/inconclusive) plus findings; only the verifier's \
                final message and verdict come back, and there is no follow-up conversation."
                .to_owned(),
            input_schema: schema_of::<VerifyInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Execution(
            "the Verify tool is executed by the engine loop; this build predates verifier \
             support"
                .to_owned(),
        ))
    }
}

/// Build the `Verify` tool descriptor.
pub fn verify_tool() -> Arc<dyn Tool> {
    Arc::new(VerifyTool)
}

/// The `SubmitVerdict` tool: parses a [`VerificationVerdict`] and returns it
/// as the call's structured result. No side effects beyond that — it exists
/// so a verifier's outcome is machine-readable, not just prose.
struct SubmitVerdictTool;

#[async_trait]
impl Tool for SubmitVerdictTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: SUBMIT_VERDICT_TOOL_NAME.to_owned(),
            description: "Report your verification outcome. Call this exactly once, after \
                you've checked the artifacts against the rubric — not before. `outcome` is \
                \"pass\" only if every rubric criterion holds; otherwise \"fail\", or \
                \"inconclusive\" if you genuinely cannot tell from the artifacts given. \
                `findings` must cite specifics (file, line, or exact behavior observed) — \
                vague findings like \"looks fine\" are not useful to the caller."
                .to_owned(),
            input_schema: schema_of::<VerificationVerdict>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let verdict: VerificationVerdict = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "SubmitVerdict input does not match its schema: {err}. Provide `outcome` \
                 (\"pass\" | \"fail\" | \"inconclusive\") and `findings` (an array of specific, \
                 citation-backed strings); `confidence` is optional."
            ))
        })?;
        let summary = if verdict.findings.is_empty() {
            format!("Verdict: {:?}", verdict.outcome)
        } else {
            format!(
                "Verdict: {:?}\n{}",
                verdict.outcome,
                verdict
                    .findings
                    .iter()
                    .map(|f| format!("- {f}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(summary)],
            is_error: false,
            structured: Some(serde_json::to_value(&verdict).unwrap_or_default()),
        })
    }
}

/// Build the `SubmitVerdict` tool.
pub fn submit_verdict_tool() -> Arc<dyn Tool> {
    Arc::new(SubmitVerdictTool)
}

#[cfg(test)]
mod tests {
    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::EventSink;
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn ctx() -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-test"),
            turn_id: TurnId::from("turn-test"),
            call_id: ToolCallId::from("call-test"),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        }
    }

    #[test]
    fn verify_descriptor_is_never_asked_and_is_read_only() {
        let descriptor = verify_tool().descriptor();
        assert_eq!(descriptor.name, VERIFIER_TOOL_NAME);
        assert!(descriptor.read_only);
        assert_eq!(descriptor.needs_permission, PermissionHint::Never);
    }

    #[tokio::test]
    async fn verify_run_is_never_reached_in_a_correct_build() {
        let err = verify_tool()
            .run(ctx(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[tokio::test]
    async fn submit_verdict_accepts_a_valid_verdict_and_carries_it_structured() {
        let input = serde_json::json!({
            "outcome": "pass",
            "findings": ["src/lib.rs:12 matches the rubric"],
            "confidence": 0.9,
        });
        let output = submit_verdict_tool().run(ctx(), input).await.unwrap();
        assert!(!output.is_error);
        let structured = output.structured.expect("structured verdict present");
        let verdict: VerificationVerdict = serde_json::from_value(structured).unwrap();
        assert_eq!(verdict.outcome, agentloop_contracts::VerdictOutcome::Pass);
        assert_eq!(verdict.findings.len(), 1);
    }

    #[tokio::test]
    async fn submit_verdict_teaches_on_invalid_input() {
        let err = submit_verdict_tool()
            .run(ctx(), serde_json::json!({"outcome": "maybe"}))
            .await
            .unwrap_err();
        let ToolError::InvalidInput(message) = err else {
            panic!("expected InvalidInput, got {err:?}");
        };
        assert!(message.contains("outcome"));
    }

    #[tokio::test]
    async fn submit_verdict_requires_findings_field() {
        let err = submit_verdict_tool()
            .run(ctx(), serde_json::json!({"outcome": "pass"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }
}
