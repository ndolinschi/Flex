//! `Task`: delegate a scoped job to a subagent.
//!
//! This crate ships only the descriptor — execution is intercepted and run by
//! the engine loop (it needs to spawn a child session, which a pure tool
//! cannot). The description is written for the orchestrator model: it teaches
//! when to fan out searchers vs workers and how to write a self-contained
//! brief.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::tool::SUBAGENT_TOOL_NAME;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

// Schema-only: the loop parses Task input itself; this struct exists to
// generate the JSON schema shown to the model.
#[allow(dead_code)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskInput {
    /// Which role runs the task (see the tool description for the list).
    role: String,
    /// A 3-7 word label shown to the user, e.g. "map session event flow".
    description: String,
    /// The self-contained brief: everything the subagent needs, since it
    /// sees none of this conversation.
    prompt: String,
    /// Precisely what the subagent should return.
    #[serde(default)]
    expected_output: Option<String>,
}

/// The `Task` descriptor. `run` is never reached in a correct build — the
/// loop intercepts calls by name and runs a subagent instead.
struct TaskTool {
    description: String,
}

#[async_trait]
impl Tool for TaskTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: SUBAGENT_TOOL_NAME.to_owned(),
            description: self.description.clone(),
            input_schema: schema_of::<TaskInput>(),
            // Read-only so consecutive Task calls batch and run concurrently;
            // children gate their own mutations.
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
            "the Task tool is executed by the engine loop; this build predates \
             subagent support"
                .to_owned(),
        ))
    }
}

/// Build a `Task` tool whose description advertises the spawnable `roles`
/// as `(name, one-line summary)` pairs.
pub fn subagent_tool(roles: &[(String, String)]) -> Arc<dyn Tool> {
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
        "Delegate a well-scoped task to a subagent that runs in its own fresh session \
         with its own tools and model, and returns a single final report. Use subagents \
         to parallelize independent work and to keep your own context small: research \
         before you build, fan out searches, split independent implementation tasks.\n\n\
         Available roles:\n{role_lines}\n\n\
         The subagent starts with NO context: it does not see this conversation, your \
         plan, or other subagents' results. Write `prompt` as a self-contained brief — \
         state the goal and exact deliverable, and include every fact it needs that you \
         already know (absolute file paths, symbol names, decisions already made). Say \
         precisely what to return in `expected_output`; only the subagent's final \
         message comes back, and there is no follow-up conversation.\n\n\
         Choosing granularity: a question answerable by reading code or docs → \
         searcher(s) in parallel; implementation after the plan is clear → one worker \
         per INDEPENDENT task (tasks touching the same files are not independent). Do \
         not delegate trivial work — the handoff costs more than doing it yourself.\n\n\
         Parallelism: emit multiple Task calls in ONE message to run them concurrently \
         (they may be served by different models). You own integrating the results and \
         the correctness of the final answer."
    );
    Arc::new(TaskTool { description })
}
