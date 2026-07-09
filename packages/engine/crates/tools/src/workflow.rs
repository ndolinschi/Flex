//! `RunWorkflow`: a declarative pipeline of subagent steps.
//!
//! This crate ships only the descriptor — execution is intercepted and run by
//! the engine loop, same as `Agent` (each step is itself a subagent spawn, so
//! it needs the same session-creation machinery a pure tool cannot reach).
//! The plan is *data*, not executable code: no sandboxed script, no arbitrary
//! model-authored control flow — just an ordered list of steps, each either
//! one subagent task or a barrier fan-out of several.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::tool::WORKFLOW_TOOL_NAME;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

/// One subagent task within a workflow step.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkflowStepInput {
    /// Which role runs the task (same roles the `Agent` tool lists).
    pub role: String,
    /// The self-contained brief for this task; the loop appends every
    /// earlier step's combined results before this text, so it can build on
    /// them without repeating them here.
    pub prompt: String,
    /// A short label shown to the user, e.g. "map session event flow".
    #[serde(default)]
    pub label: Option<String>,
}

/// One step of a workflow: a single task, or a barrier fan-out of several
/// that all run concurrently before the next step starts.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WorkflowStepKind {
    Task(WorkflowStepInput),
    Parallel { tasks: Vec<WorkflowStepInput> },
}

/// The `RunWorkflow` tool's input — also the shape the loop deserializes a
/// dispatched call's raw JSON into (see `loop::workflow::run_workflow`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunWorkflowInput {
    /// Ordered pipeline steps; each completes before the next starts.
    pub steps: Vec<WorkflowStepKind>,
}

/// The `RunWorkflow` descriptor. `run` is never reached in a correct build —
/// the loop intercepts calls by name and runs the pipeline instead.
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

/// Build a `RunWorkflow` tool whose description advertises the spawnable
/// `roles` as `(name, one-line summary)` pairs (same shape `subagent_tool`
/// takes).
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
