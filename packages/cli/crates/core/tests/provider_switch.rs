//! The load-bearing acceptance test for runtime model/provider switching:
//! with two providers in one registry, `TurnOptions.model` must route each
//! turn to the provider named in the qualified reference.

use std::sync::Arc;

use agentloop_contracts::{ModelRef, NewSessionParams, PromptInput, TurnOptions};
use agentloop_core::ProviderRegistry;
use agentloop_engine::EngineService;
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::MockProvider;

#[tokio::test]
async fn turn_options_model_routes_each_turn_to_the_named_provider() {
    let provider_a = Arc::new(MockProvider::with_id("mock-a"));
    let provider_b = Arc::new(MockProvider::with_id("mock-b"));
    provider_a.push_turn(MockProvider::text_turn("from a"));
    provider_b.push_turn(MockProvider::text_turn("from b"));

    let mut registry = ProviderRegistry::new();
    registry.register(provider_a.clone());
    registry.register(provider_b.clone());

    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(registry)
        .default_model(ModelRef::from("mock-a/model-one"))
        .build();
    let service = EngineService::new(agent, store);

    let session = service
        .create_session(NewSessionParams::default())
        .await
        .unwrap();

    let turn_a = TurnOptions {
        model: Some(ModelRef::from("mock-a/model-one")),
        ..TurnOptions::default()
    };
    service
        .prompt(&session, PromptInput::text("hello a"), turn_a)
        .await
        .unwrap();

    let turn_b = TurnOptions {
        model: Some(ModelRef::from("mock-b/model-two")),
        ..TurnOptions::default()
    };
    service
        .prompt(&session, PromptInput::text("hello b"), turn_b)
        .await
        .unwrap();

    let requests_a = provider_a.requests();
    let requests_b = provider_b.requests();
    assert_eq!(requests_a.len(), 1, "provider a serves exactly one turn");
    assert_eq!(requests_a[0].model, "model-one");
    assert_eq!(requests_b.len(), 1, "provider b serves exactly one turn");
    assert_eq!(requests_b[0].model, "model-two");
}

#[tokio::test]
async fn bare_model_refs_fall_back_to_priority_provider() {
    let provider_a = Arc::new(MockProvider::with_id("mock-a"));
    let provider_b = Arc::new(MockProvider::with_id("mock-b"));
    provider_a.push_turn(MockProvider::text_turn("from a"));

    let mut registry = ProviderRegistry::new();
    registry.register(provider_a.clone());
    registry.register(provider_b.clone());

    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(registry)
        .default_model(ModelRef::from("bare-model"))
        .build();
    let service = EngineService::new(agent, store);

    let session = service
        .create_session(NewSessionParams::default())
        .await
        .unwrap();
    service
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .unwrap();

    assert_eq!(provider_a.requests().len(), 1, "priority provider serves");
    assert!(provider_b.requests().is_empty());
    assert_eq!(provider_a.requests()[0].model, "bare-model");
}
