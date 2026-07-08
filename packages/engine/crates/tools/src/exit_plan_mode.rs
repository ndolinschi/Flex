//! `ExitPlanMode`: hand a finished implementation plan to the user for approval.
//!
//! Read-only signalling tool used during plan mode. The model calls it once it
//! has researched and written a plan; the client (CLI) intercepts the call,
//! shows the plan, and — on approval — switches the session out of plan mode so
//! the work can be carried out. The tool itself performs no mutation, so it is
//! allowed by the plan-mode permission gate.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExitPlanModeInput {
    /// The implementation plan to present, as concise Markdown (numbered steps).
    plan: String,
}

/// Signals that planning is complete and a plan is ready for approval.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExitPlanModeTool;

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "ExitPlanMode".to_owned(),
            description: "Use ONLY while in plan mode, once you have finished researching and \
                          written an implementation plan. Pass the plan as Markdown in `plan`; it \
                          is shown to the user for approval. Do not call this to answer questions \
                          or when not in plan mode. After calling it, stop and wait — do not start \
                          implementing until the user approves and leaves plan mode."
                .to_owned(),
            input_schema: schema_of::<ExitPlanModeInput>(),
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
        let input: ExitPlanModeInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `ExitPlanMode` must be {{\"plan\": \"<markdown plan>\"}}: {err}."
            ))
        })?;
        if input.plan.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`ExitPlanMode` needs a non-empty `plan`.".to_owned(),
            ));
        }
        Ok(ToolOutput::text(
            "Plan submitted for the user's approval. Stop here and wait — if the user approves, \
             you will be switched to code mode to carry it out.",
        ))
    }
}
