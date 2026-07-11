//! NDJSON stdio transport boundary.
//!
//! The runner composes an [`EngineService`];
//! this crate owns the wire framing. Each line is one serialized
//! [`SessionEvent`].

use std::path::PathBuf;

use futures::StreamExt;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use agentloop_contracts::{
    AgentEvent, ModelRef, NewSessionParams, PermissionMode, PromptInput, SessionEvent, TurnOptions,
    TurnSummary,
};
use agentloop_engine::{EngineService, EngineServiceError, OutputVerbosity};

/// Request served by the current headless NDJSON protocol.
#[derive(Debug, Clone)]
pub struct OneTurnRequest {
    pub prompt: String,
    pub title: Option<String>,
    pub cwd: Option<PathBuf>,
    pub permission_mode: PermissionMode,
    pub verbosity: OutputVerbosity,
    /// This session's fallback chain; empty defers to the engine's own
    /// default (see `EngineConfig.default_fallback_models`).
    pub fallback_models: Vec<ModelRef>,
}

impl OneTurnRequest {
    pub fn new(prompt: impl Into<String>, cwd: Option<PathBuf>) -> Self {
        let prompt = prompt.into();
        Self {
            title: Some(prompt.chars().take(60).collect()),
            prompt,
            cwd,
            permission_mode: PermissionMode::Plan,
            verbosity: OutputVerbosity::default(),
            fallback_models: Vec::new(),
        }
    }
}

/// Whether an [`AgentEvent`] should be emitted at the given verbosity level.
fn event_visible(event: &AgentEvent, verbosity: OutputVerbosity) -> bool {
    match verbosity {
        OutputVerbosity::High => true,
        OutputVerbosity::Medium => !matches!(
            event,
            AgentEvent::MarkdownDelta { .. }
                | AgentEvent::ThinkingDelta { .. }
                | AgentEvent::ToolArgsDelta { .. }
                | AgentEvent::ExecChunk { .. }
        ),
        OutputVerbosity::Low => matches!(
            event,
            AgentEvent::TurnStarted { .. }
                | AgentEvent::TurnCompleted { .. }
                | AgentEvent::AssistantMessage { .. }
                | AgentEvent::UserMessage { .. }
                | AgentEvent::ToolCallUpdated { .. }
                | AgentEvent::PermissionRequested { .. }
                | AgentEvent::PermissionResolved { .. }
                | AgentEvent::SessionError { .. }
                | AgentEvent::ModelFallback { .. }
                | AgentEvent::RetryScheduled { .. }
                | AgentEvent::SubagentEvent { .. }
                | AgentEvent::SubagentCompleted { .. }
                | AgentEvent::CompactionBoundary { .. }
                | AgentEvent::Gap { .. }
        ),
    }
}

/// Transport-level failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StdioTransportError {
    #[error(transparent)]
    Engine(#[from] EngineServiceError),
    #[error("cannot serialize NDJSON event: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("cannot write NDJSON event: {0}")]
    Io(#[from] std::io::Error),
    #[error("prompt task failed to join: {0}")]
    Join(String),
}

pub type StdioResult<T> = Result<T, StdioTransportError>;

/// Serialize one event as exactly one NDJSON line.
pub async fn write_event<W>(writer: &mut W, event: &SessionEvent) -> StdioResult<()>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

/// Serve a single prompt turn as NDJSON.
///
/// The initial `SessionCreated` and `EngineInfo` events are replayed before
/// the live turn begins. Live events then continue until `TurnCompleted`.
pub async fn serve_one_turn<W>(
    service: EngineService,
    request: OneTurnRequest,
    mut writer: W,
) -> StdioResult<TurnSummary>
where
    W: AsyncWrite + Unpin,
{
    let session = service
        .create_session(NewSessionParams {
            title: request.title,
            cwd: request.cwd,
            model: None,
            fallback_models: request.fallback_models,
            mode: None,
            permission_mode: None,
            isolation: None,
            role: None,
            extra: Default::default(),
        })
        .await?;
    let mut events = service.subscribe(&session)?;

    for event in service.replay(&session, 0).await? {
        write_event(&mut writer, &event).await?;
    }

    let prompt_service = service.clone();
    let prompt_session = session.clone();
    let prompt_input = PromptInput::text(request.prompt);
    let turn_opts = TurnOptions {
        permission_mode: Some(request.permission_mode),
        ..TurnOptions::default()
    };
    let verbosity = service.verbosity();
    let prompt_task = tokio::spawn(async move {
        prompt_service
            .prompt(&prompt_session, prompt_input, turn_opts)
            .await
    });

    while let Some(event) = events.next().await {
        if !event_visible(&event.payload, verbosity) {
            continue;
        }
        let completed = matches!(event.payload, AgentEvent::TurnCompleted { .. });
        write_event(&mut writer, &event).await?;
        if completed {
            break;
        }
    }

    prompt_task
        .await
        .map_err(|err| StdioTransportError::Join(err.to_string()))?
        .map_err(StdioTransportError::Engine)
}

#[cfg(test)]
mod tests {
    use agentloop_contracts::{AgentEvent, SessionId, now_ms};
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn write_event_frames_one_json_line() {
        let event = SessionEvent {
            session_id: SessionId::from("s1"),
            seq: 7,
            turn_id: None,
            ts_ms: now_ms(),
            payload: AgentEvent::Gap { from_seq: 3 },
        };
        let mut out = Vec::new();

        write_event(&mut out, &event).await.expect("write succeeds");

        assert_eq!(out.last(), Some(&b'\n'));
        let parsed: SessionEvent = serde_json::from_slice(&out).expect("line parses");
        assert_eq!(parsed.session_id, event.session_id);
        assert_eq!(parsed.seq, 7);
        assert!(matches!(parsed.payload, AgentEvent::Gap { from_seq: 3 }));
    }

    #[test]
    fn high_verbosity_shows_all_events() {
        assert!(event_visible(
            &AgentEvent::MarkdownDelta {
                message_id: agentloop_contracts::MessageId::generate(),
                text: "hi".into(),
            },
            OutputVerbosity::High,
        ));
        assert!(event_visible(
            &AgentEvent::Gap { from_seq: 0 },
            OutputVerbosity::High,
        ));
    }

    #[test]
    fn medium_verbosity_hides_streaming_deltas() {
        assert!(!event_visible(
            &AgentEvent::MarkdownDelta {
                message_id: agentloop_contracts::MessageId::generate(),
                text: "hi".into(),
            },
            OutputVerbosity::Medium,
        ));
        assert!(!event_visible(
            &AgentEvent::ThinkingDelta {
                message_id: agentloop_contracts::MessageId::generate(),
                text: "think".into(),
            },
            OutputVerbosity::Medium,
        ));
        assert!(!event_visible(
            &AgentEvent::ToolArgsDelta {
                call_id: agentloop_contracts::ToolCallId::generate(),
                json_fragment: "{}".into(),
            },
            OutputVerbosity::Medium,
        ));
    }

    #[test]
    fn medium_verbosity_shows_materialized_events() {
        assert!(event_visible(
            &AgentEvent::TurnStarted {
                turn_id: agentloop_contracts::TurnId::generate(),
            },
            OutputVerbosity::Medium,
        ));
        assert!(event_visible(
            &AgentEvent::Gap { from_seq: 0 },
            OutputVerbosity::Medium,
        ));
    }

    #[test]
    fn low_verbosity_only_shows_materialized_events() {
        assert!(event_visible(
            &AgentEvent::TurnStarted {
                turn_id: agentloop_contracts::TurnId::generate(),
            },
            OutputVerbosity::Low,
        ));
        assert!(!event_visible(
            &AgentEvent::MarkdownDelta {
                message_id: agentloop_contracts::MessageId::generate(),
                text: "hi".into(),
            },
            OutputVerbosity::Low,
        ));
        // Gap is present in Low verbosity (subscriber sync hint).
        assert!(event_visible(
            &AgentEvent::Gap { from_seq: 0 },
            OutputVerbosity::Low,
        ));
        // Intermediate events like MessageStarted are hidden.
        assert!(!event_visible(
            &AgentEvent::MessageStarted {
                message_id: agentloop_contracts::MessageId::generate(),
                role: agentloop_contracts::Role::Assistant,
            },
            OutputVerbosity::Low,
        ));
    }
}
