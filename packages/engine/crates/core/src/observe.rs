//! Metric recording — the single mapping from persisted events to metrics.
//!
//! Called at the one choke point where events are appended and broadcast, so
//! native and delegated agents produce identical metrics, and any third-party
//! `Agent` implementation that reuses the shared session pipeline gets
//! metrics for free. The `metrics` facade is a no-op unless the runner
//! installs a recorder.

use agentloop_contracts::observe::*;
use agentloop_contracts::{AgentEvent, ToolCallStatus};

/// Record metrics for one persisted event. Ephemeral events are ignored.
pub fn record_event_metrics(agent_id: &str, event: &AgentEvent) {
    match event {
        AgentEvent::TurnCompleted { summary, .. } => {
            let stop = format!("{:?}", summary.stop_reason).to_lowercase();
            metrics::counter!(
                METRIC_TURNS_TOTAL,
                LABEL_AGENT => agent_id.to_owned(),
                LABEL_STOP_REASON => stop
            )
            .increment(1);
            metrics::histogram!(METRIC_TURN_DURATION_MS, LABEL_AGENT => agent_id.to_owned())
                .record(summary.duration_ms as f64);
            for (direction, amount) in [
                ("input", Some(summary.usage.input)),
                ("output", Some(summary.usage.output)),
                ("cache_read", summary.usage.cache_read),
                ("cache_write", summary.usage.cache_write),
            ] {
                if let Some(amount) = amount {
                    metrics::counter!(
                        METRIC_TOKENS_TOTAL,
                        LABEL_AGENT => agent_id.to_owned(),
                        LABEL_DIRECTION => direction
                    )
                    .increment(amount);
                }
            }
            if let Some(cost) = summary.cost_usd {
                metrics::counter!(METRIC_COST_USD_MICROS_TOTAL, LABEL_AGENT => agent_id.to_owned())
                    .increment((cost * 1_000_000.0) as u64);
            }
        }
        AgentEvent::ToolCallUpdated { call } if call.status.is_terminal() => {
            let status = match &call.status {
                ToolCallStatus::Completed => "completed",
                ToolCallStatus::Failed { .. } => "failed",
                ToolCallStatus::Denied { .. } => "denied",
                ToolCallStatus::Cancelled => "cancelled",
                _ => "unknown",
            };
            metrics::counter!(
                METRIC_TOOL_CALLS_TOTAL,
                LABEL_AGENT => agent_id.to_owned(),
                LABEL_TOOL => call.tool_name.clone(),
                LABEL_STATUS => status
            )
            .increment(1);
            if let Some(duration) = call.timing.duration_ms() {
                metrics::histogram!(
                    METRIC_TOOL_DURATION_MS,
                    LABEL_AGENT => agent_id.to_owned(),
                    LABEL_TOOL => call.tool_name.clone()
                )
                .record(duration as f64);
            }
            if let Some(wait) = call.timing.permission_wait_ms {
                metrics::histogram!(METRIC_PERMISSION_WAIT_MS, LABEL_AGENT => agent_id.to_owned())
                    .record(wait as f64);
            }
        }
        AgentEvent::SessionError { error } => {
            metrics::counter!(
                METRIC_ERRORS_TOTAL,
                LABEL_AGENT => agent_id.to_owned(),
                LABEL_KIND => format!("{:?}", error.code).to_lowercase()
            )
            .increment(1);
        }
        AgentEvent::CompactionBoundary { summary } => {
            metrics::counter!(
                METRIC_COMPACTIONS_TOTAL,
                LABEL_AGENT => agent_id.to_owned(),
                "strategy" => summary.strategy.clone()
            )
            .increment(1);
        }
        _ => {}
    }
}
