use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc};
use tokio::task::AbortHandle;
use tracing::{Instrument, info_span};

use agentloop_contracts::{ToolCallId, ToolOutput};
use agentloop_core::tool::{Tool, ToolContext, ToolError};

pub(crate) struct ToolJob {
    pub(crate) call_id: ToolCallId,
    pub(crate) tool: Arc<dyn Tool>,
    pub(crate) ctx: ToolContext,
    pub(crate) input: serde_json::Value,
    pub(crate) timeout: Duration,
}

pub(crate) enum ToolEvent {
    Started {
        call_id: ToolCallId,
    },
    Finished {
        call_id: ToolCallId,
        outcome: ToolJobOutcome,
    },
}

pub(crate) enum ToolJobOutcome {
    Output(Result<ToolOutput, ToolError>),
    Panicked { message: String },
}

pub(crate) struct ToolWorkerPool {
    global: Option<Arc<Semaphore>>,
}

impl ToolWorkerPool {
    pub(crate) fn new(global_cap: Option<usize>) -> Self {
        Self {
            global: global_cap.map(|n| Arc::new(Semaphore::new(n))),
        }
    }

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
