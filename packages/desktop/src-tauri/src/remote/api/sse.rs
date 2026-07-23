
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};

use agentloop_contracts::{AgentEvent, BlobSource, ContentBlock, SessionEvent};
use agentloop_engine::EngineService;

pub(crate) fn event_visible(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::MessageStarted { .. }
            | AgentEvent::UserMessage { .. }
            | AgentEvent::AssistantMessage { .. }
            | AgentEvent::MarkdownDelta { .. }
            | AgentEvent::TextSnapshot { .. }
            | AgentEvent::TurnStarted { .. }
            | AgentEvent::TurnCompleted { .. }
    )
}

fn scrub_blocks(blocks: Vec<ContentBlock>) -> Vec<ContentBlock> {
    blocks
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Markdown { text } => Some(ContentBlock::Markdown { text }),
            ContentBlock::Image { media_type, data } => match data {
                BlobSource::Base64 { data } => Some(ContentBlock::Image {
                    media_type,
                    data: BlobSource::Base64 { data },
                }),
                BlobSource::Url { url } => Some(ContentBlock::Image {
                    media_type,
                    data: BlobSource::Url { url },
                }),
                BlobSource::Path { .. } | _ => None,
            },
            ContentBlock::File { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | _ => None,
        })
        .collect()
}

fn scrub_payload(payload: AgentEvent) -> AgentEvent {
    match payload {
        AgentEvent::UserMessage {
            message_id,
            content,
        } => AgentEvent::UserMessage {
            message_id,
            content: scrub_blocks(content),
        },
        AgentEvent::AssistantMessage {
            message_id,
            content,
            model,
            usage,
        } => AgentEvent::AssistantMessage {
            message_id,
            content: scrub_blocks(content),
            model,
            usage,
        },
        other => other,
    }
}

fn to_sse_event(mut event: SessionEvent) -> Result<Event, axum::Error> {
    event.payload = scrub_payload(event.payload);
    Ok(Event::default()
        .event(event.payload.kind_name())
        .json_data(&event)
        .unwrap_or_else(|_| Event::default().data("{}")))
}

pub(crate) async fn session_events_stream(
    service: &EngineService,
    session: agentloop_contracts::SessionId,
    from_seq: u64,
) -> Result<
    Sse<impl Stream<Item = Result<Event, axum::Error>> + use<>>,
    agentloop_engine::EngineServiceError,
> {
    let backlog = service.replay(&session, from_seq).await?;
    let live = service.subscribe(&session)?;
    let stream = futures::stream::iter(backlog)
        .chain(live)
        .filter(|event| futures::future::ready(event_visible(&event.payload)))
        .map(to_sse_event);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::{event_visible, scrub_blocks};
    use agentloop_contracts::{AgentEvent, BlobSource, ContentBlock};

    #[test]
    fn hides_permission_events() {
        assert!(!event_visible(&AgentEvent::PermissionRequested {
            id: agentloop_contracts::PermissionRequestId::from("p".to_string()),
            call_id: None,
            title: "x".into(),
            detail: None,
            options: Vec::new(),
        }));
    }

    #[test]
    fn shows_user_message_events() {
        assert!(event_visible(&AgentEvent::UserMessage {
            message_id: agentloop_contracts::MessageId::from("m".to_string()),
            content: Vec::new(),
        }));
    }

    #[test]
    fn hides_tool_thinking_and_exec_events() {
        assert!(!event_visible(&AgentEvent::ThinkingDelta {
            message_id: agentloop_contracts::MessageId::from("m".to_string()),
            text: "secret".into(),
        }));
        assert!(!event_visible(&AgentEvent::ToolArgsDelta {
            call_id: agentloop_contracts::ToolCallId::from("c".to_string()),
            json_fragment: "{\"command\":".into(),
        }));
        assert!(!event_visible(&AgentEvent::ExecChunk {
            call_id: agentloop_contracts::ToolCallId::from("c".to_string()),
            stream: agentloop_contracts::ExecStream::Stdout,
            text: "leaked".into(),
        }));
    }

    #[test]
    fn scrub_drops_tool_use_and_local_paths() {
        let scrubbed = scrub_blocks(vec![
            ContentBlock::Markdown { text: "hi".into() },
            ContentBlock::ToolUse {
                id: agentloop_contracts::ToolCallId::from("c".to_string()),
                name: "Bash".into(),
                input: serde_json::json!({"command": "rm -rf /"}),
            },
            ContentBlock::Image {
                media_type: "image/png".into(),
                data: BlobSource::Path {
                    path: "/etc/passwd".into(),
                },
            },
        ]);
        assert_eq!(scrubbed.len(), 1);
        assert!(matches!(
            &scrubbed[0],
            ContentBlock::Markdown { text } if text == "hi"
        ));
    }
}
