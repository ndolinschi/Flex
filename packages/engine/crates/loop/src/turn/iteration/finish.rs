//! Materialize the assistant message and run tools / stop hooks.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, SessionMeta, StopReason, TokenUsage, TurnId, TurnOptions,
    TurnStopReason, now_ms,
};
use agentloop_core::hook::{HookData, HookOutcome};
use agentloop_core::{AgentError, EventSink};

use crate::deps::TurnDeps;
use crate::draft::AssistantDraft;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;

use super::super::IterationOutcome;
use super::super::hooks::run_hooks;
use crate::turn::tool_exec::execute_tool_requests;

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish_iteration(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    opts: &TurnOptions,
    cancel: &CancellationToken,
    sink: &EventSink,
    manager: &mut ToolCallManager,
    usage_total: &mut TokenUsage,
    num_model_calls: &mut u32,
    num_tool_calls: &mut u32,
    last_model: &mut Option<String>,
    draft: AssistantDraft,
    was_cancelled: bool,
    llm_started: u64,
    llm_span: tracing::Span,
) -> Result<IterationOutcome, AgentError> {
    *num_model_calls += 1;
    if let Some(usage) = draft.usage {
        usage_total.add(&usage);
    }
    tracing::info!(
        parent: &llm_span,
        latency_ms = now_ms().saturating_sub(llm_started),
        tokens_in = draft.usage.map(|u| u.input).unwrap_or(0),
        tokens_out = draft.usage.map(|u| u.output).unwrap_or(0),
        "model call finished"
    );

    let message_id = draft.message_id.clone();
    let model_name = draft.model.clone();
    // Attribution note: a turn's `usage_total` is a single running sum across
    // every model call in the turn, not split per-model (see `TurnDeps` /
    // `usage_total` above). When a turn fails over between models, cost is
    // approximated by pricing the *whole* accumulated usage at the last
    // (most current) model's rate rather than splitting per call.
    if let Some(name) = &model_name {
        *last_model = Some(name.clone());
    }
    let usage = draft.usage;
    let stop = draft.stop_reason;
    let (content, tool_requests) = draft.finish();

    if !content.is_empty() {
        handle
            .emit_persistent(
                Some(turn_id),
                AgentEvent::AssistantMessage {
                    message_id: message_id.clone(),
                    content,
                    model: model_name,
                    usage,
                },
            )
            .await?;
    }

    if was_cancelled {
        return Ok(IterationOutcome::Stop(TurnStopReason::Cancelled));
    }

    match stop {
        Some(StopReason::MaxTokens) => {
            return Ok(IterationOutcome::Stop(TurnStopReason::MaxTokens));
        }
        Some(StopReason::Refusal) => {
            return Ok(IterationOutcome::Stop(TurnStopReason::Refusal));
        }
        Some(StopReason::Cancelled) => {
            return Ok(IterationOutcome::Stop(TurnStopReason::Cancelled));
        }
        _ => {}
    }

    if tool_requests.is_empty() {
        let mut continuation: Option<String> = None;
        let outcome = run_hooks(
            deps,
            handle,
            agentloop_contracts::HookPoint::Stop,
            turn_id,
            HookData::Stop {
                continuation: &mut continuation,
            },
            Some(sink.clone()),
        )
        .await?;
        if !matches!(outcome, HookOutcome::Block { .. }) {
            if let Some(text) = continuation {
                handle
                    .emit_persistent(
                        Some(turn_id),
                        AgentEvent::UserMessage {
                            message_id: MessageId::generate(),
                            content: vec![ContentBlock::markdown(text)],
                        },
                    )
                    .await?;
                return Ok(IterationOutcome::Continue);
            }
        }
        return Ok(IterationOutcome::Stop(TurnStopReason::EndTurn));
    }

    *num_tool_calls += tool_requests.len() as u32;
    execute_tool_requests(
        deps,
        handle,
        meta,
        turn_id,
        opts,
        cancel,
        sink,
        manager,
        &message_id,
        &tool_requests,
    )
    .await?;

    if cancel.is_cancelled() {
        return Ok(IterationOutcome::Stop(TurnStopReason::Cancelled));
    }

    // In Plan permission mode, a *successful* `ExitPlanMode` call is a hard
    // turn-ending interrupt: the plan is already ready for the user's
    // approval (surfaced via the `ToolCallUpdated` event emitted above), so
    // there is nothing left for the model to do this turn. Without this, a
    // model that doesn't voluntarily emit `end_turn` keeps iterating/
    // re-planning, and the turn never reaches `TurnCompleted` — which is
    // what the client's plan-approval gate waits on. Guarded strictly to
    // Plan mode: in any other mode `ExitPlanMode` shouldn't be called, and if
    // it somehow is, this must not change behavior.
    if matches!(
        opts.permission_mode,
        Some(agentloop_contracts::PermissionMode::Plan)
    ) && tool_requests.iter().any(|request| {
        request.name == agentloop_core::tool::EXIT_PLAN_MODE_TOOL_NAME
            && manager.get(&request.id).is_some_and(|call| {
                matches!(&call.status, agentloop_contracts::ToolCallStatus::Completed)
                    && call.result.as_ref().is_some_and(|result| !result.is_error)
            })
    }) {
        return Ok(IterationOutcome::Stop(TurnStopReason::EndTurn));
    }

    Ok(IterationOutcome::Continue)
}
