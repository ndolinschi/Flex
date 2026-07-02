//! `MockProvider`: a fully scripted [`Provider`] for loop and engine tests.
//!
//! Each call to [`Provider::stream_chat`] records the incoming request and
//! plays back the next [`ScriptedTurn`] from a FIFO script. An `Ok` turn is a
//! sequence of [`ProviderStreamEvent`]s; an `Err` turn plays as a single
//! stream error. When the script is exhausted, the provider plays a default
//! end-of-turn message so tests never hang on a missing script entry.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use futures::stream;
use tokio_util::sync::CancellationToken;

use agentloop_core::contracts::{
    MessageId, ModelInfo, ProviderCaps, ProviderId, StopReason, TokenUsage, ToolCallId,
};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};

/// The stable provider key the mock reports from [`Provider::id`].
pub const MOCK_PROVIDER_ID: &str = "mock";

/// The single model the mock advertises and stamps on every message.
pub const MOCK_MODEL: &str = "mock-1";

/// One scripted model response: either a full event sequence or an error.
pub type ScriptedTurn = Result<Vec<ProviderStreamEvent>, ScriptedError>;

/// A scriptable provider failure. Cloneable (unlike [`ProviderError`]) so
/// scripts can be built up front; converted into the corresponding
/// [`ProviderError`] when the turn is played.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScriptedError {
    /// Plays as [`ProviderError::Http`].
    Http(String),
    /// Plays as [`ProviderError::RateLimited`].
    RateLimited { retry_after_ms: Option<u64> },
    /// Plays as [`ProviderError::Stream`].
    Stream(String),
    /// Plays as [`ProviderError::ContextOverflow`].
    ContextOverflow(String),
    /// Plays as [`ProviderError::InvalidRequest`].
    InvalidRequest(String),
    /// Plays as [`ProviderError::Cancelled`].
    Cancelled,
}

impl ScriptedError {
    /// The [`ProviderError`] this scripted failure plays as.
    pub fn into_provider_error(self) -> ProviderError {
        let provider = ProviderId::from(MOCK_PROVIDER_ID);
        match self {
            Self::Http(message) => ProviderError::Http { provider, message },
            Self::RateLimited { retry_after_ms } => ProviderError::RateLimited {
                provider,
                retry_after_ms,
            },
            Self::Stream(message) => ProviderError::Stream { provider, message },
            Self::ContextOverflow(message) => ProviderError::ContextOverflow { provider, message },
            Self::InvalidRequest(message) => ProviderError::InvalidRequest { provider, message },
            Self::Cancelled => ProviderError::Cancelled { provider },
        }
    }
}

impl From<ScriptedError> for ProviderError {
    fn from(value: ScriptedError) -> Self {
        value.into_provider_error()
    }
}

/// A scripted, request-recording [`Provider`] test double.
#[derive(Default)]
pub struct MockProvider {
    script: Mutex<VecDeque<ScriptedTurn>>,
    requests: Mutex<Vec<ChatRequest>>,
    /// Provider id override; [`MOCK_PROVIDER_ID`] when unset.
    id: Option<ProviderId>,
    /// Capability override; [`MockProvider::default_caps`] when unset.
    caps: Option<ProviderCaps>,
}

/// Test doubles must never deadlock a suite on a poisoned lock: a panic in
/// one assertion should not cascade into unrelated tests, so recover the data.
fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MockProvider {
    /// A provider with an empty script (every call plays the default turn).
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider preloaded with `turns`, played in order.
    pub fn with_turns(turns: impl IntoIterator<Item = ScriptedTurn>) -> Self {
        let provider = Self::new();
        provider.push_turns(turns);
        provider
    }

    /// A provider registering under `id` instead of [`MOCK_PROVIDER_ID`], so
    /// multi-provider tests can tell two mocks apart in one registry.
    pub fn with_id(id: impl Into<ProviderId>) -> Self {
        Self {
            id: Some(id.into()),
            ..Self::default()
        }
    }

    /// A provider advertising `caps` instead of [`MockProvider::default_caps`],
    /// so capability-gated code paths (e.g. thinking forwarding) can be
    /// exercised from tests.
    pub fn with_caps(caps: ProviderCaps) -> Self {
        Self {
            caps: Some(caps),
            ..Self::default()
        }
    }

    /// The capabilities advertised when no override is set.
    pub fn default_caps() -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            max_context_tokens: Some(1_000_000),
            ..ProviderCaps::default()
        }
    }

    /// Append one turn to the end of the script.
    pub fn push_turn(&self, turn: ScriptedTurn) {
        lock_unpoisoned(&self.script).push_back(turn);
    }

    /// Append several turns to the end of the script.
    pub fn push_turns(&self, turns: impl IntoIterator<Item = ScriptedTurn>) {
        lock_unpoisoned(&self.script).extend(turns);
    }

    /// How many scripted turns have not been played yet.
    pub fn remaining_turns(&self) -> usize {
        lock_unpoisoned(&self.script).len()
    }

    /// Every [`ChatRequest`] received so far, in call order.
    pub fn requests(&self) -> Vec<ChatRequest> {
        lock_unpoisoned(&self.requests).clone()
    }

    /// The token usage stamped on every generated turn (input 10 / output 5).
    pub fn default_usage() -> TokenUsage {
        TokenUsage {
            input: 10,
            output: 5,
            ..TokenUsage::default()
        }
    }

    fn message_start() -> ProviderStreamEvent {
        ProviderStreamEvent::MessageStart {
            message_id: MessageId::generate(),
            model: MOCK_MODEL.to_owned(),
        }
    }

    /// A plain text response ending the turn.
    pub fn text_turn(text: impl Into<String>) -> ScriptedTurn {
        Ok(vec![
            Self::message_start(),
            ProviderStreamEvent::MarkdownDelta { text: text.into() },
            ProviderStreamEvent::Usage(Self::default_usage()),
            ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn,
            },
        ])
    }

    /// A thinking block followed by a text response ending the turn.
    pub fn thinking_turn(thinking: impl Into<String>, text: impl Into<String>) -> ScriptedTurn {
        Ok(vec![
            Self::message_start(),
            ProviderStreamEvent::ThinkingDelta {
                text: thinking.into(),
            },
            ProviderStreamEvent::MarkdownDelta { text: text.into() },
            ProviderStreamEvent::Usage(Self::default_usage()),
            ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn,
            },
        ])
    }

    /// A turn requesting one tool call per `(tool_name, args)` pair, stopping
    /// with [`StopReason::ToolUse`]. Returns the generated [`ToolCallId`]s in
    /// pair order so tests can reference them in results and assertions.
    pub fn tool_turn(pairs: &[(&str, serde_json::Value)]) -> (ScriptedTurn, Vec<ToolCallId>) {
        Self::tool_turn_with_text(None, pairs)
    }

    /// Like [`Self::tool_turn`], with an optional markdown preamble emitted
    /// before the tool calls.
    pub fn tool_turn_with_text(
        preamble: Option<&str>,
        pairs: &[(&str, serde_json::Value)],
    ) -> (ScriptedTurn, Vec<ToolCallId>) {
        let mut events = vec![Self::message_start()];
        if let Some(text) = preamble {
            events.push(ProviderStreamEvent::MarkdownDelta {
                text: text.to_owned(),
            });
        }
        let mut call_ids = Vec::with_capacity(pairs.len());
        for (name, args) in pairs {
            let call_id = ToolCallId::generate();
            events.push(ProviderStreamEvent::ToolCallStart {
                call_id: call_id.clone(),
                name: (*name).to_owned(),
            });
            events.push(ProviderStreamEvent::ToolCallArgsDelta {
                call_id: call_id.clone(),
                json_fragment: args.to_string(),
            });
            events.push(ProviderStreamEvent::ToolCallEnd {
                call_id: call_id.clone(),
            });
            call_ids.push(call_id);
        }
        events.push(ProviderStreamEvent::Usage(Self::default_usage()));
        events.push(ProviderStreamEvent::MessageEnd {
            stop_reason: StopReason::ToolUse,
        });
        (Ok(events), call_ids)
    }

    /// The turn played when the script is empty.
    fn default_turn_events() -> Vec<ProviderStreamEvent> {
        Self::text_turn("Done.").unwrap_or_default()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> ProviderId {
        self.id
            .clone()
            .unwrap_or_else(|| ProviderId::from(MOCK_PROVIDER_ID))
    }

    fn capabilities(&self) -> ProviderCaps {
        self.caps.unwrap_or_else(Self::default_caps)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![ModelInfo {
            id: MOCK_MODEL.to_owned(),
            display_name: Some("Mock Model 1".to_owned()),
            context_window: Some(1_000_000),
            reasoning: true,
            vision: false,
        }])
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream, ProviderError> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled {
                provider: self.id(),
            });
        }
        lock_unpoisoned(&self.requests).push(request);
        let turn = lock_unpoisoned(&self.script).pop_front();
        let items: Vec<Result<ProviderStreamEvent, ProviderError>> = match turn {
            Some(Ok(events)) => events.into_iter().map(Ok).collect(),
            Some(Err(scripted)) => vec![Err(scripted.into_provider_error())],
            None => Self::default_turn_events().into_iter().map(Ok).collect(),
        };
        Ok(Box::pin(stream::iter(items)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn request(model: &str) -> ChatRequest {
        ChatRequest::new(model, Vec::new())
    }

    async fn play(provider: &MockProvider) -> Vec<ProviderStreamEvent> {
        let stream = provider
            .stream_chat(request(MOCK_MODEL), CancellationToken::new())
            .await
            .expect("stream_chat should succeed");
        stream
            .map(|item| item.expect("scripted Ok turns contain no Err items"))
            .collect()
            .await
    }

    fn markdown_of(events: &[ProviderStreamEvent]) -> String {
        events
            .iter()
            .filter_map(|event| match event {
                ProviderStreamEvent::MarkdownDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn plays_scripted_turns_in_pop_order() {
        let provider = MockProvider::with_turns([
            MockProvider::text_turn("one"),
            MockProvider::text_turn("two"),
        ]);
        assert_eq!(provider.remaining_turns(), 2);

        assert_eq!(markdown_of(&play(&provider).await), "one");
        assert_eq!(markdown_of(&play(&provider).await), "two");
        assert_eq!(provider.remaining_turns(), 0);
    }

    #[tokio::test]
    async fn plays_default_end_turn_when_script_is_empty() {
        let provider = MockProvider::new();
        let events = play(&provider).await;

        assert_eq!(events.len(), 4);
        assert!(matches!(
            &events[0],
            ProviderStreamEvent::MessageStart { model, .. } if model == MOCK_MODEL
        ));
        assert_eq!(
            events[1],
            ProviderStreamEvent::MarkdownDelta {
                text: "Done.".to_owned()
            }
        );
        assert_eq!(
            events[2],
            ProviderStreamEvent::Usage(MockProvider::default_usage())
        );
        assert_eq!(
            events[3],
            ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn
            }
        );
    }

    #[tokio::test]
    async fn records_every_request_in_call_order() {
        let provider = MockProvider::new();
        for model in ["model-a", "model-b"] {
            let _ = provider
                .stream_chat(request(model), CancellationToken::new())
                .await
                .expect("stream_chat should succeed");
        }

        let recorded = provider.requests();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].model, "model-a");
        assert_eq!(recorded[1].model, "model-b");
    }

    #[tokio::test]
    async fn scripted_error_plays_as_single_err_item() {
        let provider =
            MockProvider::with_turns([Err(ScriptedError::Http("connection reset".to_owned()))]);
        let stream = provider
            .stream_chat(request(MOCK_MODEL), CancellationToken::new())
            .await
            .expect("stream_chat itself succeeds; the error is a stream item");
        let items: Vec<_> = stream.collect().await;

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            Err(ProviderError::Http { message, .. }) if message == "connection reset"
        ));
    }

    #[tokio::test]
    async fn tool_turn_returns_ids_matching_emitted_events() {
        let (turn, ids) = MockProvider::tool_turn_with_text(
            Some("Calling tools."),
            &[
                ("echo", serde_json::json!({"text": "ping"})),
                ("slow", serde_json::json!({"ms": 1})),
            ],
        );
        let events = turn.expect("tool_turn is always Ok");
        assert_eq!(ids.len(), 2);

        let started: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ProviderStreamEvent::ToolCallStart { call_id, name } => {
                    Some((call_id.clone(), name.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![
                (ids[0].clone(), "echo".to_owned()),
                (ids[1].clone(), "slow".to_owned())
            ]
        );
        assert!(events.contains(&ProviderStreamEvent::ToolCallArgsDelta {
            call_id: ids[0].clone(),
            json_fragment: "{\"text\":\"ping\"}".to_owned(),
        }));
        assert_eq!(
            events.last(),
            Some(&ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse
            })
        );
    }

    #[tokio::test]
    async fn thinking_turn_emits_thinking_before_text() {
        let events =
            MockProvider::thinking_turn("pondering", "answer").expect("thinking_turn is Ok");
        assert!(matches!(
            &events[1],
            ProviderStreamEvent::ThinkingDelta { text } if text == "pondering"
        ));
        assert!(matches!(
            &events[2],
            ProviderStreamEvent::MarkdownDelta { text } if text == "answer"
        ));
    }

    #[tokio::test]
    async fn identity_capabilities_and_models() {
        let provider = MockProvider::new();
        assert_eq!(provider.id().as_str(), MOCK_PROVIDER_ID);

        let caps = provider.capabilities();
        assert!(caps.tool_use);
        assert!(caps.parallel_tool_use);
        assert_eq!(caps.max_context_tokens, Some(1_000_000));

        let models = provider.list_models().await.expect("list_models is Ok");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, MOCK_MODEL);
    }

    #[tokio::test]
    async fn pre_cancelled_token_short_circuits() {
        let provider = MockProvider::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = provider.stream_chat(request(MOCK_MODEL), cancel).await;
        assert!(matches!(result, Err(ProviderError::Cancelled { .. })));
    }
}
