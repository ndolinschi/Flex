//! One model iteration orchestrator.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{SessionMeta, TokenUsage, TurnId, TurnOptions};
use agentloop_core::{AgentError, EventSink};

use crate::deps::TurnDeps;
use crate::manager::ToolCallManager;
use crate::session_handle::SessionHandle;

use super::super::IterationOutcome;
use super::finish::finish_iteration;
use super::stream::{StreamResult, stream_model_response};

/// One model call plus its tool executions.
#[allow(clippy::too_many_arguments)]
pub(in crate::turn) async fn run_iteration(
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
) -> Result<IterationOutcome, AgentError> {
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
    let fallback_source = if !opts.fallback_models.is_empty() {
        &opts.fallback_models
    } else {
        &meta.fallback_models
    };
    let mut chain = vec![primary];
    for candidate in fallback_source {
        if !chain.contains(candidate) {
            chain.push(candidate.clone());
        }
    }

    match stream_model_response(deps, handle, meta, turn_id, opts, cancel, sink, &chain).await? {
        StreamResult::Stop(outcome) => Ok(outcome),
        StreamResult::Draft {
            draft,
            was_cancelled,
            llm_started,
            llm_span,
        } => {
            finish_iteration(
                deps,
                handle,
                meta,
                turn_id,
                opts,
                cancel,
                sink,
                manager,
                usage_total,
                num_model_calls,
                num_tool_calls,
                last_model,
                draft,
                was_cancelled,
                llm_started,
                llm_span,
            )
            .await
        }
    }
}
