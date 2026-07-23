use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::content::{BlobSource, ContentBlock, Role};
use crate::event::AgentEvent;
use crate::ids::{MessageId, ProviderId, SessionId, ToolCallId, TurnId};
use crate::session::{CompactionSummary, TokenUsage};
use crate::tool_call::{ToolCall, ToolCallOrigin, ToolCallStatus, ToolCallTiming};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TranscriptBlock {
    Markdown {
        text: String,
    },
    Thinking {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolCall(Box<ToolCall>),
    Image {
        media_type: String,
        data: BlobSource,
    },
    File {
        name: String,
        media_type: String,
        data: BlobSource,
    },
    Opaque {
        provider: ProviderId,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TranscriptItem {
    pub message_id: MessageId,
    pub role: Role,
    pub blocks: Vec<TranscriptBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transcript {
    pub items: Vec<TranscriptItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_index: Option<usize>,
}

impl Transcript {
    pub fn total_usage(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        for item in &self.items {
            if let Some(usage) = &item.usage {
                total.add(usage);
            }
        }
        total
    }

    pub fn context_view(&self) -> (Option<&CompactionSummary>, &[TranscriptItem]) {
        match (&self.compaction, self.boundary_index) {
            (Some(summary), Some(idx)) if idx <= self.items.len() => {
                (Some(summary), &self.items[idx..])
            }
            _ => (None, &self.items[..]),
        }
    }
}

pub fn reduce<'a>(events: impl IntoIterator<Item = &'a AgentEvent> + Clone) -> Transcript {
    let mut calls: HashMap<ToolCallId, ToolCall> = HashMap::new();
    let mut session_id: Option<SessionId> = None;
    for event in events.clone() {
        match event {
            AgentEvent::SessionCreated { meta } => session_id = Some(meta.id.clone()),
            AgentEvent::ToolCallUpdated { call } => {
                calls.insert(call.id.clone(), call.clone());
            }
            _ => {}
        }
    }

    let mut transcript = Transcript::default();
    let mut current_turn: Option<TurnId> = None;
    for event in events {
        match event {
            AgentEvent::TurnStarted { turn_id } => current_turn = Some(turn_id.clone()),
            AgentEvent::UserMessage {
                message_id,
                content,
            } => {
                let blocks = map_blocks(content, message_id, &calls, &session_id, &current_turn)
                    .into_iter()
                    .collect::<Vec<_>>();
                if !blocks.is_empty() {
                    transcript.items.push(TranscriptItem {
                        message_id: message_id.clone(),
                        role: Role::User,
                        blocks,
                        model: None,
                        usage: None,
                    });
                }
            }
            AgentEvent::AssistantMessage {
                message_id,
                content,
                model,
                usage,
            } => {
                transcript.items.push(TranscriptItem {
                    message_id: message_id.clone(),
                    role: Role::Assistant,
                    blocks: map_blocks(content, message_id, &calls, &session_id, &current_turn),
                    model: model.clone(),
                    usage: *usage,
                });
            }
            AgentEvent::CompactionBoundary { summary } => {
                transcript.compaction = Some(summary.clone());
                transcript.boundary_index = Some(transcript.items.len());
            }
            _ => {}
        }
    }
    transcript
}

fn map_blocks(
    content: &[ContentBlock],
    message_id: &MessageId,
    calls: &HashMap<ToolCallId, ToolCall>,
    session_id: &Option<SessionId>,
    current_turn: &Option<TurnId>,
) -> Vec<TranscriptBlock> {
    let mut blocks = Vec::with_capacity(content.len());
    for block in content {
        match block {
            ContentBlock::Markdown { text } => {
                blocks.push(TranscriptBlock::Markdown { text: text.clone() });
            }
            ContentBlock::Thinking { text, signature } => blocks.push(TranscriptBlock::Thinking {
                text: text.clone(),
                signature: signature.clone(),
            }),
            ContentBlock::ToolUse { id, name, input } => {
                let call = calls.get(id).cloned().unwrap_or_else(|| ToolCall {
                    id: id.clone(),
                    session_id: session_id
                        .clone()
                        .unwrap_or_else(|| SessionId(String::new())),
                    turn_id: current_turn
                        .clone()
                        .unwrap_or_else(|| TurnId(String::new())),
                    message_id: message_id.clone(),
                    tool_name: name.clone(),
                    input: input.clone(),
                    read_only: false,
                    origin: ToolCallOrigin::Model,
                    status: ToolCallStatus::Pending,
                    timing: ToolCallTiming::default(),
                    result: None,
                });
                blocks.push(TranscriptBlock::ToolCall(Box::new(call)));
            }
            ContentBlock::ToolResult { .. } => {}
            ContentBlock::Image { media_type, data } => blocks.push(TranscriptBlock::Image {
                media_type: media_type.clone(),
                data: data.clone(),
            }),
            ContentBlock::File {
                name,
                media_type,
                data,
            } => blocks.push(TranscriptBlock::File {
                name: name.clone(),
                media_type: media_type.clone(),
                data: data.clone(),
            }),
            ContentBlock::Opaque { provider, data } => blocks.push(TranscriptBlock::Opaque {
                provider: provider.clone(),
                data: data.clone(),
            }),
        }
    }
    blocks
}
