use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};

use agentloop_contracts::{AgentEvent, SessionEvent};
use agentloop_engine::EngineService;

fn event_visible(event: &AgentEvent) -> bool {
    !matches!(
        event,
        AgentEvent::MessageStarted { .. }
            | AgentEvent::MarkdownDelta { .. }
            | AgentEvent::ThinkingDelta { .. }
            | AgentEvent::TextSnapshot { .. }
            | AgentEvent::ToolArgsDelta { .. }
            | AgentEvent::ExecChunk { .. }
    )
}

fn to_sse_event(event: SessionEvent) -> Result<Event, axum::Error> {
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
