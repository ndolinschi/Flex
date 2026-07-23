use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::tool::WORKFLOW_TOOL_NAME;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkflowStepInput {
    pub role: String,
    pub prompt: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WorkflowStepKind {
    Task(WorkflowStepInput),
    Parallel { tasks: Vec<WorkflowStepInput> },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunWorkflowInput {
    pub steps: Vec<WorkflowStepKind>,
}

struct WorkflowTool {
    description: String,
}

#[async_trait]
impl Tool for WorkflowTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: WORKFLOW_TOOL_NAME.to_owned(),
            description: self.description.clone(),
            input_schema: schema_of::<RunWorkflowInput>(),
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
            "the RunWorkflow tool is executed by the engine loop; this build predates \
             workflow support"
                .to_owned(),
        ))
    }
}

pub fn workflow_tool(roles: &[(String, String)]) -> Arc<dyn Tool> {
    let role_lines = if roles.is_empty() {
        "  (no roles configured)".to_owned()
    } else {
        roles
            .iter()
            .map(|(name, summary)| format!("- {name} — {summary}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "Run a multi-step plan of subagent tasks in one call, instead of calling `Agent` \
         once per turn. Each step is either a single task or a `parallel` fan-out of \
         several tasks that all run concurrently; steps run in order, and a `parallel` \
         step is a barrier — every task in it finishes before the next step starts.\n\n\
         Available roles:\n{role_lines}\n\n\
         Each task starts with NO context, same as `Agent` — it does not see this \
         conversation. It DOES see the combined results of every earlier step (appended \
         automatically before its own prompt), so a later step can build on what came \
         before; a task in the SAME `parallel` step cannot see its siblings' results \
         (they run concurrently). Write each `prompt` as a self-contained brief for what \
         that task alone must do.\n\n\
         Prefer `Agent` for a single task or a one-shot fan-out; reach for `RunWorkflow` \
         only when you already know the full multi-step shape up front and want it to run \
         without waiting for your own next turn between steps."
    );
    Arc::new(WorkflowTool { description })
}
