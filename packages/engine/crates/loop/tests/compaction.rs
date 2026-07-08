//! Compaction reduces the message count sent to the model on subsequent turns.

use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, ContentBlock, PromptInput, TurnOptions, TurnStopReason, reduce,
};
use agentloop_core::{Agent, ProviderRegistry, SessionStore};
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};

fn provider_registry(provider: Arc<MockProvider>) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(provider);
    providers
}

fn default_model() -> agentloop_contracts::ModelRef {
    agentloop_contracts::ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
}

#[tokio::test]
async fn compaction_reduces_messages_sent_to_provider() {
    let summary_text = "User asked about foo; assistant explained bar.";
    let provider = Arc::new(MockProvider::with_turns([
        MockProvider::text_turn("first reply"),
        MockProvider::text_turn("second reply"),
        MockProvider::text_turn(summary_text),
        MockProvider::text_turn("after compact"),
    ]));
    let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider.clone()))
        .tools(agentloop_tools::base_tools().registry)
        .system_prompt("test")
        .default_model(default_model())
        .build();

    let session = agent
        .create_session(agentloop_contracts::NewSessionParams::default())
        .await
        .expect("session");

    agent
        .prompt(
            &session,
            PromptInput::text("message one"),
            TurnOptions::default(),
        )
        .await
        .expect("turn 1");
    agent
        .prompt(
            &session,
            PromptInput::text("message two"),
            TurnOptions::default(),
        )
        .await
        .expect("turn 2");

    let before_compact = provider.requests().len();
    assert_eq!(before_compact, 2);

    let summary = agent
        .compact(&session, TurnOptions::default())
        .await
        .expect("compact");
    assert!(summary.summary_markdown.contains(summary_text));

    agent
        .prompt(
            &session,
            PromptInput::text("message three"),
            TurnOptions::default(),
        )
        .await
        .expect("turn 3");

    let requests = provider.requests();
    assert_eq!(requests.len(), 4);

    let post_compact = &requests[3];
    assert!(
        post_compact.messages.len() <= 2,
        "expected compacted context, got {} messages",
        post_compact.messages.len()
    );
    let joined = post_compact
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            ContentBlock::Markdown { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains(summary_text));
    assert!(joined.contains("message three"));
    assert!(!joined.contains("message one"));

    let events = store.read(&session, 0).await.expect("events");
    let transcript = reduce(events.iter().map(|(_, event)| event).collect::<Vec<_>>());
    let (_, tail) = transcript.context_view();
    assert_eq!(tail.len(), 2);
    assert!(matches!(
        tail[0].blocks.first(),
        Some(agentloop_contracts::TranscriptBlock::Markdown { text })
            if text == "message three"
    ));
}

#[tokio::test]
async fn compaction_boundary_event_is_persisted() {
    let provider = Arc::new(MockProvider::with_turns([
        MockProvider::text_turn("hi"),
        MockProvider::text_turn("summary of hi"),
    ]));
    let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider))
        .tools(agentloop_tools::base_tools().registry)
        .system_prompt("test")
        .default_model(default_model())
        .build();

    let session = agent
        .create_session(agentloop_contracts::NewSessionParams::default())
        .await
        .expect("session");
    let summary = agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .expect("turn");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    agent
        .compact(&session, TurnOptions::default())
        .await
        .expect("compact");

    let events = store.read(&session, 0).await.expect("events");
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::CompactionBoundary { summary } if !summary.summary_markdown.is_empty()
    )));
}
