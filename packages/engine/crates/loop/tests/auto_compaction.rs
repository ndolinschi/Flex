//! Auto-compaction triggers when estimated prompt tokens approach the context limit.

use std::sync::Arc;

use agentloop_contracts::{
    AgentEvent, ContentBlock, PromptInput, ProviderCaps, TurnOptions, TurnStopReason,
};
use agentloop_core::{Agent, ProviderRegistry, SessionStore, ToolRegistry};
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
async fn auto_compacts_when_near_context_limit() {
    let summary_text = "Prior conversation summarized.";
    let caps = ProviderCaps {
        max_context_tokens: Some(500),
        ..MockProvider::default_caps()
    };
    let mock = MockProvider::with_caps(caps);
    mock.push_turns([
        MockProvider::text_turn("first reply"),
        MockProvider::text_turn(summary_text),
        MockProvider::text_turn("after auto-compact"),
    ]);
    let provider = Arc::new(mock);
    let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider.clone()))
        .tools(ToolRegistry::new())
        .system_prompt("test")
        .default_model(default_model())
        .build();

    let session = agent
        .create_session(agentloop_contracts::NewSessionParams::default())
        .await
        .expect("session");

    agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .expect("turn 1");

    let long_prompt = "x".repeat(2_000);
    let summary = agent
        .prompt(
            &session,
            PromptInput::text(long_prompt),
            TurnOptions::default(),
        )
        .await
        .expect("turn 2");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events");
    let auto_boundary = events.iter().find_map(|(_, event)| match event {
        AgentEvent::CompactionBoundary { summary }
            if summary.strategy == "auto_summarize_oldest" =>
        {
            Some(summary.clone())
        }
        _ => None,
    });
    let boundary = auto_boundary.expect("auto compaction boundary");
    assert!(boundary.summary_markdown.contains(summary_text));

    let last_assistant = events.iter().rev().find_map(|(_, event)| match event {
        AgentEvent::AssistantMessage { content, .. } => Some(content.clone()),
        _ => None,
    });
    let content = last_assistant.expect("assistant reply");
    assert!(content.iter().any(|block| matches!(
        block,
        ContentBlock::Markdown { text } if text.contains("after auto-compact")
    )));
}
