//! Rebuild provider messages from a materialized transcript.
//!
//! The transcript folds tool results into `ToolCall` records; providers need
//! the classic shape back — `ToolUse` blocks in assistant messages and
//! `ToolResult` blocks in a following user message. This is the inverse of the
//! reducer's folding, applied to the compaction-aware context view.

use agentloop_contracts::{ContentBlock, Message, Role, Transcript, TranscriptBlock};

use crate::tool_results::output_or_synthetic;

/// Build the provider-facing message list for the next model call.
pub fn transcript_to_messages(transcript: &Transcript) -> Vec<Message> {
    let (compaction, items) = transcript.context_view();
    let mut messages = Vec::with_capacity(items.len() + 1);

    if let Some(summary) = compaction {
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::markdown(format!(
                "Summary of the conversation so far (earlier turns were compacted):\n\n{}",
                summary.summary_markdown
            ))],
            cache_hint: false,
        });
    }

    for item in items {
        let mut content: Vec<ContentBlock> = Vec::with_capacity(item.blocks.len());
        let mut results: Vec<ContentBlock> = Vec::new();

        for block in &item.blocks {
            match block {
                TranscriptBlock::Markdown { text } => {
                    content.push(ContentBlock::Markdown { text: text.clone() });
                }
                TranscriptBlock::Thinking { text, signature } => {
                    content.push(ContentBlock::Thinking {
                        text: text.clone(),
                        signature: signature.clone(),
                    });
                }
                TranscriptBlock::Image { media_type, data } => {
                    content.push(ContentBlock::Image {
                        media_type: media_type.clone(),
                        data: data.clone(),
                    });
                }
                TranscriptBlock::File {
                    name,
                    media_type,
                    data,
                } => {
                    content.push(ContentBlock::File {
                        name: name.clone(),
                        media_type: media_type.clone(),
                        data: data.clone(),
                    });
                }
                TranscriptBlock::Opaque { provider, data } => {
                    content.push(ContentBlock::Opaque {
                        provider: provider.clone(),
                        data: data.clone(),
                    });
                }
                TranscriptBlock::ToolCall(call) => {
                    content.push(ContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.tool_name.clone(),
                        input: call.input.clone(),
                    });
                    // Every requested call needs a result message or the
                    // conversation is invalid for tool-use APIs. Unresolved
                    // calls (cancelled turns) get a synthetic error result.
                    let (blocks, is_error) = output_or_synthetic(
                        call.result.as_ref(),
                        &call.status,
                        "The tool call did not complete (turn was interrupted).",
                        false,
                    );
                    results.push(ContentBlock::ToolResult {
                        tool_use_id: call.id.clone(),
                        content: blocks,
                        is_error,
                    });
                }
                // Unknown future block kinds cannot be re-encoded for a
                // provider; skip them rather than corrupt the request.
                _ => {}
            }
        }

        if !content.is_empty() {
            messages.push(Message {
                role: item.role,
                content,
                cache_hint: false,
            });
        }
        if !results.is_empty() {
            messages.push(Message {
                role: Role::User,
                content: results,
                cache_hint: false,
            });
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::*;

    #[test]
    fn rebuilds_tool_roundtrip_shape() {
        let call = ToolCall {
            id: ToolCallId::from("c1"),
            session_id: SessionId::from("s1"),
            turn_id: TurnId::from("t1"),
            message_id: MessageId::from("m2"),
            tool_name: "echo".to_owned(),
            input: serde_json::json!({"text": "hi"}),
            read_only: true,
            origin: ToolCallOrigin::Model,
            status: ToolCallStatus::Completed,
            timing: ToolCallTiming::default(),
            result: Some(ToolOutput::text("hi")),
        };
        let events = [
            AgentEvent::UserMessage {
                message_id: MessageId::from("m1"),
                content: vec![ContentBlock::markdown("say hi")],
            },
            AgentEvent::AssistantMessage {
                message_id: MessageId::from("m2"),
                content: vec![
                    ContentBlock::markdown("calling echo"),
                    ContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.tool_name.clone(),
                        input: call.input.clone(),
                    },
                ],
                model: None,
                usage: None,
            },
            AgentEvent::ToolCallUpdated { call },
        ];
        let messages = transcript_to_messages(&reduce(events.iter()));

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert!(matches!(
            messages[1].content[1],
            ContentBlock::ToolUse { .. }
        ));
        assert_eq!(messages[2].role, Role::User);
        let ContentBlock::ToolResult { is_error, .. } = &messages[2].content[0] else {
            panic!("expected tool result");
        };
        assert!(!is_error);
    }

    #[test]
    fn unresolved_call_gets_synthetic_error_result() {
        let events = [AgentEvent::AssistantMessage {
            message_id: MessageId::from("m1"),
            content: vec![ContentBlock::ToolUse {
                id: ToolCallId::from("c9"),
                name: "slow".to_owned(),
                input: serde_json::json!({}),
            }],
            model: None,
            usage: None,
        }];
        let messages = transcript_to_messages(&reduce(events.iter()));
        assert_eq!(messages.len(), 2);
        let ContentBlock::ToolResult { is_error, .. } = &messages[1].content[0] else {
            panic!("expected synthetic result");
        };
        assert!(is_error);
    }

    #[test]
    fn compaction_summary_becomes_leading_user_message() {
        let events = [
            AgentEvent::UserMessage {
                message_id: MessageId::from("m1"),
                content: vec![ContentBlock::markdown("old stuff")],
            },
            AgentEvent::CompactionBoundary {
                summary: CompactionSummary {
                    summary_markdown: "did old stuff".to_owned(),
                    strategy: "summarize_oldest".to_owned(),
                    tokens_before: None,
                    tokens_after: None,
                },
            },
            AgentEvent::UserMessage {
                message_id: MessageId::from("m2"),
                content: vec![ContentBlock::markdown("new question")],
            },
        ];
        let messages = transcript_to_messages(&reduce(events.iter()));
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::Markdown { text } if text.contains("did old stuff")
        ));
        assert!(matches!(
            &messages[1].content[0],
            ContentBlock::Markdown { text } if text == "new question"
        ));
    }
}
