//! Tool-call batching, permissions, hooks, and result feed-back.

use std::sync::{Arc, Mutex};

use futures::StreamExt;
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, ContentBlock, HookPoint, MessageId, PermissionDecision, PermissionDecisionKind,
    PermissionRequestId, SessionMeta, ToolCallOrigin, ToolCallStatus, ToolOutput, TurnId,
    TurnOptions,
};
use agentloop_core::tool::{ToolContext, ToolError};
use agentloop_core::{AgentError, EventSink};

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::permission::{PermissionPolicy, Verdict};
use crate::pool::{ToolEvent, ToolJob, ToolJobOutcome};
use crate::session_handle::SessionHandle;
use crate::subagent::SubagentRequest;
use crate::tool_results::output_or_synthetic;
use agentloop_core::tool::SUBAGENT_TOOL_NAME;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::hooks::run_hooks;

/// Hard cap on subagents spawned in one turn — a runaway-spawn backstop.
const MAX_CHILDREN_PER_TURN: usize = 8;

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_tool_requests(
    deps: &Arc<TurnDeps>,
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
        let read_only = deps
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
    // One turn per session (turn gate) makes this per-turn semaphore the
    // session-level execution bound the pool expects.
    let session_permits = Arc::new(Semaphore::new(deps.limits.tool_concurrency));
    let children_spawned = Arc::new(AtomicUsize::new(0));
    // Per-role spawn counters for split mode: parallel Task calls of the
    // same role in this message round-robin across the role's model chain.
    let split_counters = Arc::new(Mutex::new(std::collections::HashMap::<String, usize>::new()));
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
                        deps,
                        handle,
                        meta,
                        turn_id,
                        opts,
                        cancel,
                        sink,
                        &manager_shared,
                        &session_permits,
                        &children_spawned,
                        &split_counters,
                        req,
                    )
                })
                .buffer_unordered(deps.limits.tool_concurrency)
                .collect::<Vec<_>>()
                .await;
            index = end;
        } else {
            execute_one_call(
                deps,
                handle,
                meta,
                turn_id,
                opts,
                cancel,
                sink,
                &manager_shared,
                &session_permits,
                &children_spawned,
                &split_counters,
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

    // Subagent spawn: intercepted and run by the loop, not the pool.
    if request.name == SUBAGENT_TOOL_NAME {
        if let Some(call) = transition(ToolCallStatus::Running, None) {
            emit_update(call).await;
        }
        let result = run_subagent_call(
            deps,
            handle,
            turn_id,
            cancel,
            children_spawned,
            split_counters,
            &request,
        )
        .await;
        let final_call = match result {
            // Tool-level errors (bad role, child failed) teach the model and
            // never fail the turn; hard failures mark the call failed.
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

    // Unknown tool: a model mistake — feed a teaching error result back.
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

    // ── permission gate ─────────────────────────────────────────────────────
    let verdict = deps.policy.evaluate(
        &descriptor,
        &request.input,
        &meta.cwd,
        handle.turn_permission_mode(),
    );
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

            let wait = deps
                .pending_permissions
                .wait(request_id.clone(), deps.policy.ask_timeout);
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
                    deps.policy.add_rule(PermissionPolicy::rule_for_always(
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
                deps,
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

    // ── run on the worker pool: real parallelism + panic isolation ─────────
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
        tool: tool.clone(),
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
                // Permits acquired; the tool is actually executing now.
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
            // A report for another call can only mean a wiring bug; ignore.
            ToolEvent::Started { .. } | ToolEvent::Finished { .. } => {}
        }
    }
    let result = match outcome {
        Some(ToolJobOutcome::Output(result)) => result,
        Some(ToolJobOutcome::Panicked { message }) => {
            // The call fails; the turn (and the session) survive.
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
        // Channel closed without a Finished report: torn down mid-flight.
        None => Err(ToolError::Cancelled),
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
                    deps,
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

/// Parse a Task call, enforce the per-turn budget, and run the subagent.
#[allow(clippy::too_many_arguments)]
async fn run_subagent_call(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    _turn_id: &TurnId,
    cancel: &CancellationToken,
    children_spawned: &Arc<AtomicUsize>,
    split_counters: &Arc<Mutex<std::collections::HashMap<String, usize>>>,
    request: &DraftToolCall,
) -> Result<ToolOutput, ToolError> {
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

    if children_spawned.fetch_add(1, Ordering::SeqCst) >= MAX_CHILDREN_PER_TURN {
        return Err(ToolError::Execution(format!(
            "subagent budget of {MAX_CHILDREN_PER_TURN} per turn reached — consolidate \
             remaining work into fewer, larger briefs or finish it yourself."
        )));
    }

    let agent = deps.agent.upgrade().ok_or_else(|| {
        ToolError::Execution("the agent is shutting down; cannot spawn subagents".to_owned())
    })?;

    // Split mode: rotate parallel spawns of the same role across its model
    // chain (deterministic chain-order assignment per batch). The child puts
    // the assigned model first and keeps the rest of the chain for failover.
    // Unknown roles get None and hit the teaching path in run_subagent.
    let assigned_model = deps.roles.get(&role).and_then(|spec| {
        if !spec.split || spec.models.len() < 2 {
            return None;
        }
        let mut counters = split_counters.lock().unwrap_or_else(|p| p.into_inner());
        let counter = counters.entry(role.clone()).or_insert(0);
        let model = spec.models[*counter % spec.models.len()].clone();
        *counter += 1;
        Some(model)
    });

    let sub = SubagentRequest {
        call_id: request.id.clone(),
        role,
        description,
        prompt,
        expected_output,
        assigned_model,
        permission_mode: handle.turn_permission_mode(),
        cancel: cancel.child_token(),
    };
    agent.run_subagent(&handle.id, sub).await
}
