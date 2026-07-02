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
    let (request, provider, model) = loop {
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

        let model_ref = opts
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
        let (provider, model) = deps.providers.resolve(&model_ref).ok_or_else(|| {
            AgentError::Other(format!(
                "no provider registered for model reference `{model_ref}`; \
                 registered providers: {:?}",
                deps.providers.ids()
            ))
        })?;

        let mut request = ChatRequest::new(model.clone(), messages);
        request.system = (!system.is_empty()).then_some(system.clone());
        request.tools = deps.tools.specs(&Default::default());
        // #region agent log
        {
            use std::io::Write;
            let body_estimate = crate::context_budget::estimate_request_chars(
                request.system.as_deref().unwrap_or(""),
                &request,
            );
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/Users/ndolinschi/Documents/Apps/AgenticStudio/.cursor/debug-79ecfd.log")
            {
                let payload = serde_json::json!({
                    "sessionId": "79ecfd",
                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0),
                    "hypothesisId": "P",
                    "location": "iteration.rs:run_iteration",
                    "message": "llm request size estimate",
                    "data": {
                        "provider": provider.id().as_str(),
                        "model": model,
                        "chars": body_estimate,
                        "tokens_est": body_estimate.div_ceil(4),
                    },
                    "runId": "context-overflow",
                });
                let _ = writeln!(file, "{payload}");
            }
        }
        // #endregion

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

        break (request, provider, model);
    };

    // ── stream the model response ───────────────────────────────────────────
    let llm_started = now_ms();
    let llm_span = info_span!("llm_request", provider = %provider.id(), model = %model);
    let mut stream = {
        let _enter = llm_span.enter();
        provider.stream_chat(request, cancel.child_token()).await?
    };

    let mut draft = AssistantDraft::new();
    let mut was_cancelled = false;
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
                        return Err(err.into());
                    }
                }
            }
        }
    }

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
