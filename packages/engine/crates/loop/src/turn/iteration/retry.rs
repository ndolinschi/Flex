//! Provider failure classification, retry schedule, and model failover helpers.

use std::sync::Arc;
use std::time::Duration;

use agentloop_contracts::{AgentEvent, TurnId};
use agentloop_core::ProviderError;
use tokio_util::sync::CancellationToken;

use crate::session_handle::SessionHandle;

/// Whether a provider failure should advance the fallback chain. Terminal
/// classes (invalid request, context overflow, cancellation) never fall back.
/// Context overflow is recovered by compacting and retrying — not by failing
/// over to another model, which would face the same oversized context.
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

/// Bounded same-model retries for a failure that only manifests once a
/// response is already streaming (a dropped connection mid-turn, one
/// corrupted frame). These read as a transient wire hiccup on an otherwise
/// healthy model, not a reason to burn a configured fallback model or
/// abandon the turn outright the way a connect-time failure (already retried
/// inside the provider's own `send_chat_request`) would.
pub(super) const MAX_STREAM_RETRIES: u32 = 2;
pub(super) const STREAM_RETRY_BASE_BACKOFF_MS: u64 = 250;

pub(super) fn mid_stream_retryable(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::Stream { .. } | ProviderError::Http { .. }
    )
}

/// Whether a failure is RETRYABLE under the patient [`RetryPolicy`] schedule:
/// timeouts, dropped/reset connections and other transport failures
/// (`Http`), a stream cut mid-response (`Stream`), and rate limiting
/// (`RateLimited`). These are transient — the same request to the same
/// model is expected to succeed once the network or provider recovers.
///
/// TERMINAL classes never enter the schedule: `AuthMissing`/`AuthRejected`
/// (a wait won't fix bad credentials), `InvalidRequest`/`ModelUnavailable`
/// (the request itself is the problem), `ContextOverflow` (handled by
/// compaction above, not retried), and `Cancelled` (the user asked to stop).
pub(super) fn is_retryable(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::Http { .. }
            | ProviderError::Stream { .. }
            | ProviderError::RateLimited { .. }
    )
}

/// Outcome of consulting the retry schedule for one failure.
pub(super) enum RetryDecision {
    /// A delay was slept (or a zero-length hint elapsed instantly); the
    /// caller should `continue` the model-call loop to retry the same model.
    Retry,
    /// The cancel token fired while sleeping; the turn is stopping.
    Cancelled,
    /// The schedule is exhausted (or the error's own attempt counter is
    /// already past `max_attempts`); the caller should fall through to the
    /// existing fallback/model-exhausted handling.
    Exhausted,
}

/// Consult `policy` for `err`, incrementing `*retry_attempt` and — if the
/// schedule still has a slot — emitting [`AgentEvent::RetryScheduled`] and
/// sleeping the scheduled delay (or the provider's own `retry_after_ms` hint
/// when the error carries one, which takes priority over the schedule step).
/// The sleep races the turn's cancel token so pressing Stop during a
/// multi-minute backoff cancels immediately instead of waiting it out.
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

    // Exhaustion is governed by the schedule's attempt budget regardless of
    // which delay source is used below: a `Retry-After` hint picks *how
    // long* to wait, not *whether* the turn still has attempts left.
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

/// Record a model switch in the session log (best effort — a store hiccup
/// must not abort the retry that keeps the turn alive).
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
