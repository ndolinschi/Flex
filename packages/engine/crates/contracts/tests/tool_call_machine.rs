mod common;

use agentloop_contracts::{PermissionRequestId, ToolCallStatus};

fn states() -> Vec<ToolCallStatus> {
    vec![
        ToolCallStatus::Pending,
        ToolCallStatus::AwaitingPermission {
            request_id: PermissionRequestId::from("perm-1"),
        },
        ToolCallStatus::Running,
        ToolCallStatus::Completed,
        ToolCallStatus::Failed {
            error: "boom".to_owned(),
        },
        ToolCallStatus::Denied { reason: None },
        ToolCallStatus::Cancelled,
    ]
}

#[test]
fn transition_matrix() {
    let allowed: Vec<(&str, &str)> = states()
        .iter()
        .flat_map(|from| {
            states()
                .iter()
                .filter(|to| from.can_transition_to(to))
                .map(|to| (kind(from), kind(to)))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        allowed,
        vec![
            ("pending", "awaiting_permission"),
            ("pending", "running"),
            ("pending", "denied"),
            ("pending", "cancelled"),
            ("awaiting_permission", "running"),
            ("awaiting_permission", "denied"),
            ("awaiting_permission", "cancelled"),
            ("running", "completed"),
            ("running", "failed"),
            ("running", "cancelled"),
        ]
    );
}

#[test]
fn terminal_states_admit_nothing() {
    for from in states().iter().filter(|s| s.is_terminal()) {
        for to in states() {
            assert!(
                !from.can_transition_to(&to),
                "terminal {} must not transition to {}",
                kind(from),
                kind(&to)
            );
        }
    }
}

#[test]
fn timing_math() {
    let call = common::sample_tool_call(ToolCallStatus::Completed);
    assert_eq!(call.timing.duration_ms(), Some(45));
    assert_eq!(call.timing.total_ms(), Some(55));

    let running = common::sample_tool_call(ToolCallStatus::Running);
    assert_eq!(running.timing.duration_ms(), None);
    assert_eq!(running.timing.total_ms(), None);
}

fn kind(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::AwaitingPermission { .. } => "awaiting_permission",
        ToolCallStatus::Running => "running",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed { .. } => "failed",
        ToolCallStatus::Denied { .. } => "denied",
        ToolCallStatus::Cancelled => "cancelled",
        _ => panic!("unhandled status variant added — extend this test"),
    }
}
