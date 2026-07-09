//! `GET /sessions/{id}/events`: replay-then-tail as one SSE stream covering
//! every turn on the session, not just the one in flight.
//!
//! **Known race** (documented, not hidden): this calls `replay(from_seq)`
//! then `subscribe()` as two separate `EngineService` calls. An event
//! appended in the gap between them is missed — the same
//! append→broadcast-adjacent race the session actor (`packages/engine/crates/loop/src/actor.rs`)
//! fixed for a single session's *own* concurrent writers. Closing this
//! transport-level gap needs an atomic `subscribe_from(session, from_seq)` on
//! `EngineService`/the `Agent` trait, which the actor's `Subscribe` primitive
//! makes straightforward to add later; until then, a client that cares
//! should re-`GET` with the last-seen `seq` on reconnect to fill any gap.

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};

use agentloop_contracts::{AgentEvent, SessionEvent};
use agentloop_engine::EngineService;

/// Which events reach the client. Deliberately smaller than the engine's own
/// `OutputVerbosity` — this transport does not (yet) forward streaming token
/// deltas, only materialized/control-plane events.
fn event_visible(event: &AgentEvent) -> bool {
    !matches!(
        event,
        AgentEvent::MessageStarted { .. }
            | AgentEvent::MarkdownDelta { .. }
            | AgentEvent::ThinkingDelta { .. }
            | AgentEvent::TextSnapshot { .. }
            | AgentEvent::ToolArgsDelta { .. }
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
