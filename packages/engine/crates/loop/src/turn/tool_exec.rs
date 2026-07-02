//! Tool-call batching, permissions, hooks, and result feed-back.

use std::sync::{Arc, Mutex};

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, info_span};

use agentloop_contracts::{
    AgentEvent, ContentBlock, HookPoint, MessageId, PermissionDecision, PermissionDecisionKind,
    PermissionRequestId, SessionMeta, ToolCallOrigin, ToolCallStatus, ToolOutput, TurnId,
    TurnOptions,
};
use agentloop_core::tool::{ToolContext, ToolError};
use agentloop_core::{AgentError, EventSink};

use crate::agent::NativeAgent;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::permission::{PermissionPolicy, Verdict};
use crate::session_handle::SessionHandle;
use crate::tool_results::output_or_synthetic;

use super::hooks::run_hooks;

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_tool_requests(
    agent: &NativeAgent,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    opts: &TurnOptions,
    cancel: &CancellationToken,
    sink: &EventSink,
    manager: &mut ToolCallManager,
    message_id: &MessageId,
    tool_requests: &[DraftToolCall],
) -> Result<(), AgentError> {
    for request in tool_requests {
        let read_only = agent
            .tools
            .get(&request.name)
            .map(|tool| tool.descriptor().read_only)
            .unwrap_or(false);
        let call = manager.admit(
            request.id.clone(),
            handle.id.clone(),
            turn_id.clone(),
            message_id.clone(),
            request.name.clone(),
            request.input.clone(),
            read_only,
            ToolCallOrigin::Model,
        );
        handle
            .emit_persistent(Some(turn_id), AgentEvent::ToolCallUpdated { call })
            .await?;
    }

    // Batch consecutive read-only calls; run them concurrently (bounded).
    // The manager is shared behind a mutex — no await happens under the lock.
    let manager_shared = Arc::new(Mutex::new(std::mem::take(manager)));
    let mut index = 0;
    while index < tool_requests.len() {
        if cancel.is_cancelled() {
            break;
        }
        let is_read_only = |req: &DraftToolCall| {
            manager_shared
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&req.id)
                .map(|c| c.read_only)
                .unwrap_or(false)
        };
        if is_read_only(&tool_requests[index]) {
            let mut end = index + 1;
            while end < tool_requests.len() && is_read_only(&tool_requests[end]) {
                end += 1;
            }
            let batch = &tool_requests[index..end];
            futures::stream::iter(batch.iter().cloned())
                .map(|req| {
                    execute_one_call(
                        agent,
                        handle,
                        meta,
                        turn_id,
                        opts,
                        cancel,
                        sink,
                        &manager_shared,
                        req,
                    )
                })
                .buffer_unordered(agent.limits.tool_concurrency)
                .collect::<Vec<_>>()
                .await;
            index = end;
        } else {
            execute_one_call(
                agent,
                handle,
                meta,
                turn_id,
                opts,
                cancel,
                sink,
                &manager_shared,
                tool_requests[index].clone(),
            )
            .await;
            index += 1;
        }
    }
    *manager = Arc::try_unwrap(manager_shared)
        .map_err(|_| AgentError::Other("tool execution task leaked".to_owned()))?
        .into_inner()
        .unwrap_or_else(|p| p.into_inner());

    // Feed results back in request order.
    let result_blocks: Vec<ContentBlock> = tool_requests
        .iter()
        .filter_map(|req| {
            manager.get(&req.id).map(|call| {
                let (blocks, is_error) = output_or_synthetic(
                    call.result.as_ref(),
                    &call.status,
                    "The tool call did not complete.",
                    true,
                );
                ContentBlock::ToolResult {
                    tool_use_id: req.id.clone(),
                    content: blocks,
                    is_error,
                }
            })
        })
        .collect();

    handle
        .emit_persistent(
            Some(turn_id),
            AgentEvent::UserMessage {
                message_id: MessageId::generate(),
                content: result_blocks,
            },
        )
        .await?;

    Ok(())
}

/// Execute a single tool call through its full lifecycle.
#[allow(clippy::too_many_arguments)]
async fn execute_one_call(
    agent: &NativeAgent,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    opts: &TurnOptions,
    cancel: &CancellationToken,
    sink: &EventSink,
    manager: &Arc<Mutex<ToolCallManager>>,
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

    // Unknown tool: a model mistake — feed a teaching error result back.
    let Some(tool) = agent.tools.get(&request.name) else {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        if let Some(call) = transition(
            ToolCallStatus::Completed,
            Some(ToolOutput::error(format!(
                "Unknown tool `{}`. Available tools: {}.",
                request.name,
                agent.tools.names().join(", ")
            ))),
        ) {
            emit_update(call).await;
        }
        return;
    };
    let descriptor = tool.descriptor();

    // ── permission gate ─────────────────────────────────────────────────────
    let verdict =
        agent
            .policy
            .evaluate(&descriptor, &request.input, &meta.cwd, opts.permission_mode);
    match verdict {
        Verdict::Deny { reason } => {
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
        Verdict::Ask => {
            let request_id = PermissionRequestId::generate();
            if let Some(call) = transition(
                ToolCallStatus::AwaitingPermission {
                    request_id: request_id.clone(),
                },
                None,
            ) {
                emit_update(call).await;
            }
            let detail = serde_json::to_string_pretty(&request.input).ok().map(|s| {
                if s.len() > 2000 {
                    format!("{}…", &s[..2000])
                } else {
                    s
                }
            });
            let _ = handle
                .emit_persistent(
                    Some(turn_id),
                    AgentEvent::PermissionRequested {
                        id: request_id.clone(),
                        call_id: Some(request.id.clone()),
                        title: format!("Allow `{}`?", descriptor.name),
                        detail,
                        options: vec![
                            PermissionDecisionKind::AllowOnce,
                            PermissionDecisionKind::AllowAlways,
                            PermissionDecisionKind::Deny,
                        ],
                    },
                )
                .await;

            let wait = agent
                .pending_permissions
                .wait(request_id.clone(), agent.policy.ask_timeout);
            let decision = tokio::select! {
                decision = wait => decision,
                _ = cancel.cancelled() => None,
            }
            .unwrap_or(PermissionDecision::Deny {
                reason: Some("permission request timed out or was interrupted".to_owned()),
            });

            let _ = handle
                .emit_persistent(
                    Some(turn_id),
                    AgentEvent::PermissionResolved {
                        id: request_id,
                        decision: decision.clone(),
                    },
                )
                .await;

            match decision {
                PermissionDecision::Deny { reason } => {
                    if let Some(call) = transition(ToolCallStatus::Denied { reason }, None) {
                        emit_update(call).await;
                    }
                    return;
                }
                PermissionDecision::AllowAlways => {
                    agent.policy.add_rule(PermissionPolicy::rule_for_always(
                        &descriptor,
                        &request.input,
                    ));
                }
                PermissionDecision::AllowOnce => {}
                // Unknown future decision kinds fail closed.
                _ => {
                    if let Some(call) = transition(
                        ToolCallStatus::Denied {
                            reason: Some("unrecognized permission decision".to_owned()),
                        },
                        None,
                    ) {
                        emit_update(call).await;
                    }
                    return;
                }
            }
        }
        Verdict::Allow => {}
    }

    // ── pre-execution hook (may rewrite input or block) ─────────────────────
    let mut input = request.input.clone();
    {
        let call_snapshot = manager
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&request.id)
            .cloned();
        if let Some(mut call) = call_snapshot {
            call.input = input.clone();
            let outcome = run_hooks(
                agent,
                handle,
                HookPoint::PreToolUse,
                turn_id,
                agentloop_core::HookData::ToolUse { call: &mut call },
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

    // ── run ─────────────────────────────────────────────────────────────────
    if let Some(call) = transition(ToolCallStatus::Running, None) {
        emit_update(call).await;
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
    let span = info_span!("tool_call", tool = %descriptor.name, call_id = %request.id);
    let result = tokio::select! {
        result = tokio::time::timeout(agent.limits.tool_timeout, tool.run(ctx, input))
            .instrument(span) => match result {
                Ok(inner) => inner,
                Err(_) => Err(ToolError::Timeout(agent.limits.tool_timeout.as_millis() as u64)),
            },
        _ = call_token.cancelled() => Err(ToolError::Cancelled),
    };

    let final_call = match result {
        Ok(mut output) => {
            // Post-execution hook may rewrite the output.
            let call_snapshot = manager
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&request.id)
                .cloned();
            if let Some(call) = call_snapshot {
                let _ = run_hooks(
                    agent,
                    handle,
                    HookPoint::PostToolUse,
                    turn_id,
                    agentloop_core::HookData::ToolResult {
                        call: &call,
                        output: &mut output,
                    },
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
        // Tool-level errors teach the model and never fail the loop.
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
