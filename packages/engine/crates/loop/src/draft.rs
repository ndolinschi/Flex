//! Accumulate a provider stream into a complete assistant message while
//! surfacing live deltas.

use agentloop_contracts::{
    AgentEvent, ContentBlock, MessageId, StopReason, TokenUsage, ToolCallId,
};
use agentloop_core::provider::ProviderStreamEvent;

/// A parsed tool request from the finished message.
#[derive(Debug, Clone)]
pub(crate) struct DraftToolCall {
    pub(crate) id: ToolCallId,
    pub(crate) name: String,
    pub(crate) input: serde_json::Value,
}

/// The in-progress assistant message.
pub(crate) struct AssistantDraft {
    pub(crate) message_id: MessageId,
    pub(crate) model: Option<String>,
    blocks: Vec<DraftBlock>,
    pub(crate) usage: Option<TokenUsage>,
    pub(crate) stop_reason: Option<StopReason>,
}

enum DraftBlock {
    Markdown(String),
    Thinking {
        text: String,
        signature: Option<String>,
    },
    Tool {
        id: ToolCallId,
        name: String,
        args: String,
    },
}

impl AssistantDraft {
    pub(crate) fn new() -> Self {
        Self {
            message_id: MessageId::generate(),
            model: None,
            blocks: Vec::new(),
            usage: None,
            stop_reason: None,
        }
    }

    /// Apply one provider event; returns the ephemeral delta event to
    /// broadcast, if any.
    pub(crate) fn apply(&mut self, event: ProviderStreamEvent) -> Option<AgentEvent> {
        match event {
            ProviderStreamEvent::MessageStart { message_id, model } => {
                self.message_id = message_id;
                self.model = Some(model);
                Some(AgentEvent::MessageStarted {
                    message_id: self.message_id.clone(),
                    role: agentloop_contracts::Role::Assistant,
                })
            }
            ProviderStreamEvent::MarkdownDelta { text } => {
                match self.blocks.last_mut() {
                    Some(DraftBlock::Markdown(buffer)) => buffer.push_str(&text),
                    _ => self.blocks.push(DraftBlock::Markdown(text.clone())),
                }
                Some(AgentEvent::MarkdownDelta {
                    message_id: self.message_id.clone(),
                    text,
                })
            }
            ProviderStreamEvent::ThinkingDelta { text } => {
                match self.blocks.last_mut() {
                    Some(DraftBlock::Thinking { text: buffer, .. }) => buffer.push_str(&text),
                    _ => self.blocks.push(DraftBlock::Thinking {
                        text: text.clone(),
                        signature: None,
                    }),
                }
                Some(AgentEvent::ThinkingDelta {
                    message_id: self.message_id.clone(),
                    text,
                })
            }
            ProviderStreamEvent::ThinkingSignature { signature } => {
                if let Some(DraftBlock::Thinking {
                    signature: slot, ..
                }) = self
                    .blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, DraftBlock::Thinking { .. }))
                {
                    *slot = Some(signature);
                }
                None
            }
            ProviderStreamEvent::ToolCallStart { call_id, name } => {
                self.blocks.push(DraftBlock::Tool {
                    id: call_id,
                    name,
                    args: String::new(),
                });
                None
            }
            ProviderStreamEvent::ToolCallArgsDelta {
                call_id,
                json_fragment,
            } => {
                if let Some(DraftBlock::Tool { args, .. }) = self
                    .blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, DraftBlock::Tool { id, .. } if *id == call_id))
                {
                    args.push_str(&json_fragment);
                }
                Some(AgentEvent::ToolArgsDelta {
                    call_id,
                    json_fragment,
                })
            }
            ProviderStreamEvent::ToolCallEnd { .. } => None,
            ProviderStreamEvent::Usage(usage) => {
                self.usage = Some(usage);
                None
            }
            ProviderStreamEvent::MessageEnd { stop_reason } => {
                self.stop_reason = Some(stop_reason);
                None
            }
        }
    }

    /// The finished content blocks and parsed tool requests.
    pub(crate) fn finish(self) -> (Vec<ContentBlock>, Vec<DraftToolCall>) {
        let mut content = Vec::with_capacity(self.blocks.len());
        let mut calls = Vec::new();
        for block in self.blocks {
            match block {
                DraftBlock::Markdown(text) => {
                    if !text.is_empty() {
                        content.push(ContentBlock::Markdown { text });
                    }
                }
                DraftBlock::Thinking { text, signature } => {
                    content.push(ContentBlock::Thinking { text, signature });
                }
                DraftBlock::Tool { id, name, args } => {
                    let input = if args.trim().is_empty() {
                        serde_json::json!({})
                    } else {
                        serde_json::from_str(&args).unwrap_or(serde_json::Value::String(args))
                    };
                    content.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    calls.push(DraftToolCall { id, name, input });
                }
            }
        }
        (content, calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_interleaved_stream() {
        let mut draft = AssistantDraft::new();
        let events = [
            ProviderStreamEvent::MessageStart {
                message_id: MessageId::from("m1"),
                model: "mock-1".to_owned(),
            },
            ProviderStreamEvent::ThinkingDelta {
                text: "hmm ".to_owned(),
            },
            ProviderStreamEvent::ThinkingDelta {
                text: "ok".to_owned(),
            },
            ProviderStreamEvent::MarkdownDelta {
                text: "Let me ".to_owned(),
            },
            ProviderStreamEvent::MarkdownDelta {
                text: "check.".to_owned(),
            },
            ProviderStreamEvent::ToolCallStart {
                call_id: ToolCallId::from("c1"),
                name: "echo".to_owned(),
            },
            ProviderStreamEvent::ToolCallArgsDelta {
                call_id: ToolCallId::from("c1"),
                json_fragment: "{\"text\":".to_owned(),
            },
            ProviderStreamEvent::ToolCallArgsDelta {
                call_id: ToolCallId::from("c1"),
                json_fragment: "\"hi\"}".to_owned(),
            },
            ProviderStreamEvent::ToolCallEnd {
                call_id: ToolCallId::from("c1"),
            },
            ProviderStreamEvent::Usage(TokenUsage {
                input: 10,
                output: 5,
                ..Default::default()
            }),
            ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse,
            },
        ];
        for event in events {
            draft.apply(event);
        }
        assert_eq!(draft.stop_reason, Some(StopReason::ToolUse));
        let (content, calls) = draft.finish();
        assert_eq!(content.len(), 3);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "echo");
        assert_eq!(calls[0].input, serde_json::json!({"text": "hi"}));
    }

    #[test]
    fn interleaved_parallel_tool_calls_attribute_args_correctly() {
        let mut draft = AssistantDraft::new();
        draft.apply(ProviderStreamEvent::ToolCallStart {
            call_id: ToolCallId::from("call_a"),
            name: "Read".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallStart {
            call_id: ToolCallId::from("call_b"),
            name: "Write".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ToolCallId::from("call_a"),
            json_fragment: "{\"file_path\":\"a.txt\"".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ToolCallId::from("call_b"),
            json_fragment: "{\"file_path\":\"b.txt\"".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ToolCallId::from("call_a"),
            json_fragment: "}".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ToolCallId::from("call_b"),
            json_fragment: "}".to_owned(),
        });

        let (_, calls) = draft.finish();
        assert_eq!(calls.len(), 2);

        let call_a = calls
            .iter()
            .find(|c| c.id == ToolCallId::from("call_a"))
            .expect("call_a should be present");
        assert_eq!(call_a.name, "Read");
        assert_eq!(call_a.input, serde_json::json!({"file_path": "a.txt"}));

        let call_b = calls
            .iter()
            .find(|c| c.id == ToolCallId::from("call_b"))
            .expect("call_b should be present");
        assert_eq!(call_b.name, "Write");
        assert_eq!(call_b.input, serde_json::json!({"file_path": "b.txt"}));
    }

    #[test]
    fn malformed_tool_args_become_raw_string() {
        let mut draft = AssistantDraft::new();
        draft.apply(ProviderStreamEvent::ToolCallStart {
            call_id: ToolCallId::from("c1"),
            name: "echo".to_owned(),
        });
        draft.apply(ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ToolCallId::from("c1"),
            json_fragment: "{not json".to_owned(),
        });
        let (_, calls) = draft.finish();
        assert_eq!(
            calls[0].input,
            serde_json::Value::String("{not json".to_owned())
        );
    }
}
