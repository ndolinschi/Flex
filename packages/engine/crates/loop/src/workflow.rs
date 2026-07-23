use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::join_all;
use tokio_util::sync::CancellationToken;

use serde::Deserialize;

use agentloop_contracts::{SessionMeta, ToolCallId, ToolOutput};
use agentloop_core::tool::{ToolError, WORKFLOW_TOOL_NAME};

use crate::agent::NativeAgent;
use crate::deps::TurnDeps;
use crate::session_handle::SessionHandle;
use crate::subagent::SubagentRequest;
use crate::turn::tool_exec::MAX_CHILDREN_PER_TURN;

const MAX_PARALLEL_WORKFLOW_TASKS: usize = 8;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunWorkflowInput {
    steps: Vec<WorkflowStepKind>,
}

#[derive(Debug, Deserialize)]
struct WorkflowStepInput {
    role: String,
    prompt: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum WorkflowStepKind {
    Task(WorkflowStepInput),
    Parallel { tasks: Vec<WorkflowStepInput> },
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_workflow_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    cancel: &CancellationToken,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<HashMap<String, usize>>>,
    call_id: &ToolCallId,
    input: &serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    if let Some(role) = meta.role.as_deref() {
        let filter = deps.roles.tool_filter(role, &deps.tools, meta.depth);
        if !filter.permits(WORKFLOW_TOOL_NAME) {
            return Err(ToolError::InvalidInput(format!(
                "role `{role}` may not run a workflow at depth {} (max_depth reached) — \
                 finish this work yourself instead of delegating further.",
                meta.depth
            )));
        }
    }

    let parsed: RunWorkflowInput = serde_json::from_value(input.clone()).map_err(|err| {
        ToolError::InvalidInput(format!(
            "RunWorkflow input does not match the expected shape: {err}"
        ))
    })?;
    if parsed.steps.is_empty() {
        return Err(ToolError::InvalidInput(
            "RunWorkflow requires at least one step in `steps`.".to_owned(),
        ));
    }

    let agent = deps.agent.upgrade().ok_or_else(|| {
        ToolError::Execution("the agent is shutting down; cannot run a workflow".to_owned())
    })?;

    let mut context = String::new();
    let mut rendered_steps = Vec::with_capacity(parsed.steps.len());

    for (index, step) in parsed.steps.into_iter().enumerate() {
        let step_num = index + 1;
        match step {
            WorkflowStepKind::Task(task) => {
                let label = task.label.clone().unwrap_or_else(|| task.role.clone());
                let output = spawn_one(
                    &agent,
                    deps,
                    handle,
                    cancel,
                    children_spawned,
                    split_counters,
                    call_id,
                    &task,
                    &context,
                )
                .await?;
                let text = output.render_text();
                context.push_str(&format!("\n\n=== step {step_num} ({label}) ===\n{text}"));
                rendered_steps.push(format!("## Step {step_num}: {label}\n{text}"));
            }
            WorkflowStepKind::Parallel { tasks } => {
                if tasks.is_empty() {
                    return Err(ToolError::InvalidInput(format!(
                        "step {step_num} is a `parallel` step with no `tasks`."
                    )));
                }
                let dropped = tasks.len().saturating_sub(MAX_PARALLEL_WORKFLOW_TASKS);
                let tasks: Vec<WorkflowStepInput> = tasks
                    .into_iter()
                    .take(MAX_PARALLEL_WORKFLOW_TASKS)
                    .collect();
                let snapshot = context.clone();
                let futures = tasks.iter().map(|task| {
                    spawn_one(
                        &agent,
                        deps,
                        handle,
                        cancel,
                        children_spawned,
                        split_counters,
                        call_id,
                        task,
                        &snapshot,
                    )
                });
                let outcomes = join_all(futures).await;

                let mut section = format!("=== step {step_num} (parallel) ===");
                if dropped > 0 {
                    section.push_str(&format!(
                        "\n[{dropped} task(s) beyond the {MAX_PARALLEL_WORKFLOW_TASKS}-per-step \
                         limit were dropped, not run]"
                    ));
                }
                let mut rendered = format!("## Step {step_num} (parallel)");
                if dropped > 0 {
                    rendered.push_str(&format!(
                        "\n_{dropped} task(s) beyond the {MAX_PARALLEL_WORKFLOW_TASKS}-per-step \
                         limit were dropped, not run._"
                    ));
                }
                for (task, outcome) in tasks.iter().zip(outcomes) {
                    let output = outcome?;
                    let label = task.label.clone().unwrap_or_else(|| task.role.clone());
                    let text = output.render_text();
                    section.push_str(&format!("\n--- {label} ---\n{text}"));
                    rendered.push_str(&format!("\n### {label}\n{text}"));
                }
                context.push_str(&format!("\n\n{section}"));
                rendered_steps.push(rendered);
            }
        }
    }

    Ok(ToolOutput::text(rendered_steps.join("\n\n")))
}

#[allow(clippy::too_many_arguments)]
async fn spawn_one(
    agent: &Arc<NativeAgent>,
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    cancel: &CancellationToken,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<HashMap<String, usize>>>,
    call_id: &ToolCallId,
    task: &WorkflowStepInput,
    context_so_far: &str,
) -> Result<ToolOutput, ToolError> {
    if children_spawned.fetch_add(1, Ordering::SeqCst) >= MAX_CHILDREN_PER_TURN {
        return Err(ToolError::Execution(format!(
            "subagent budget of {MAX_CHILDREN_PER_TURN} per turn reached — consolidate the \
             remaining workflow steps into fewer, larger tasks or finish them yourself."
        )));
    }

    let assigned_model = deps.roles.get(&task.role).and_then(|spec| {
        if !spec.split || spec.models.len() < 2 {
            return None;
        }
        let mut counters = split_counters.lock().unwrap_or_else(|p| p.into_inner());
        let counter = counters.entry(task.role.clone()).or_insert(0);
        let model = spec.models[*counter % spec.models.len()].clone();
        *counter += 1;
        Some(model)
    });

    let prompt = if context_so_far.is_empty() {
        task.prompt.clone()
    } else {
        format!(
            "Results from earlier workflow steps (for context only — this is not your \
             own conversation):\n{context_so_far}\n\n=== your task ===\n{}",
            task.prompt
        )
    };

    let sub = SubagentRequest {
        call_id: call_id.clone(),
        role: task.role.clone(),
        description: task
            .label
            .clone()
            .unwrap_or_else(|| format!("workflow step ({})", task.role)),
        prompt,
        expected_output: None,
        assigned_model,
        permission_mode: handle.turn_permission_mode(),
        effort: handle.turn_effort(),
        cancel: cancel.child_token(),
    };
    agent.run_subagent(&handle.id, sub).await
}
