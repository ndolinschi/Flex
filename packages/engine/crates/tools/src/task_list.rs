//! `TaskList`: update the agent-visible working plan.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, PlanEntry, ToolOutput};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskListInput {
    /// The full current plan; callers should include every visible item.
    entries: Vec<PlanEntry>,
}

/// Emits a canonical `PlanUpdated` event.
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskListTool;

#[async_trait]
impl Tool for TaskListTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "TaskList".to_owned(),
            description: "Update the working task list shown to clients. Pass the complete \
                          current `entries` array each time, with each item containing \
                          `content` and `status` (`pending`, `in_progress`, or `completed`). \
                          Use this for multi-step work; do not use it for trivial one-step \
                          answers."
                .to_owned(),
            input_schema: schema_of::<TaskListInput>(),
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
        let input: TaskListInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `TaskList` must be {{\"entries\": [{{\"content\": \"...\", \
                 \"status\": \"pending|in_progress|completed\"}}]}}: {err}."
            ))
        })?;
        if input
            .entries
            .iter()
            .any(|entry| entry.content.trim().is_empty())
        {
            return Err(ToolError::InvalidInput(
                "Every TaskList entry needs non-empty `content`.".to_owned(),
            ));
        }

        let count = input.entries.len();
        ctx.events.emit(AgentEvent::PlanUpdated {
            entries: input.entries,
        });
        Ok(ToolOutput::text(format!(
            "Updated task list with {count} entr{}.",
            if count == 1 { "y" } else { "ies" }
        )))
    }
}
