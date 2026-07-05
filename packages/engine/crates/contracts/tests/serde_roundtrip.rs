//! Every event variant must survive a JSON round-trip unchanged, and the
//! wire shape of the full log is pinned by a snapshot — a churn in that
//! snapshot is a visible, reviewable protocol change.

mod common;

use agentloop_contracts::*;
use pretty_assertions::assert_eq;

fn all_variants() -> Vec<AgentEvent> {
    let mut events = common::sample_event_log();
    events.extend([
        AgentEvent::EngineInfo {
            agent_id: "native".to_owned(),
            capabilities: AgentCaps::default(),
            provider_session_id: Some("remote-42".to_owned()),
            resolution_trace: vec!["explicit --agent native".to_owned()],
        },
        AgentEvent::SessionError {
            error: EngineError::engine(ErrorCode::AuthMissing, "no API key configured"),
        },
        AgentEvent::MessageStarted {
            message_id: MessageId::from("msg-9"),
            role: Role::Assistant,
        },
        AgentEvent::MarkdownDelta {
            message_id: MessageId::from("msg-9"),
            text: "partial ".to_owned(),
        },
        AgentEvent::ThinkingDelta {
            message_id: MessageId::from("msg-9"),
            text: "hmm".to_owned(),
        },
        AgentEvent::TextSnapshot {
            message_id: MessageId::from("msg-9"),
            text: "full text so far".to_owned(),
        },
        AgentEvent::ToolArgsDelta {
            call_id: ToolCallId::from("call-1"),
            json_fragment: "{\"file".to_owned(),
        },
        AgentEvent::ToolProgress {
            call_id: ToolCallId::from("call-1"),
            note: "read 4kb".to_owned(),
        },
        AgentEvent::PlanUpdated {
            entries: vec![PlanEntry {
                content: "write tests".to_owned(),
                status: PlanStatus::InProgress,
            }],
        },
        AgentEvent::PermissionRequested {
            id: PermissionRequestId::from("perm-1"),
            call_id: Some(ToolCallId::from("call-1")),
            title: "Run `git push`?".to_owned(),
            detail: None,
            options: vec![
                PermissionDecisionKind::AllowOnce,
                PermissionDecisionKind::AllowAlways,
                PermissionDecisionKind::Deny,
            ],
        },
        AgentEvent::PermissionResolved {
            id: PermissionRequestId::from("perm-1"),
            decision: PermissionDecision::AllowOnce,
        },
        AgentEvent::QuestionRequested {
            id: QuestionId::from("q-1"),
            questions: vec![Question {
                header: "Approach".to_owned(),
                question: "Which storage backend?".to_owned(),
                options: vec![QuestionOption {
                    label: "jsonl".to_owned(),
                    description: Some("append-only file".to_owned()),
                }],
                multi_select: false,
                allow_custom: true,
            }],
        },
        AgentEvent::QuestionResolved {
            id: QuestionId::from("q-1"),
            answers: vec![Answer {
                question: "Which storage backend?".to_owned(),
                selected: vec!["jsonl".to_owned()],
            }],
        },
        AgentEvent::CommandExpanded {
            name: "review".to_owned(),
            args: "src/".to_owned(),
        },
        AgentEvent::CompactionBoundary {
            summary: CompactionSummary {
                summary_markdown: "Earlier: user asked for X, agent did Y.".to_owned(),
                strategy: "summarize_oldest".to_owned(),
                tokens_before: Some(90_000),
                tokens_after: Some(12_000),
            },
        },
        AgentEvent::HookFired {
            point: HookPoint::PreToolUse,
            outcome: HookOutcomeKind::Continue,
        },
        AgentEvent::SubagentStarted {
            call_id: None,
            role: Some("searcher".to_owned()),
            child_session: SessionId::from("sess-2"),
            task: "explore the repo".to_owned(),
        },
        AgentEvent::SubagentEvent {
            child_session: SessionId::from("sess-2"),
            event: Box::new(AgentEvent::MarkdownDelta {
                message_id: MessageId::from("msg-c1"),
                text: "child says hi".to_owned(),
            }),
        },
        AgentEvent::SubagentCompleted {
            child_session: SessionId::from("sess-2"),
            summary: TurnSummary {
                turn_id: TurnId::from("turn-c1"),
                stop_reason: TurnStopReason::EndTurn,
                usage: TokenUsage::default(),
                cost_usd: None,
                num_model_calls: 1,
                num_tool_calls: 0,
                duration_ms: 900,
            },
        },
        AgentEvent::Gap { from_seq: 17 },
        AgentEvent::Unknown {
            raw: serde_json::json!({"kind": "from_the_future", "x": 1}),
        },
    ]);
    events
}

#[test]
fn every_variant_roundtrips() {
    for event in all_variants() {
        let json = serde_json::to_value(&event).expect("serialize");
        let back: AgentEvent = serde_json::from_value(json).expect("deserialize");
        assert_eq!(event, back, "round-trip mismatch for {}", event.kind_name());
    }
}

#[test]
fn unknown_kind_is_lenient_not_fatal() {
    let alien = serde_json::json!({"kind": "hologram_projection", "payload": {"x": 1}});
    let event = AgentEvent::from_json_lenient(alien.clone());
    assert_eq!(event, AgentEvent::Unknown { raw: alien });
    // But a known kind parses normally through the lenient path.
    let known = serde_json::json!({"kind": "gap", "from_seq": 3});
    assert_eq!(
        AgentEvent::from_json_lenient(known),
        AgentEvent::Gap { from_seq: 3 }
    );
}

#[test]
fn persistence_classes_are_stable() {
    let ephemeral: Vec<&str> = all_variants()
        .iter()
        .filter(|e| !e.is_persistent())
        .map(|e| e.kind_name())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        ephemeral,
        vec![
            "gap",
            "markdown_delta",
            "message_started",
            "subagent_event",
            "text_snapshot",
            "thinking_delta",
            "tool_args_delta",
            "tool_progress",
        ]
    );
}

#[test]
fn wire_shape_snapshot() {
    let envelope: Vec<SessionEvent> = common::sample_event_log()
        .into_iter()
        .enumerate()
        .map(|(i, payload)| SessionEvent {
            session_id: SessionId::from("sess-1"),
            seq: i as u64,
            turn_id: Some(TurnId::from("turn-1")),
            ts_ms: 1_000 + i as u64,
            payload,
        })
        .collect();
    insta::assert_json_snapshot!("session_event_wire_shape", envelope);
}

#[test]
fn hello_snapshot() {
    let hello = Hello::new(AgentCaps::default());
    // Identity fields are redacted: the snapshot pins the wire shape, and the
    // brand string must not appear outside branding.rs (CI greps for leaks).
    insta::assert_json_snapshot!("hello_wire_shape", hello, {
        ".engine.name" => "[product]",
        ".engine.version" => "[version]",
    });
}
