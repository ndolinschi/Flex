//! Loop-intercepted tool calls: Task (subagent) and Verify.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{SessionMeta, ToolOutput, TurnId};
use agentloop_core::tool::{SUBAGENT_TOOL_NAME, ToolError, VERIFIER_TOOL_NAME};

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::roles::VERIFIER_ROLE;
use crate::session_handle::SessionHandle;
use crate::subagent::SubagentRequest;

use super::batch::MAX_CHILDREN_PER_TURN;

/// Parse a Task call, enforce the per-turn budget, and run the subagent.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_subagent_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    _turn_id: &TurnId,
    cancel: &CancellationToken,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<std::collections::HashMap<String, usize>>>,
    request: &DraftToolCall,
) -> Result<ToolOutput, ToolError> {
    if let Some(role) = meta.role.as_deref() {
        let filter = deps.roles.tool_filter(role, &deps.tools, meta.depth);
        if !filter.permits(SUBAGENT_TOOL_NAME) {
            return Err(ToolError::InvalidInput(format!(
                "role `{role}` may not spawn further subagents at depth {} \
                 (max_depth reached) — finish this work yourself instead of delegating further.",
                meta.depth
            )));
        }
    }

    let required = |field: &str| -> Result<String, ToolError> {
        request
            .input
            .get(field)
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                ToolError::InvalidInput(format!(
                    "Task requires string fields `role`, `description`, `prompt`; \
                     `{field}` is missing or empty."
                ))
            })
    };
    let role = required("role")?;
    let description = required("description")?;
    let prompt = required("prompt")?;
    let expected_output = request
        .input
        .get("expected_output")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned);
    let model_override = request
        .input
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(agentloop_contracts::ModelRef::from);

    if children_spawned.fetch_add(1, Ordering::SeqCst) >= MAX_CHILDREN_PER_TURN {
        return Err(ToolError::Execution(format!(
            "subagent budget of {MAX_CHILDREN_PER_TURN} per turn reached — consolidate \
             remaining work into fewer, larger briefs or finish it yourself."
        )));
    }

    let agent = deps.agent.upgrade().ok_or_else(|| {
        ToolError::Execution("the agent is shutting down; cannot spawn subagents".to_owned())
    })?;

    let assigned_model = model_override.or_else(|| {
        deps.roles.get(&role).and_then(|spec| {
            if !spec.split || spec.models.len() < 2 {
                return None;
            }
            let mut counters = split_counters.lock().unwrap_or_else(|p| p.into_inner());
            let counter = counters.entry(role.clone()).or_insert(0);
            let model = spec.models[*counter % spec.models.len()].clone();
            *counter += 1;
            Some(model)
        })
    });

    let sub = SubagentRequest {
        call_id: request.id.clone(),
        role,
        description,
        prompt,
        expected_output,
        assigned_model,
        permission_mode: handle.turn_permission_mode(),
        effort: handle.turn_effort(),
        cancel: cancel.child_token(),
    };
    agent.run_subagent(&handle.id, sub).await
}

/// Parse a Verify call and run it as a `verifier`-role subagent whose brief
/// is built programmatically from `rubric` + `artifacts` — never from the
/// caller's own reasoning, since the input schema has no field for that
/// ("maker is never the grader").
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_verify_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    _turn_id: &TurnId,
    cancel: &CancellationToken,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<std::collections::HashMap<String, usize>>>,
    request: &DraftToolCall,
) -> Result<ToolOutput, ToolError> {
    if let Some(role) = meta.role.as_deref() {
        let filter = deps.roles.tool_filter(role, &deps.tools, meta.depth);
        if !filter.permits(VERIFIER_TOOL_NAME) {
            return Err(ToolError::InvalidInput(format!(
                "role `{role}` may not run a verifier at depth {} (max_depth reached) — \
                 finish this work yourself instead of delegating further.",
                meta.depth
            )));
        }
    }

    let rubric = request
        .input
        .get("rubric")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ToolError::InvalidInput("Verify requires a non-empty string field `rubric`.".to_owned())
        })?
        .to_owned();
    let artifacts: Vec<String> = request
        .input
        .get("artifacts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ToolError::InvalidInput(
                "Verify requires `artifacts`: an array of paths (relative to the working \
                 directory) the verifier should read."
                    .to_owned(),
            )
        })?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if artifacts.is_empty() {
        return Err(ToolError::InvalidInput(
            "Verify requires at least one entry in `artifacts` — a verifier with nothing to \
             read cannot form a verdict."
                .to_owned(),
        ));
    }
    let model_override = request
        .input
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(agentloop_contracts::ModelRef::from);

    if children_spawned.fetch_add(1, Ordering::SeqCst) >= MAX_CHILDREN_PER_TURN {
        return Err(ToolError::Execution(format!(
            "subagent budget of {MAX_CHILDREN_PER_TURN} per turn reached — consolidate \
             remaining verification into fewer calls or finish it yourself."
        )));
    }

    let agent = deps.agent.upgrade().ok_or_else(|| {
        ToolError::Execution("the agent is shutting down; cannot spawn a verifier".to_owned())
    })?;

    let assigned_model = model_override.or_else(|| {
        deps.roles.get(VERIFIER_ROLE).and_then(|spec| {
            if !spec.split || spec.models.len() < 2 {
                return None;
            }
            let mut counters = split_counters.lock().unwrap_or_else(|p| p.into_inner());
            let counter = counters.entry(VERIFIER_ROLE.to_owned()).or_insert(0);
            let model = spec.models[*counter % spec.models.len()].clone();
            *counter += 1;
            Some(model)
        })
    });

    let artifact_list = artifacts
        .iter()
        .map(|path| format!("- {path}"))
        .collect::<Vec<_>>()
        .join("\n");
    let brief = format!(
        "Rubric — what must be true for this to pass:\n{rubric}\n\n\
         Artifacts to inspect (read-only; this is the only context you have):\n{artifact_list}"
    );

    let sub = SubagentRequest {
        call_id: request.id.clone(),
        role: VERIFIER_ROLE.to_owned(),
        description: "verify artifacts against a rubric".to_owned(),
        prompt: brief,
        expected_output: None,
        assigned_model,
        permission_mode: handle.turn_permission_mode(),
        effort: handle.turn_effort(),
        cancel: cancel.child_token(),
    };
    agent.run_subagent(&handle.id, sub).await
}
