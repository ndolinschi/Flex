mod hooks;
mod iteration;
pub(crate) mod tool_exec;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{Instrument, info_span};

use agentloop_contracts::{
    AgentEvent, HookPoint, MessageId, PromptInput, SessionMetaPatch, TokenUsage, TurnId,
    TurnOptions, TurnStopReason, TurnSummary, now_ms, price_for,
};
use agentloop_core::hook::{HookData, HookOutcome};
use agentloop_core::{AgentError, EventSink};

use crate::attachments::resolve_blob_paths;
use crate::deps::TurnDeps;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;

use self::hooks::run_hooks;
use self::iteration::run_iteration;

pub(super) enum IterationOutcome {
    Continue,
    Stop(TurnStopReason),
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn flush_turn_events(
    drain: tokio::task::JoinHandle<()>,
    done: tokio::sync::oneshot::Sender<()>,
) {
    let _ = done.send(());
    const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
    let _ = tokio::time::timeout(FLUSH_TIMEOUT, drain).await;
}

pub(crate) async fn run_turn(
    deps: &Arc<TurnDeps>,
    handle: Arc<SessionHandle>,
    mut input: PromptInput,
    opts: TurnOptions,
    done: tokio::sync::oneshot::Sender<()>,
) -> Result<TurnSummary, AgentError> {
    let initial_meta = deps.store.get_meta(&handle.id).await?;
    let meta = crate::workspace_ensure::ensure_root_workspace(deps, &handle, initial_meta).await?;
    let turn_id = TurnId::generate();
    let cancel = CancellationToken::new();
    *handle
        .current_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
    let started_at = now_ms();

    let _watchdog = opts.turn_timeout_ms.map(|ms| {
        let cancel = cancel.clone();
        AbortOnDrop(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            tracing::info!(target: "turn", timeout_ms = ms, "turn timeout elapsed; cancelling");
            cancel.cancel();
        }))
    });

    let span = info_span!(
        "turn",
        session_id = %handle.id,
        turn_id = %turn_id,
        agent = %deps.agent_id
    );

    async {
        handle.set_turn_permission_mode(opts.permission_mode);
        handle.set_turn_disable_tools(opts.disable_tools);
        handle.set_turn_effort(opts.effort);
        deps.routing.clear(&handle.id);

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

        let turn_result = async {
            let outcome = run_hooks(
                deps,
                &handle,
                HookPoint::UserPromptSubmit,
                &turn_id,
                HookData::UserPrompt { input: &mut input },
                Some(sink.clone()),
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
            if meta.title.is_none() && input.command.is_none() {
                let text = input.joined_text();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let title: String = trimmed.chars().take(60).collect();
                    let _ = deps
                        .store
                        .update_meta(
                            &handle.id,
                            SessionMetaPatch {
                                title: Some(title),
                                ..Default::default()
                            },
                        )
                        .await;
                }
            }

            let mut usage_total = TokenUsage::default();
            let mut num_model_calls = 0u32;
            let mut num_tool_calls = 0u32;
            let mut manager = ToolCallManager::new();
            let mut stop_reason = TurnStopReason::MaxIterations;
            let mut last_model: Option<String> = None;

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
                    &mut last_model,
                )
                .await;
                match outcome {
                    Ok(IterationOutcome::Continue) => continue,
                    Ok(IterationOutcome::Stop(reason)) => {
                        stop_reason = reason;
                        break;
                    }
                    Err(err) => {
                        tracing::error!(
                            target: "turn",
                            session_id = %handle.id,
                            turn_id = %turn_id,
                            error = %err,
                            "turn failed"
                        );
                        for call in manager.cancel_in_flight() {
                            let _ = handle
                                .emit_persistent(
                                    Some(&turn_id),
                                    AgentEvent::ToolCallUpdated { call },
                                )
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
                            cost_usd: cost_for_turn(last_model.as_deref(), &usage_total),
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
                cost_usd: cost_for_turn(last_model.as_deref(), &usage_total),
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

            tracing::info!(
                target: "turn",
                session_id = %handle.id,
                turn_id = %turn_id,
                stop_reason = ?summary.stop_reason,
                duration_ms = summary.duration_ms,
                num_model_calls = summary.num_model_calls,
                num_tool_calls = summary.num_tool_calls,
                "turn completed"
            );

            if let Some(workspace) = &deps.workspace {
                match workspace
                    .snapshot(&meta.cwd, &format!("turn {turn_id}"))
                    .await
                {
                    Ok(Some(snapshot_id)) => {
                        let _ = handle
                            .emit_persistent(
                                Some(&turn_id),
                                AgentEvent::SnapshotCreated {
                                    snapshot_id,
                                    turn_id: turn_id.clone(),
                                },
                            )
                            .await;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::debug!(target: "turn", error = %err, "workspace snapshot skipped");
                    }
                }
            }

            Ok(summary)
        }
        .await;

        drop(sink);
        flush_turn_events(drain, done).await;
        turn_result
    }
    .instrument(span)
    .await
}

fn cost_for_turn(model: Option<&str>, usage: &TokenUsage) -> Option<f64> {
    let price = price_for(model?)?;
    Some(price.cost(usage))
}
