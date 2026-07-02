//! One turn of the native loop: prompt → model → tools → repeat until idle.
//!
//! Concurrency rule: consecutive read-only tool calls execute concurrently
//! (bounded); mutating calls execute strictly sequentially at their position.
//! Results are keyed by call id, so protocol correctness never depends on
//! completion order while write-then-read hazards are impossible.
//!
//! Cancellation is not an error: an interrupted turn marks in-flight calls
//! `Cancelled`, completes with `TurnStopReason::Cancelled`, and `prompt()`
//! returns `Ok`.

mod hooks;
mod iteration;
mod tool_exec;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{Instrument, info_span};

use agentloop_contracts::{
    AgentEvent, HookPoint, MessageId, PromptInput, TokenUsage, TurnId, TurnOptions, TurnStopReason,
    TurnSummary, now_ms,
};
use agentloop_core::hook::{HookData, HookOutcome};
use agentloop_core::{AgentError, EventSink};

use crate::attachments::resolve_blob_paths;
use crate::deps::TurnDeps;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;

use self::hooks::run_hooks;
use self::iteration::run_iteration;

/// Outcome of one loop iteration.
pub(super) enum IterationOutcome {
    Continue,
    Stop(TurnStopReason),
}

pub(crate) async fn run_turn(
    deps: &Arc<TurnDeps>,
    handle: Arc<SessionHandle>,
    mut input: PromptInput,
    opts: TurnOptions,
) -> Result<TurnSummary, AgentError> {
    let meta = deps.store.get_meta(&handle.id).await?;
    let turn_id = TurnId::generate();
    let cancel = CancellationToken::new();
    *handle
        .current_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
    let started_at = now_ms();

    let span = info_span!(
        "turn",
        session_id = %handle.id,
        turn_id = %turn_id,
        agent = %deps.agent_id
    );

    async {
        handle.set_turn_permission_mode(opts.permission_mode);

        // ── prompt intake ───────────────────────────────────────────────────
        let outcome = run_hooks(
            deps,
            &handle,
            HookPoint::UserPromptSubmit,
            &turn_id,
            HookData::UserPrompt { input: &mut input },
        )
        .await?;
        if let HookOutcome::Block { reason } = outcome {
            return Err(AgentError::Other(format!(
                "prompt rejected by hook: {reason}"
            )));
        }
        resolve_blob_paths(&mut input, &meta.cwd).await?;

        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                },
            )
            .await?;
        if let Some(command) = &input.command {
            handle
                .emit_persistent(
                    Some(&turn_id),
                    AgentEvent::CommandExpanded {
                        name: command.name.clone(),
                        args: command.args.clone(),
                    },
                )
                .await?;
        }
        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::UserMessage {
                    message_id: MessageId::generate(),
                    content: input.parts.clone(),
                },
            )
            .await?;

        // ── side-channel events (tools emit progress/plans/questions) ──────
        let (sink, mut sink_rx) = EventSink::channel();
        let drain = tokio::spawn({
            let handle = handle.clone();
            let turn_id = turn_id.clone();
            async move {
                while let Some(event) = sink_rx.recv().await {
                    if event.is_persistent() {
                        let _ = handle.emit_persistent(Some(&turn_id), event).await;
                    } else {
                        handle.emit_ephemeral(Some(&turn_id), event);
                    }
                }
            }
        });

        // ── the loop ────────────────────────────────────────────────────────
        let mut usage_total = TokenUsage::default();
        let mut num_model_calls = 0u32;
        let mut num_tool_calls = 0u32;
        let mut manager = ToolCallManager::new();
        let mut stop_reason = TurnStopReason::MaxIterations;

        for _iteration in 0..deps.limits.max_iterations {
            if cancel.is_cancelled() {
                stop_reason = TurnStopReason::Cancelled;
                break;
            }
            let outcome = run_iteration(
                deps,
                &handle,
                &meta,
                &turn_id,
                &opts,
                &cancel,
                &sink,
                &mut manager,
                &mut usage_total,
                &mut num_model_calls,
                &mut num_tool_calls,
            )
            .await;
            match outcome {
                Ok(IterationOutcome::Continue) => continue,
                Ok(IterationOutcome::Stop(reason)) => {
                    stop_reason = reason;
                    break;
                }
                Err(err) => {
                    // Terminal failure: normalize, record, fail the turn.
                    for call in manager.cancel_in_flight() {
                        let _ = handle
                            .emit_persistent(Some(&turn_id), AgentEvent::ToolCallUpdated { call })
                            .await;
                    }
                    let _ = handle
                        .emit_persistent(
                            Some(&turn_id),
                            AgentEvent::SessionError {
                                error: err.to_engine_error(),
                            },
                        )
                        .await;
                    let summary = TurnSummary {
                        turn_id: turn_id.clone(),
                        stop_reason: TurnStopReason::Error,
                        usage: usage_total,
                        cost_usd: None,
                        num_model_calls,
                        num_tool_calls,
                        duration_ms: now_ms().saturating_sub(started_at),
                    };
                    let _ = handle
                        .emit_persistent(
                            Some(&turn_id),
                            AgentEvent::TurnCompleted {
                                turn_id: turn_id.clone(),
                                summary,
                            },
                        )
                        .await;
                    drop(sink);
                    let _ = drain.await;
                    return Err(err);
                }
            }
        }

        if cancel.is_cancelled() {
            stop_reason = TurnStopReason::Cancelled;
        }
        if stop_reason == TurnStopReason::Cancelled {
            for call in manager.cancel_in_flight() {
                let _ = handle
                    .emit_persistent(Some(&turn_id), AgentEvent::ToolCallUpdated { call })
                    .await;
            }
        }

        let summary = TurnSummary {
            turn_id: turn_id.clone(),
            stop_reason,
            usage: usage_total,
            cost_usd: None,
            num_model_calls,
            num_tool_calls,
            duration_ms: now_ms().saturating_sub(started_at),
        };
        handle
            .emit_persistent(
                Some(&turn_id),
                AgentEvent::TurnCompleted {
                    turn_id: turn_id.clone(),
                    summary: summary.clone(),
                },
            )
            .await?;
        drop(sink);
        let _ = drain.await;
        Ok(summary)
    }
    .instrument(span)
    .await
}
