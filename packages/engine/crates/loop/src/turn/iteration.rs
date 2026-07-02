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

use crate::agent::NativeAgent;
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
    agent: &NativeAgent,
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
    // ── build the request from the log ──────────────────────────────────────
    let events = agent.store.read(&handle.id, 0).await?;
    let transcript =
        agentloop_contracts::reduce(events.iter().map(|(_, event)| event).collect::<Vec<_>>());
    let messages = transcript_to_messages(&transcript);

    let mut system = agent.system_prompt.clone();
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
        .or_else(|| agent.default_model.clone())
        .ok_or_else(|| {
            AgentError::Other(
                "no model configured: pass TurnOptions.model, set a session model, \
                 or configure a default model"
                    .to_owned(),
            )
        })?;
    let (provider, model) = agent.providers.resolve(&model_ref).ok_or_else(|| {
        AgentError::Other(format!(
            "no provider registered for model reference `{model_ref}`; \
             registered providers: {:?}",
            agent.providers.ids()
        ))
    })?;

    let mut request = ChatRequest::new(model.clone(), messages);
    request.system = (!system.is_empty()).then_some(system);
    request.tools = agent.tools.specs(&Default::default());
    if !opts.extra.is_empty() {
        for (key, value) in &opts.extra {
            request
                .extra
                .insert(ProviderId::from(key.as_str()), value.clone());
        }
    }

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
            agent,
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
        agent,
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
