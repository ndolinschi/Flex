//! MCP reload must not orphan in-memory sessions: the CLI reuses one native
//! session store across native service rebuilds.

use std::sync::Arc;

use agentloop_contracts::{ModelRef, NewSessionParams, PromptInput, TurnOptions};
use agentloop_core::ProviderRegistry;
use agentloop_engine::EngineService;
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::MockProvider;

fn mock_native_service(store: Arc<MemoryStore>) -> EngineService {
    let provider = Arc::new(MockProvider::with_id("mock"));
    provider.push_turn(MockProvider::text_turn("hello"));
    let mut registry = ProviderRegistry::new();
    registry.register(provider);
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(registry)
        .default_model(ModelRef::from("mock/mock-1"))
        .build();
    EngineService::new(agent, store)
}

#[tokio::test]
async fn shared_store_lets_second_service_resume_session() {
    let store = Arc::new(MemoryStore::new());
    let first = mock_native_service(store.clone());
    let session = first
        .create_session(NewSessionParams::default())
        .await
        .expect("create session");
    first
        .prompt(&session, PromptInput::text("hi"), TurnOptions::default())
        .await
        .expect("prompt");

    let second = mock_native_service(store);
    second
        .resume_session(&session)
        .await
        .expect("resume after rebuild");
    let transcript = second.session_items(&session).await.expect("transcript");
    assert!(
        !transcript.items.is_empty(),
        "session history should survive service rebuild"
    );
}
