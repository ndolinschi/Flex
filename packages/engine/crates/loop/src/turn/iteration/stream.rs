//! Model streaming loop with compaction, retry, and failover.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::info_span;

use agentloop_contracts::{
    ProviderId, SessionMeta, ThinkingConfig, TurnId, TurnOptions, TurnStopReason, now_ms,
};
use agentloop_core::provider::ChatRequest;
use agentloop_core::provider::ProviderStreamEvent;
use agentloop_core::{AgentError, EventSink, ProviderError};

use crate::compaction::compact_session;
use crate::context_budget::{
    AUTO_COMPACT_STRATEGY, estimate_request_tokens, resolve_context_limit, should_auto_compact,
};
use crate::deps::TurnDeps;
use crate::draft::AssistantDraft;
use crate::effort;
use crate::messages::transcript_to_messages;
use crate::session_handle::SessionHandle;

use super::super::IterationOutcome;
use super::retry::{
    MAX_STREAM_RETRIES, RetryDecision, emit_fallback, fallback_eligible, is_context_overflow,
    is_retryable, mid_stream_retryable, schedule_retry, stream_retry_backoff_ms,
};

pub(super) enum StreamResult {
    Draft {
        draft: AssistantDraft,
        was_cancelled: bool,
        llm_started: u64,
        llm_span: tracing::Span,
    },
    Stop(IterationOutcome),
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn stream_model_response(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    opts: &TurnOptions,
    cancel: &CancellationToken,
    _sink: &EventSink,
    chain: &[agentloop_contracts::ModelRef],
) -> Result<StreamResult, AgentError> {
    let mut auto_compacted = false;
    let mut attempt = 0usize;
    let mut stream_retries = 0u32;
    let mut retry_attempt = 0u32;
    let (draft, was_cancelled, llm_started, llm_span) = loop {
        let model_ref = chain[attempt].clone();
        let next_model = chain.get(attempt + 1).cloned();
        let events = deps.store.read(&handle.id, 0).await?;
        let transcript = agentloop_contracts::reduce(
            events
                .iter()
                .map(|stored| &stored.event)
                .collect::<Vec<_>>(),
        );
        let messages = transcript_to_messages(&transcript);

        let mut system = deps.system_prompt.clone();
        if let Some(append) = &opts.system_append {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(append);
        }
        if let Some(effort) = opts.effort {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(effort::guidance(effort));
        }
        if matches!(
            opts.permission_mode,
            Some(agentloop_contracts::PermissionMode::Plan)
        ) {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(crate::plan::guidance());
        }

        let Some((provider, model)) = deps.providers.resolve(&model_ref) else {
            let message = format!(
                "no provider registered for model reference `{model_ref}`; \
                 registered providers: {:?}",
                deps.providers.ids()
            );
            if let Some(next) = next_model {
                emit_fallback(
                    handle,
                    turn_id,
                    &model_ref,
                    Some(&next),
                    agentloop_contracts::EngineError::engine(
                        agentloop_contracts::ErrorCode::InvalidRequest,
                        message,
                    ),
                )
                .await;
                attempt += 1;
                stream_retries = 0;
                continue;
            }
            return Err(AgentError::Other(message));
        };

        let mut request = ChatRequest::new(model.clone(), messages);
        request.system = (!system.is_empty()).then_some(system.clone());
        let tool_filter = match meta.role.as_deref() {
            Some(role) => deps.roles.tool_filter(role, &deps.tools, meta.depth),
            None => Default::default(),
        };
        request.tools = if opts.disable_tools {
            Vec::new()
        } else {
            deps.tools.specs(&tool_filter)
        };

        let tokens_est = estimate_request_tokens(request.system.as_deref().unwrap_or(""), &request);
        let context_limit = resolve_context_limit(&provider);
        if !auto_compacted && should_auto_compact(tokens_est, context_limit) {
            tracing::info!(
                target: "loop",
                session_id = %handle.id,
                tokens_est,
                context_limit,
                "auto-compacting session — context near limit"
            );
            compact_session(
                deps,
                handle.clone(),
                opts.clone(),
                cancel.clone(),
                AUTO_COMPACT_STRATEGY,
            )
            .await?;
            auto_compacted = true;
            continue;
        }

        let budget = opts
            .thinking
            .map(|thinking| thinking.budget_tokens)
            .or_else(|| {
                opts.effort
                    .and_then(|effort| effort::thinking_budget(effort, meta.role.as_deref()))
            });
        if let Some(budget) = budget.filter(|budget| *budget > 0) {
            if provider.capabilities().thinking {
                request.thinking = Some(ThinkingConfig {
                    budget_tokens: budget,
                });
            }
        }
        if !opts.extra.is_empty() {
            for (key, value) in &opts.extra {
                request
                    .extra
                    .insert(ProviderId::from(key.as_str()), value.clone());
            }
        }

        let llm_started = now_ms();
        let llm_span = info_span!("llm_request", provider = %provider.id(), model = %model);
        let mut stream = {
            let _enter = llm_span.enter();
            match provider.stream_chat(request, cancel.child_token()).await {
                Ok(stream) => stream,
                Err(err) if is_context_overflow(&err) && !auto_compacted => {
                    tracing::info!(
                        target: "loop",
                        session_id = %handle.id,
                        "context overflow — compacting and retrying the turn"
                    );
                    compact_session(
                        deps,
                        handle.clone(),
                        opts.clone(),
                        cancel.clone(),
                        AUTO_COMPACT_STRATEGY,
                    )
                    .await?;
                    auto_compacted = true;
                    continue;
                }
                Err(err) if is_retryable(&err) => {
                    match schedule_retry(
                        handle,
                        turn_id,
                        cancel,
                        &deps.limits.retry,
                        &mut retry_attempt,
                        &err,
                    )
                    .await
                    {
                        RetryDecision::Retry => continue,
                        RetryDecision::Cancelled => {
                            return Ok(StreamResult::Stop(IterationOutcome::Stop(
                                TurnStopReason::Cancelled,
                            )));
                        }
                        RetryDecision::Exhausted => {}
                    }
                    if fallback_eligible(&err) {
                        emit_fallback(
                            handle,
                            turn_id,
                            &model_ref,
                            next_model.as_ref(),
                            err.to_engine_error(),
                        )
                        .await;
                        if next_model.is_some() {
                            attempt += 1;
                            stream_retries = 0;
                            retry_attempt = 0;
                            continue;
                        }
                    }
                    return Err(err.into());
                }
                Err(err) if fallback_eligible(&err) => {
                    emit_fallback(
                        handle,
                        turn_id,
                        &model_ref,
                        next_model.as_ref(),
                        err.to_engine_error(),
                    )
                    .await;
                    if next_model.is_some() {
                        attempt += 1;
                        stream_retries = 0;
                        retry_attempt = 0;
                        continue;
                    }
                    return Err(err.into());
                }
                Err(err) => return Err(err.into()),
            }
        };

        let mut draft = AssistantDraft::new();
        let mut was_cancelled = false;
        let mut stream_err: Option<ProviderError> = None;
        let mut saw_terminal_event = false;
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    was_cancelled = true;
                    break;
                }
                item = stream.next() => {
                    match item {
                        None => break,
                        Some(Ok(event)) => {
                            if matches!(
                                event,
                                ProviderStreamEvent::MessageEnd { .. }
                                    | ProviderStreamEvent::Usage(_)
                            ) {
                                saw_terminal_event = true;
                            }
                            if let Some(delta) = draft.apply(event) {
                                handle.emit_ephemeral(Some(turn_id), delta);
                            }
                        }
                        Some(Err(err)) => {
                            if matches!(err, ProviderError::Cancelled { .. }) {
                                was_cancelled = true;
                                break;
                            }
                            stream_err = Some(err);
                            break;
                        }
                    }
                }
            }
        }

        if stream_err.is_none() && !was_cancelled && !saw_terminal_event {
            stream_err = Some(ProviderError::Stream {
                provider: provider.id(),
                message: "stream ended before a MessageEnd/Usage event was received \
                          (truncated response)"
                    .to_owned(),
            });
        }

        if let Some(err) = stream_err {
            if is_context_overflow(&err) && !auto_compacted {
                tracing::info!(
                    target: "loop",
                    session_id = %handle.id,
                    "context overflow mid-stream — compacting and retrying the turn"
                );
                compact_session(
                    deps,
                    handle.clone(),
                    opts.clone(),
                    cancel.clone(),
                    AUTO_COMPACT_STRATEGY,
                )
                .await?;
                auto_compacted = true;
                continue;
            }
            if mid_stream_retryable(&err) && stream_retries < MAX_STREAM_RETRIES {
                stream_retries += 1;
                tracing::info!(
                    target: "loop",
                    session_id = %handle.id,
                    model = %model_ref,
                    attempt = stream_retries,
                    "mid-stream failure — retrying same model: {err}"
                );
                tokio::select! {
                    _ = cancel.cancelled() => {
                        return Ok(StreamResult::Stop(IterationOutcome::Stop(TurnStopReason::Cancelled)));
                    }
                    _ = tokio::time::sleep(Duration::from_millis(stream_retry_backoff_ms(stream_retries))) => {}
                }
                continue;
            }
            if is_retryable(&err) {
                match schedule_retry(
                    handle,
                    turn_id,
                    cancel,
                    &deps.limits.retry,
                    &mut retry_attempt,
                    &err,
                )
                .await
                {
                    RetryDecision::Retry => continue,
                    RetryDecision::Cancelled => {
                        return Ok(StreamResult::Stop(IterationOutcome::Stop(
                            TurnStopReason::Cancelled,
                        )));
                    }
                    RetryDecision::Exhausted => {}
                }
            }
            if fallback_eligible(&err) {
                emit_fallback(
                    handle,
                    turn_id,
                    &model_ref,
                    next_model.as_ref(),
                    err.to_engine_error(),
                )
                .await;
                if next_model.is_some() {
                    attempt += 1;
                    stream_retries = 0;
                    retry_attempt = 0;
                    continue;
                }
            }
            return Err(err.into());
        }

        break (draft, was_cancelled, llm_started, llm_span);
    };

    Ok(StreamResult::Draft {
        draft,
        was_cancelled,
        llm_started,
        llm_span,
    })
}
