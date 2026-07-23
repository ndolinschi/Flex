use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, SessionMeta, ToolCallStatus, ToolOutput, TurnId, TurnOptions,
};
use agentloop_core::EventSink;
use agentloop_core::tool::{SUBAGENT_TOOL_NAME, ToolError, VERIFIER_TOOL_NAME, WORKFLOW_TOOL_NAME};

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;

use super::dispatch::dispatch_tool_call;
use super::intercept::{run_subagent_call, run_verify_call};
use super::permission::{PermissionGate, gate_permission};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_one_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    _opts: &TurnOptions,
    cancel: &CancellationToken,
    sink: &EventSink,
    manager: &Arc<Mutex<ToolCallManager>>,
    session_permits: &Arc<Semaphore>,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<std::collections::HashMap<String, usize>>>,
    request: DraftToolCall,
) {
    let emit_update = |call: agentloop_contracts::ToolCall| {
        let handle = handle.clone();
        let turn_id = turn_id.clone();
        async move {
            let _ = handle
                .emit_persistent(Some(&turn_id), AgentEvent::ToolCallUpdated { call })
                .await;
        }
    };
    let transition = |to: ToolCallStatus, result: Option<ToolOutput>| {
        manager
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .transition(&request.id, to, result)
            .ok()
    };

    if request.name == SUBAGENT_TOOL_NAME {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        let result = run_subagent_call(
            deps,
            handle,
            meta,
            turn_id,
            cancel,
            children_spawned,
            split_counters,
            &request,
        )
        .await;
        let final_call = match result {
            Ok(output) => transition(ToolCallStatus::Completed, Some(output)),
            Err(err @ (ToolError::InvalidInput(_) | ToolError::Execution(_))) => transition(
                ToolCallStatus::Completed,
                Some(ToolOutput::error(err.to_string())),
            ),
            Err(ToolError::Cancelled) => transition(ToolCallStatus::Cancelled, None),
            Err(err) => transition(
                ToolCallStatus::Failed {
                    error: err.to_string(),
                },
                None,
            ),
        };
        if let Some(call) = final_call {
            emit_update(call).await;
        }
        return;
    }

    if request.name == VERIFIER_TOOL_NAME {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        let result = run_verify_call(
            deps,
            handle,
            meta,
            turn_id,
            cancel,
            children_spawned,
            split_counters,
            &request,
        )
        .await;
        let final_call = match result {
            Ok(output) => transition(ToolCallStatus::Completed, Some(output)),
            Err(err @ (ToolError::InvalidInput(_) | ToolError::Execution(_))) => transition(
                ToolCallStatus::Completed,
                Some(ToolOutput::error(err.to_string())),
            ),
            Err(ToolError::Cancelled) => transition(ToolCallStatus::Cancelled, None),
            Err(err) => transition(
                ToolCallStatus::Failed {
                    error: err.to_string(),
                },
                None,
            ),
        };
        if let Some(call) = final_call {
            emit_update(call).await;
        }
        return;
    }

    if request.name == WORKFLOW_TOOL_NAME {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        let result = crate::workflow::run_workflow_call(
            deps,
            handle,
            meta,
            cancel,
            children_spawned,
            split_counters,
            &request.id,
            &request.input,
        )
        .await;
        let final_call = match result {
            Ok(output) => transition(ToolCallStatus::Completed, Some(output)),
            Err(err @ (ToolError::InvalidInput(_) | ToolError::Execution(_))) => transition(
                ToolCallStatus::Completed,
                Some(ToolOutput::error(err.to_string())),
            ),
            Err(ToolError::Cancelled) => transition(ToolCallStatus::Cancelled, None),
            Err(err) => transition(
                ToolCallStatus::Failed {
                    error: err.to_string(),
                },
                None,
            ),
        };
        if let Some(call) = final_call {
            emit_update(call).await;
        }
        return;
    }

    let Some(tool) = deps.tools.get(&request.name) else {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        if let Some(call) = transition(
            ToolCallStatus::Completed,
            Some(ToolOutput::error(format!(
                "Unknown tool `{}`. Available tools: {}.",
                request.name,
                deps.tools.names().join(", ")
            ))),
        ) {
            emit_update(call).await;
        }
        return;
    };
    let descriptor = tool.descriptor();

    if matches!(
        gate_permission(
            deps,
            handle,
            meta,
            turn_id,
            cancel,
            manager,
            &request,
            &descriptor,
        )
        .await,
        PermissionGate::Stop
    ) {
        return;
    }

    dispatch_tool_call(
        deps,
        handle,
        meta,
        turn_id,
        cancel,
        sink,
        manager,
        session_permits,
        &request,
        tool,
        request.input.clone(),
        &descriptor.name,
    )
    .await;
}
