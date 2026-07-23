mod common;

use agentloop_contracts::*;

#[test]
fn markdown_projection_snapshot() {
    let mut events = common::sample_event_log();
    events.push(AgentEvent::CompactionBoundary {
        summary: CompactionSummary {
            summary_markdown: "Earlier: read hello.txt ('hello world').".to_owned(),
            strategy: "summarize_oldest".to_owned(),
            tokens_before: Some(500),
            tokens_after: Some(60),
            mode: None,
        },
    });
    events.push(AgentEvent::UserMessage {
        message_id: MessageId::from("msg-5"),
        content: vec![
            ContentBlock::markdown("Now attach this screenshot and the spec."),
            ContentBlock::Image {
                media_type: "image/png".to_owned(),
                data: BlobSource::Path {
                    path: "/tmp/shot.png".into(),
                },
            },
            ContentBlock::File {
                name: "spec.pdf".to_owned(),
                media_type: "application/pdf".to_owned(),
                data: BlobSource::Path {
                    path: "/tmp/spec.pdf".into(),
                },
            },
        ],
    });

    let transcript = reduce(events.iter());
    let rendered = markdown::transcript_to_markdown(&transcript);
    insta::assert_snapshot!("markdown_projection", rendered);
}

#[test]
fn failed_and_denied_calls_render_their_reason() {
    let mut call = common::sample_tool_call(ToolCallStatus::Failed {
        error: "timeout after 120s".to_owned(),
    });
    call.result = None;
    let events = [
        AgentEvent::AssistantMessage {
            message_id: MessageId::from("msg-2"),
            content: vec![ContentBlock::ToolUse {
                id: call.id.clone(),
                name: call.tool_name.clone(),
                input: call.input.clone(),
            }],
            model: None,
            usage: None,
        },
        AgentEvent::ToolCallUpdated { call },
    ];
    let rendered = markdown::transcript_to_markdown(&reduce(events.iter()));
    assert!(
        rendered.contains("failed: timeout after 120s"),
        "{rendered}"
    );
}
