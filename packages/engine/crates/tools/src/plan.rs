use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, PlanEntry, ToolOutput};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlanInput {
    entries: Vec<PlanEntry>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PlanTool;

#[async_trait]
impl Tool for PlanTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Plan".to_owned(),
            description: "Update the working plan shown to clients. Pass the complete \
                          current `entries` array each time (never empty), with each \
                          item containing `content` and `status` (`pending`, \
                          `in_progress`, or `completed`). Use this for multi-step \
                          work; do not use it for trivial one-step answers."
                .to_owned(),
            input_schema: schema_of::<PlanInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: PlanInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Plan` must be {{\"entries\": [{{\"content\": \"...\", \
                 \"status\": \"pending|in_progress|completed\"}}]}}: {err}."
            ))
        })?;
        if input
            .entries
            .iter()
            .any(|entry| entry.content.trim().is_empty())
        {
            return Err(ToolError::InvalidInput(
                "Every Plan entry needs non-empty `content`.".to_owned(),
            ));
        }
        if input.entries.is_empty() {
            return Err(ToolError::InvalidInput(
                "Plan `entries` must not be empty — pass the full current \
                 checklist (or keep the previous entries). An empty array \
                 would wipe the plan shown to the user."
                    .to_owned(),
            ));
        }

        let count = input.entries.len();
        ctx.events.emit(AgentEvent::PlanUpdated {
            entries: input.entries,
        });
        Ok(ToolOutput::text(format!(
            "Updated plan with {count} entr{}.",
            if count == 1 { "y" } else { "ies" }
        )))
    }
}
