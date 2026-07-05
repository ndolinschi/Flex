//! One model iteration: build request, stream response, materialize assistant.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::info_span;

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, ProviderId, SessionMeta, StopReason, ThinkingConfig,
    TokenUsage, TurnId, TurnOptions, TurnStopReason, now_ms,
};
use agentloop_core::hook::{HookData, HookOutcome};
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
    let mut stream_retries = 0u32;
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
        // Effort guidance composes after the role append: the role prompt says
        // what the job is, this says how hard to work at it. Only when the
        // caller set an effort — `None` keeps the prompt byte-for-byte as before.
        if let Some(effort) = opts.effort {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(effort::guidance(effort));
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
        // Role-scoped tools: subagent sessions see only their role's set.
        let tool_filter = match meta.role.as_deref() {
            Some(role) => deps.roles.tool_filter(role, &deps.tools, meta.depth),
            None => Default::default(),
        };
        request.tools = deps.tools.specs(&tool_filter);

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

        // Extended-thinking budget: an explicit `/thinking` budget wins;
        // otherwise derive one from the effort level, scaled per role. Forward
        // it only to providers that declare the capability, so strict APIs
        // never receive an unknown field.
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

        // ── stream the model response ───────────────────────────────────────
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
        // A stream that closes without ever delivering a terminal event
        // (`MessageEnd`/`Usage`) has been cut off mid-response, not
        // deliberately finished — the wire dropped, it was not the model
        // saying "done". Track whether we actually saw one so `None => break`
        // can tell the two cases apart instead of treating a truncated
        // partial draft as a normal, successful completion.
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
            // The provider connection ended before signalling completion —
            // surface it like any other stream failure so it goes through
            // the same compaction/fallback/retry path instead of silently
            // persisting a truncated partial draft as a successful answer.
            stream_err = Some(ProviderError::Stream {
                provider: provider.id(),
                message: "stream ended before a MessageEnd/Usage event was received \
                          (truncated response)"
                    .to_owned(),
            });
        }

        if let Some(err) = stream_err {
            if is_context_overflow(&err) && !auto_compacted {
                // The partial draft is dropped; compaction rewrites the log and
                // the retry rebuilds a smaller request from it.
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
                // The partial draft is dropped; a fresh request rebuilds
                // cleanly from the persisted log on the same model. This is
                // not a fallback (no model switch, nothing persisted about
                // it) — just absorbing a one-off dropped connection or
                // corrupted frame before treating the model itself as bad.
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
                        return Ok(IterationOutcome::Stop(TurnStopReason::Cancelled));
                    }
                    _ = tokio::time::sleep(Duration::from_millis(stream_retry_backoff_ms(stream_retries))) => {}
                }
                continue;
            }
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
                    stream_retries = 0;
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
/// Context overflow is recovered by compacting and retrying — not by failing
/// over to another model, which would face the same oversized context.
fn is_context_overflow(err: &ProviderError) -> bool {
    matches!(err, ProviderError::ContextOverflow { .. })
}

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

/// Bounded same-model retries for a failure that only manifests once a
/// response is already streaming (a dropped connection mid-turn, one
/// corrupted frame). These read as a transient wire hiccup on an otherwise
/// healthy model, not a reason to burn a configured fallback model or
/// abandon the turn outright the way a connect-time failure (already retried
/// inside the provider's own `send_chat_request`) would.
const MAX_STREAM_RETRIES: u32 = 2;
const STREAM_RETRY_BASE_BACKOFF_MS: u64 = 250;

fn mid_stream_retryable(err: &ProviderError) -> bool {
    matches!(err, ProviderError::Stream { .. } | ProviderError::Http { .. })
}

fn stream_retry_backoff_ms(attempt: u32) -> u64 {
    STREAM_RETRY_BASE_BACKOFF_MS.saturating_mul(1u64 << attempt.saturating_sub(1).min(4))
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
