//! Golden tests for the pure reducer.

mod common;

use agentloop_contracts::*;
use pretty_assertions::assert_eq;

#[test]
fn tool_roundtrip_transcript_snapshot() {
    let events = common::sample_event_log();
    let transcript = reduce(events.iter());
    insta::assert_json_snapshot!("tool_roundtrip_transcript", transcript);
}

#[test]
fn tool_use_resolves_to_latest_call_record() {
    let events = common::sample_event_log();
    let transcript = reduce(events.iter());

    let item = transcript
        .items
        .iter()
        .find(|i| i.message_id.as_str() == "msg-2")
        .expect("assistant item");
    let call = item
        .blocks
        .iter()
        .find_map(|b| match b {
            TranscriptBlock::ToolCall(call) => Some(call),
            _ => None,
        })
        .expect("tool call block");
    assert_eq!(call.status, ToolCallStatus::Completed);
    assert_eq!(
        call.result.as_ref().map(|r| r.render_text()).as_deref(),
        Some("hello world")
    );
}

#[test]
fn tool_result_only_user_message_is_not_an_item() {
    let events = common::sample_event_log();
    let transcript = reduce(events.iter());
    assert!(
        transcript
            .items
            .iter()
            .all(|i| i.message_id.as_str() != "msg-3"),
        "a user message consisting only of tool results must not materialize"
    );
    assert_eq!(transcript.items.len(), 3);
}

#[test]
fn tool_use_without_update_synthesizes_pending_record() {
    let events = [AgentEvent::AssistantMessage {
        message_id: MessageId::from("msg-1"),
        content: vec![ContentBlock::ToolUse {
            id: ToolCallId::from("call-x"),
            name: "Bash".to_owned(),
            input: serde_json::json!({"command": "ls"}),
        }],
        model: None,
        usage: None,
    }];
    let transcript = reduce(events.iter());
    let TranscriptBlock::ToolCall(call) = &transcript.items[0].blocks[0] else {
        panic!("expected tool call block");
    };
    assert_eq!(call.status, ToolCallStatus::Pending);
    assert_eq!(call.tool_name, "Bash");
}

#[test]
fn compaction_boundary_shapes_context_view() {
    let mut events = common::sample_event_log();
    events.push(AgentEvent::CompactionBoundary {
        summary: CompactionSummary {
            summary_markdown: "Earlier: read hello.txt, it contained 'hello world'.".to_owned(),
            strategy: "summarize_oldest".to_owned(),
            tokens_before: None,
            tokens_after: None,
        },
    });
    events.push(AgentEvent::UserMessage {
        message_id: MessageId::from("msg-5"),
        content: vec![ContentBlock::markdown("And now delete it")],
    });

    let transcript = reduce(events.iter());
    assert_eq!(transcript.items.len(), 4);
    let (summary, tail) = transcript.context_view();
    assert!(summary.is_some(), "compaction summary must be exposed");
    assert_eq!(tail.len(), 1, "only post-boundary items in context view");
    assert_eq!(tail[0].message_id.as_str(), "msg-5");
}

#[test]
fn usage_aggregates() {
    let events = common::sample_event_log();
    let transcript = reduce(events.iter());
    let usage = transcript.total_usage();
    assert_eq!((usage.input, usage.output), (300, 65));
}

#[test]
fn reduce_ignores_deltas_and_unknowns() {
    let events = [
        AgentEvent::MarkdownDelta {
            message_id: MessageId::from("m"),
            text: "x".to_owned(),
        },
        AgentEvent::Unknown {
            raw: serde_json::json!({"kind": "alien"}),
        },
        AgentEvent::Gap { from_seq: 5 },
    ];
    assert_eq!(reduce(events.iter()), Transcript::default());
}
