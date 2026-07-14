//! Pre/post hooks and worker-pool execution for a regular tool call.

use std::sync::{Arc, Mutex};

use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{AgentEvent, HookPoint, SessionMeta, ToolCallStatus, ToolOutput, TurnId};
use agentloop_core::EventSink;
use agentloop_core::tool::{ToolContext, ToolError};

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::pool::{ToolEvent, ToolJob, ToolJobOutcome};
use crate::session_handle::SessionHandle;

use super::super::hooks::run_hooks;

#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_tool_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    cancel: &CancellationToken,
    sink: &EventSink,
    manager: &Arc<Mutex<ToolCallManager>>,
    session_permits: &Arc<Semaphore>,
    request: &DraftToolCall,
    tool: Arc<dyn agentloop_core::Tool>,
    mut input: serde_json::Value,
    descriptor_name: &str,
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

    {
        let call_snapshot = manager
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&request.id)
            .cloned();
        if let Some(mut call) = call_snapshot {
            call.input = input.clone();
            let outcome = run_hooks(
                deps,
                handle,
                HookPoint::PreToolUse,
                turn_id,
                agentloop_core::HookData::ToolUse { call: &mut call },
                Some(sink.clone()),
            )
            .await
            .unwrap_or(agentloop_core::HookOutcome::Continue);
            match outcome {
                agentloop_core::HookOutcome::Block { reason } => {
                    if let Some(call) = transition(
                        ToolCallStatus::Denied {
                            reason: Some(reason),
                        },
                        None,
                    ) {
                        emit_update(call).await;
                    }
                    return;
                }
                agentloop_core::HookOutcome::Mutated => {
                    input = call.input.clone();
                    if let Some(stored) = manager
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get_mut(&request.id)
                    {
                        stored.input = input.clone();
                    }
                }
                agentloop_core::HookOutcome::Continue => {}
            }
        }
    }

    let call_token = cancel.child_token();
    let ctx = ToolContext {
        session_id: handle.id.clone(),
        turn_id: turn_id.clone(),
        call_id: request.id.clone(),
        cwd: meta.cwd.clone(),
        cancel: call_token.clone(),
        events: sink.clone(),
    };
    let job = ToolJob {
        call_id: request.id.clone(),
        tool,
        ctx,
        input,
        timeout: deps.limits.tool_timeout,
    };
    let (results_tx, mut results_rx) = mpsc::channel(2);
    let _abort = deps.pool.submit(job, session_permits.clone(), results_tx);
    let mut outcome = None;
    while let Some(event) = results_rx.recv().await {
        match event {
            ToolEvent::Started { call_id } if call_id == request.id => {
                if let Some(call) = transition(ToolCallStatus::Running, None) {
                    emit_update(call).await;
                }
            }
            ToolEvent::Finished {
                call_id,
                outcome: done,
            } if call_id == request.id => {
                outcome = Some(done);
                break;
            }
            ToolEvent::Started { .. } | ToolEvent::Finished { .. } => {}
        }
    }
    let result = match outcome {
        Some(ToolJobOutcome::Output(result)) => result,
        Some(ToolJobOutcome::Panicked { message }) => {
            tracing::error!(
                target: "tool",
                tool = %descriptor_name,
                call_id = %request.id,
                message = %message,
                "tool panicked"
            );
            if let Some(call) = transition(
                ToolCallStatus::Failed {
                    error: format!("tool panicked: {message}"),
                },
                None,
            ) {
                emit_update(call).await;
            }
            return;
        }
        None => Err(ToolError::Cancelled),
    };

    let final_call = match result {
        Ok(mut output) => {
            let call_snapshot = manager
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&request.id)
                .cloned();
            if let Some(call) = call_snapshot {
                let _ = run_hooks(
                    deps,
                    handle,
                    HookPoint::PostToolUse,
                    turn_id,
                    agentloop_core::HookData::ToolResult {
                        call: &call,
                        output: &mut output,
                    },
                    Some(sink.clone()),
                )
                .await;
            }
            transition(ToolCallStatus::Completed, Some(output))
        }
        Err(ToolError::Cancelled) => transition(ToolCallStatus::Cancelled, None),
        Err(ToolError::Timeout(ms)) => transition(
            ToolCallStatus::Failed {
                error: format!("timed out after {ms}ms"),
            },
            None,
        ),
        Err(err @ (ToolError::InvalidInput(_) | ToolError::Execution(_))) => transition(
            ToolCallStatus::Completed,
            Some(ToolOutput::error(err.to_string())),
        ),
        Err(other) => transition(
            ToolCallStatus::Failed {
                error: other.to_string(),
            },
            None,
        ),
    };
    if let Some(call) = final_call {
        emit_update(call).await;
    }
}
