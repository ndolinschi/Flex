//! Tool-call batching across a turn.

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, SessionMeta, ToolCallOrigin, TurnId, TurnOptions,
};
use agentloop_core::{AgentError, EventSink};

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;
use crate::tool_results::output_or_synthetic;

use super::one_call::execute_one_call;

/// Hard cap on subagents spawned in one turn — a runaway-spawn backstop.
pub(crate) const MAX_CHILDREN_PER_TURN: usize = 8;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_tool_requests(
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

    let session_permits = Arc::new(Semaphore::new(deps.limits.tool_concurrency));
    let children_spawned = Arc::new(AtomicUsize::new(0));
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
