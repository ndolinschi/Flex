//! Independent verifier: an opt-in plugin providing `Verify` + `SubmitVerdict`
//! tools — "maker is never the grader".
//!
//! `Verify` spawns a fresh `verifier`-role subagent seeded with only a
//! rubric and a list of artifact paths (the loop intercepts the call by
//! name; see `agentloop_loop`). `SubmitVerdict` is what that subagent calls
//! to report a structured [`agentloop_contracts::VerificationVerdict`].
//!
//! Zero footprint when disabled: enable via
//! `AgentBuilder::enable_plugin("verifier")` (SDK feature `verifier`).

mod tools;

use std::sync::Arc;

use agentloop_core::{Plugin, Tool};

pub use tools::{submit_verdict_tool, verify_tool};

/// The independent-verifier plugin. Enabled via the composition root
/// (`AgentBuilder::enable_plugin("verifier")`); zero footprint when off.
#[derive(Debug, Default)]
pub struct VerifierPlugin;

impl Plugin for VerifierPlugin {
    fn id(&self) -> &'static str {
        "verifier"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![verify_tool(), submit_verdict_tool()]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "# Independent verification\n\
             Before declaring non-trivial work done, run `Verify` against a rubric of what \
             must be true — it spawns a fresh subagent that checks only the artifacts you \
             list, with no access to your reasoning. Prefer this over self-review for \
             anything you're about to hand off or commit to memory."
                .to_owned(),
        )
    }
}
