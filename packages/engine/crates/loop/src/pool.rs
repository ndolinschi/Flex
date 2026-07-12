//! `ToolWorkerPool`: bounded, spawned tool-call execution with panic
//! isolation.
//!
//! Shape: semaphore + `tokio::spawn` + a tracked [`AbortHandle`] per job —
//! not N fixed worker tasks. The bound is the same, but panic containment
//! comes free from [`tokio::task::JoinError::is_panic`] (no `catch_unwind`
//! over `&dyn Tool` futures), idle cost is zero, and real parallelism lands
//! on the multithreaded runtime. This type is also the seam where fixed
//! workers or remote dispatch can be swapped in later behind the same
//! job/result messages.
//!
//! Permit discipline: a job acquires its session permit, then the global
//! permit (if any), *before* reporting [`ToolEvent::Started`]. The prepare
//! stage (permissions, hooks) runs on the turn task and never holds a
//! permit, so a parked permission ask can never starve the pool.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc};
use tokio::task::AbortHandle;
use tracing::{Instrument, info_span};

use agentloop_contracts::{ToolCallId, ToolOutput};
use agentloop_core::tool::{Tool, ToolContext, ToolError};

/// One unit of tool work, fully owned so it can move onto a spawned task.
pub(crate) struct ToolJob {
    pub(crate) call_id: ToolCallId,
    pub(crate) tool: Arc<dyn Tool>,
    /// Owned context: session/turn/call ids, cwd, per-job cancel token, sink.
    pub(crate) ctx: ToolContext,
    pub(crate) input: serde_json::Value,
    pub(crate) timeout: Duration,
}

/// Progress reports from a pool job back to the turn task.
pub(crate) enum ToolEvent {
    /// Permits acquired; execution is starting (transition to `Running`).
    Started { call_id: ToolCallId },
    /// The job is done — normally, by cancellation, or by panic.
    Finished {
        call_id: ToolCallId,
        outcome: ToolJobOutcome,
    },
}

/// How a pool job ended.
pub(crate) enum ToolJobOutcome {
    /// The tool ran to a result (including tool-level errors and timeouts).
    Output(Result<ToolOutput, ToolError>),
    /// The tool panicked. The call fails; the turn continues.
    Panicked { message: String },
}

/// Engine-wide tool execution pool, built once per [`crate::NativeAgent`].
///
/// The per-session bound (`LoopLimits::tool_concurrency`) travels with each
/// [`ToolWorkerPool::submit`] call as an owned semaphore; the pool itself
/// holds only the optional global cross-session cap.
pub(crate) struct ToolWorkerPool {
    /// Global cross-session cap; `None` = per-session caps only.
    global: Option<Arc<Semaphore>>,
}

impl ToolWorkerPool {
    pub(crate) fn new(global_cap: Option<usize>) -> Self {
        Self {
            global: global_cap.map(|n| Arc::new(Semaphore::new(n))),
        }
    }

    /// Spawn `job` onto the runtime. The job queues on `session_permits`
    /// (then the global cap), reports [`ToolEvent::Started`], runs the tool
    /// under its timeout raced against the job's cancel token, and always
    /// reports [`ToolEvent::Finished`] — unless the returned [`AbortHandle`]
    /// is used, which is a last-resort hard stop.
    pub(crate) fn submit(
        &self,
        job: ToolJob,
        session_permits: Arc<Semaphore>,
        results: mpsc::Sender<ToolEvent>,
    ) -> AbortHandle {
        let global = self.global.clone();
        let span = info_span!(
            "tool_call",
            tool = %job.tool.descriptor().name,
            call_id = %job.call_id
        );
        span.follows_from(tracing::Span::current());
        let handle = tokio::spawn(run_job(job, global, session_permits, results).instrument(span));
        handle.abort_handle()
    }
}

async fn run_job(
    job: ToolJob,
    global: Option<Arc<Semaphore>>,
    session_permits: Arc<Semaphore>,
    results: mpsc::Sender<ToolEvent>,
) {
    let ToolJob {
        call_id,
        tool,
        ctx,
        input,
        timeout,
    } = job;
    let cancel = ctx.cancel.clone();

    let _session_permit = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            finish(&results, call_id, ToolJobOutcome::Output(Err(ToolError::Cancelled))).await;
            return;
        }
        permit = session_permits.acquire_owned() => match permit {
            Ok(permit) => permit,
            Err(_) => {
                finish(&results, call_id, ToolJobOutcome::Output(Err(ToolError::Cancelled)))
                    .await;
                return;
            }
        },
    };
    let _global_permit = match global {
        None => None,
        Some(global) => tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                finish(&results, call_id, ToolJobOutcome::Output(Err(ToolError::Cancelled)))
                    .await;
                return;
            }
            permit = global.acquire_owned() => match permit {
                Ok(permit) => Some(permit),
                Err(_) => {
                    finish(&results, call_id, ToolJobOutcome::Output(Err(ToolError::Cancelled)))
                        .await;
                    return;
                }
            },
        },
    };

    let _ = results
        .send(ToolEvent::Started {
            call_id: call_id.clone(),
        })
        .await;

    let run_cancel = cancel.clone();
    let inner = tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = run_cancel.cancelled() => Err(ToolError::Cancelled),
            result = tokio::time::timeout(timeout, tool.run(ctx, input)) => match result {
                Ok(inner) => inner,
                Err(_) => Err(ToolError::Timeout(timeout.as_millis() as u64)),
            },
        }
    });
    let outcome = match inner.await {
        Ok(result) => ToolJobOutcome::Output(result),
        Err(err) if err.is_panic() => ToolJobOutcome::Panicked {
            message: panic_message(err),
        },
        Err(_) => ToolJobOutcome::Output(Err(ToolError::Cancelled)),
    };
    finish(&results, call_id, outcome).await;
}

async fn finish(results: &mpsc::Sender<ToolEvent>, call_id: ToolCallId, outcome: ToolJobOutcome) {
    let _ = results.send(ToolEvent::Finished { call_id, outcome }).await;
}

/// Extract a human-readable message from a panicked task's payload.
fn panic_message(err: tokio::task::JoinError) -> String {
    if !err.is_panic() {
        return "task aborted".to_owned();
    }
    let payload = err.into_panic();
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}
