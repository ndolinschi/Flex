use std::sync::Arc;
use std::time::Duration;

use agentloop_contracts::{AgentEvent, TurnId};
use agentloop_core::ProviderError;
use tokio_util::sync::CancellationToken;

use crate::session_handle::SessionHandle;

pub(super) fn is_context_overflow(err: &ProviderError) -> bool {
    matches!(err, ProviderError::ContextOverflow { .. })
}

pub(super) fn fallback_eligible(err: &ProviderError) -> bool {
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

pub(super) const MAX_STREAM_RETRIES: u32 = 2;
pub(super) const STREAM_RETRY_BASE_BACKOFF_MS: u64 = 250;

pub(super) fn mid_stream_retryable(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::Stream { .. } | ProviderError::Http { .. }
    )
}

pub(super) fn is_retryable(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::Http { .. }
            | ProviderError::Stream { .. }
            | ProviderError::RateLimited { .. }
    )
}

pub(super) enum RetryDecision {
    Retry,
    Cancelled,
    Exhausted,
}

pub(super) async fn schedule_retry(
    handle: &Arc<SessionHandle>,
    turn_id: &TurnId,
    cancel: &CancellationToken,
    policy: &crate::builder::RetryPolicy,
    retry_attempt: &mut u32,
    err: &ProviderError,
) -> RetryDecision {
    *retry_attempt += 1;
    let attempt = *retry_attempt;
    let max_attempts = policy.max_attempts();

    let Some(scheduled_delay) = policy.delay_for(attempt) else {
        return RetryDecision::Exhausted;
    };
    let retry_after_hint = match err {
        ProviderError::RateLimited {
            retry_after_ms: Some(ms),
            ..
        } => Some(Duration::from_millis(*ms)),
        _ => None,
    };
    let delay = retry_after_hint.unwrap_or(scheduled_delay);

    handle.emit_ephemeral(
        Some(turn_id),
        AgentEvent::RetryScheduled {
            attempt,
            max_attempts,
            delay_ms: delay.as_millis() as u64,
            error: err.to_string(),
        },
    );
    tracing::warn!(
        target: "loop",
        session_id = %handle.id,
        attempt,
        max_attempts,
        delay_ms = delay.as_millis() as u64,
        "provider/network failure — retrying same model: {err}"
    );

    tokio::select! {
        _ = cancel.cancelled() => RetryDecision::Cancelled,
        _ = tokio::time::sleep(delay) => RetryDecision::Retry,
    }
}

pub(super) fn stream_retry_backoff_ms(attempt: u32) -> u64 {
    STREAM_RETRY_BASE_BACKOFF_MS.saturating_mul(1u64 << attempt.saturating_sub(1).min(4))
}

pub(super) async fn emit_fallback(
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
