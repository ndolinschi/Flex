//! Rebuild provider messages from a materialized transcript.
//!
//! The transcript folds tool results into `ToolCall` records; providers need
//! the classic shape back — `ToolUse` blocks in assistant messages and
//! `ToolResult` blocks in a following user message. This is the inverse of the
//! reducer's folding, applied to the compaction-aware context view.

use agentloop_contracts::{BlobSource, ContentBlock, Message, Role, Transcript, TranscriptBlock};
use base64::Engine as _;

use crate::tool_results::output_or_synthetic;

/// Media types whose payloads should be inlined as markdown for the model
/// rather than left as opaque `File` placeholders (providers only render a
/// `[file: …, base64 data]` stub for those).
fn is_text_media_type(media_type: &str) -> bool {
    let mt = media_type.trim().to_ascii_lowercase();
    let base = mt.split(';').next().unwrap_or(mt.as_str()).trim();
    base.starts_with("text/")
        || matches!(
            base,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/typescript"
                | "application/x-yaml"
                | "application/yaml"
                | "application/toml"
                | "application/sql"
                | "application/x-sh"
                | "application/x-python"
        )
}

/// Fence language hint from a filename extension (best-effort).
fn fence_lang(name: &str) -> &'static str {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "py" | "pyi" => "python",
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "go" => "go",
        "java" => "java",
        "md" | "markdown" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" | "htm" => "html",
        "css" => "css",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "c" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "h" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        _ => "",
    }
}

/// Expand a text-ish `File` blob into fenced markdown the model can read.
/// Returns `None` for binary / undecodable payloads (caller keeps `File`).
fn expand_text_file(name: &str, media_type: &str, data: &BlobSource) -> Option<ContentBlock> {
    if !is_text_media_type(media_type) {
        return None;
    }
    let BlobSource::Base64 { data: b64 } = data else {
        return None;
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let lang = fence_lang(name);
    Some(ContentBlock::markdown(format!(
        "Attached file `{name}`:\n\n```{lang}\n{text}\n```"
    )))
}

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
                    if let Some(expanded) = expand_text_file(name, media_type, data) {
                        content.push(expanded);
                    } else {
                        content.push(ContentBlock::File {
                            name: name.clone(),
                            media_type: media_type.clone(),
                            data: data.clone(),
                        });
                    }
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

    #[test]
    fn text_file_attachment_expands_to_markdown() {
        let body = "def main():\n    print('hi')\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
        let events = [AgentEvent::UserMessage {
            message_id: MessageId::from("m1"),
            content: vec![
                ContentBlock::markdown("what does this do?"),
                ContentBlock::File {
                    name: "pcms_cli.py".to_owned(),
                    media_type: "text/plain".to_owned(),
                    data: BlobSource::Base64 { data: b64 },
                },
            ],
        }];
        let messages = transcript_to_messages(&reduce(events.iter()));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content.len(), 2);
        let ContentBlock::Markdown { text } = &messages[0].content[1] else {
            panic!(
                "expected expanded markdown, got {:?}",
                messages[0].content[1]
            );
        };
        assert!(text.contains("pcms_cli.py"));
        assert!(text.contains("def main()"));
        assert!(text.contains("```python"));
        assert!(!text.contains("base64"));
    }

    #[test]
    fn binary_file_attachment_stays_file() {
        let b64 = base64::engine::general_purpose::STANDARD.encode([0u8, 1, 2, 255]);
        let events = [AgentEvent::UserMessage {
            message_id: MessageId::from("m1"),
            content: vec![ContentBlock::File {
                name: "blob.bin".to_owned(),
                media_type: "application/octet-stream".to_owned(),
                data: BlobSource::Base64 { data: b64 },
            }],
        }];
        let messages = transcript_to_messages(&reduce(events.iter()));
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::File { name, .. } if name == "blob.bin"
        ));
    }
}
