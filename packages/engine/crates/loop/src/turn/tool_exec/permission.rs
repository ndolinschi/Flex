use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, PermissionDecision, PermissionDecisionKind, PermissionRequestId, SessionMeta,
    ToolCallStatus, ToolOutput, TurnId,
};
use agentloop_core::tool::ToolDescriptor;

use crate::deps::TurnDeps;
use crate::draft::DraftToolCall;
use crate::manager::ToolCallManager;
use crate::permission::{PermissionPolicy, Verdict};
use crate::session_handle::SessionHandle;

pub(super) enum PermissionGate {
    Allow,
    Stop,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn gate_permission(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: &SessionMeta,
    turn_id: &TurnId,
    cancel: &CancellationToken,
    manager: &Arc<Mutex<ToolCallManager>>,
    request: &DraftToolCall,
    descriptor: &ToolDescriptor,
) -> PermissionGate {
    let emit_update = |call: agentloop_contracts::ToolCall| {
        let handle = handle.clone();
        let turn_id = turn_id.clone();
        async move {
            let _ = handle
                .emit_persistent(Some(&turn_id), AgentEvent::ToolCallUpdated { call })
                .await;
        }
    };
    let transition = |to: ToolCallStatus, result: Option<ToolOutput>| {
        manager
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .transition(&request.id, to, result)
            .ok()
    };

    let verdict = if handle.turn_disable_tools() {
        Verdict::Deny {
            reason: format!(
                "`{}` is unavailable: this turn has tools disabled (chat-only)",
                descriptor.name
            ),
        }
    } else {
        deps.policy.evaluate(
            descriptor,
            &request.input,
            &meta.cwd,
            handle.turn_permission_mode(),
        )
    };
    match verdict {
        Verdict::Deny { reason } => {
            tracing::info!(
                target: "tool",
                tool = %descriptor.name,
                call_id = %request.id,
                reason = %reason,
                "tool denied by policy"
            );
            if let Some(call) = transition(
                ToolCallStatus::Denied {
                    reason: Some(reason),
                },
                None,
            ) {
                emit_update(call).await;
            }
            return PermissionGate::Stop;
        }
        Verdict::Ask => {
            let request_id = PermissionRequestId::generate();
            tracing::info!(
                target: "tool",
                tool = %descriptor.name,
                call_id = %request.id,
                request_id = %request_id,
                "permission requested"
            );
            if let Some(call) = transition(
                ToolCallStatus::AwaitingPermission {
                    request_id: request_id.clone(),
                },
                None,
            ) {
                emit_update(call).await;
            }
            let detail = serde_json::to_string_pretty(&request.input).ok().map(|s| {
                if s.len() > 2000 {
                    let mut end = 2000;
                    while end > 0 && !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}…", &s[..end])
                } else {
                    s
                }
            });
            let _ = handle
                .emit_persistent(
                    Some(turn_id),
                    AgentEvent::PermissionRequested {
                        id: request_id.clone(),
                        call_id: Some(request.id.clone()),
                        title: format!("Allow `{}`?", descriptor.name),
                        detail,
                        options: vec![
                            PermissionDecisionKind::AllowOnce,
                            PermissionDecisionKind::AllowAlways,
                            PermissionDecisionKind::Deny,
                        ],
                    },
                )
                .await;

            let wait = deps
                .pending_permissions
                .wait(request_id.clone(), deps.policy.ask_timeout);
            let decision = tokio::select! {
                decision = wait => decision,
                _ = cancel.cancelled() => None,
            }
            .unwrap_or(PermissionDecision::Deny {
                reason: Some("permission request timed out or was interrupted".to_owned()),
            });

            tracing::info!(
                target: "tool",
                tool = %descriptor.name,
                call_id = %request.id,
                request_id = %request_id,
                decision = ?decision,
                "permission resolved"
            );

            let _ = handle
                .emit_persistent(
                    Some(turn_id),
                    AgentEvent::PermissionResolved {
                        id: request_id,
                        decision: decision.clone(),
                    },
                )
                .await;

            match decision {
                PermissionDecision::Deny { reason } => {
                    if let Some(call) = transition(ToolCallStatus::Denied { reason }, None) {
                        emit_update(call).await;
                    }
                    return PermissionGate::Stop;
                }
                PermissionDecision::AllowAlways => {
                    deps.policy.add_rule(PermissionPolicy::rule_for_always(
                        descriptor,
                        &request.input,
                    ));
                }
                PermissionDecision::AllowOnce => {}
                _ => {
                    if let Some(call) = transition(
                        ToolCallStatus::Denied {
                            reason: Some("unrecognized permission decision".to_owned()),
                        },
                        None,
                    ) {
                        emit_update(call).await;
                    }
                    return PermissionGate::Stop;
                }
            }
        }
        Verdict::Allow => {}
    }

    PermissionGate::Allow
}
