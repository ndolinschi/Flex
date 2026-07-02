//! One model iteration: build request, stream response, materialize assistant.

use std::sync::Arc;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::info_span;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, ProviderId, SessionMeta, StopReason, TokenUsage, TurnId,
    TurnOptions, TurnStopReason, now_ms,
};
use agentloop_core::hook::{HookData, HookOutcome};
use agentloop_core::provider::ChatRequest;
use agentloop_core::{AgentError, EventSink, ProviderError};

use crate::compaction::compact_session;
use crate::context_budget::{
    AUTO_COMPACT_STRATEGY, estimate_request_tokens, resolve_context_limit, should_auto_compact,
};
use crate::deps::TurnDeps;
use crate::draft::AssistantDraft;
use crate::manager::ToolCallManager;
use crate::messages::transcript_to_messages;
use crate::session_handle::SessionHandle;

use super::IterationOutcome;
use super::hooks::run_hooks;
use super::tool_exec::execute_tool_requests;

/// One model call plus its tool executions.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_iteration(
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
) -> Result<IterationOutcome, AgentError> {
    let mut auto_compacted = false;

    // Failover chain: the effective model first, then `fallback_models` in
    // order (deduped). Each candidate is tried at most once per iteration;
    // partial output from a failed attempt is discarded before it is ever
    // materialized, so a retry rebuilds cleanly from the log.
    let primary = opts
        .model
        .clone()
        .or_else(|| meta.model.clone())
        .or_else(|| deps.default_model.clone())
        .ok_or_else(|| {
            AgentError::Other(
                "no model configured: pass TurnOptions.model, set a session model, \
                 or configure a default model"
                    .to_owned(),
            )
        })?;
    let mut chain = vec![primary];
    for candidate in &opts.fallback_models {
        if !chain.contains(candidate) {
            chain.push(candidate.clone());
        }
    }

    let mut attempt = 0usize;
    let (draft, was_cancelled, llm_started, llm_span) = loop {
        let model_ref = chain[attempt].clone();
        let next_model = chain.get(attempt + 1).cloned();
        // ── build the request from the log ──────────────────────────────────────
        let events = deps.store.read(&handle.id, 0).await?;
        let transcript =
            agentloop_contracts::reduce(events.iter().map(|(_, event)| event).collect::<Vec<_>>());
        let messages = transcript_to_messages(&transcript);

        let mut system = deps.system_prompt.clone();
        if let Some(append) = &opts.system_append {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(append);
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
                continue;
            }
            return Err(AgentError::Other(message));
        };

        let mut request = ChatRequest::new(model.clone(), messages);
        request.system = (!system.is_empty()).then_some(system.clone());
        request.tools = deps.tools.specs(&Default::default());

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

        // Forward extended thinking only to providers that declare the
        // capability, so strict APIs never receive an unknown field.
        if opts.thinking.is_some() && provider.capabilities().thinking {
            request.thinking = opts.thinking;
        }
        if !opts.extra.is_empty() {
            for (key, value) in &opts.extra {
                request
                    .extra
                    .insert(ProviderId::from(key.as_str()), value.clone());
            }
        }

        // ── stream the model response ───────────────────────────────────────
        let llm_started = now_ms();
        let llm_span = info_span!("llm_request", provider = %provider.id(), model = %model);
        let mut stream = {
            let _enter = llm_span.enter();
            match provider.stream_chat(request, cancel.child_token()).await {
                Ok(stream) => stream,
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

        if let Some(err) = stream_err {
            if fallback_eligible(&err) {
                // The partial draft is dropped here, never materialized —
                // the retry rebuilds its context from the persisted log.
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
                    continue;
                }
            }
            return Err(err.into());
        }

        break (draft, was_cancelled, llm_started, llm_span);
    };

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
        // Stop hook may inject a continuation.
        let mut continuation: Option<String> = None;
        let outcome = run_hooks(
            deps,
            handle,
            agentloop_contracts::HookPoint::Stop,
            turn_id,
            HookData::Stop {
                continuation: &mut continuation,
            },
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

    Ok(IterationOutcome::Continue)
}

/// Whether a provider failure should advance the fallback chain. Terminal
/// classes (invalid request, context overflow, cancellation) never fall back.
fn fallback_eligible(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::RateLimited { .. }
            | ProviderError::Http { .. }
            | ProviderError::Stream { .. }
            | ProviderError::ModelUnavailable { .. }
            | ProviderError::AuthRejected { .. }
            | ProviderError::AuthMissing { .. }
    )
}

/// Record a model switch in the session log (best effort — a store hiccup
/// must not abort the retry that keeps the turn alive).
async fn emit_fallback(
    handle: &Arc<SessionHandle>,
    turn_id: &TurnId,
    from: &agentloop_contracts::ModelRef,
    to: Option<&agentloop_contracts::ModelRef>,
    reason: agentloop_contracts::EngineError,
) {
    tracing::warn!(
        target: "loop",
        from = %from,
        to = to.map(ToString::to_string).unwrap_or_else(|| "<exhausted>".to_owned()),
        "model fallback: {}",
        reason.message
    );
    if let Err(err) = handle
        .emit_persistent(
            Some(turn_id),
            AgentEvent::ModelFallback {
                from: from.clone(),
                to: to.cloned(),
                reason,
            },
        )
        .await
    {
        tracing::warn!(target: "loop", "could not persist model fallback: {err}");
    }
}
