//! Live smoke tests — run locally with credentials, never in CI.
//!
//! ```bash
//! AGENTLOOP_SMOKE=1 ANTHROPIC_API_KEY=... cargo test -p agentloop-cli-core smoke -- --ignored --nocapture
//! ```

use std::sync::Arc;

use agentloop_cli_core::{ModelCatalog, SessionController};
use agentloop_contracts::{
    AgentEvent, ModelRef, NewSessionParams, PromptInput, ProviderId, TurnOptions, TurnStopReason,
};
use agentloop_core::{ProviderRegistry, ToolRegistry};
use agentloop_engine::{EngineOptions, EngineService};
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::{MockProvider, SlowTool};
use futures::StreamExt;

fn smoke_enabled() -> bool {
    std::env::var("AGENTLOOP_SMOKE").is_ok_and(|value| !value.is_empty() && value != "0")
}

#[tokio::test]
#[ignore = "live API; set AGENTLOOP_SMOKE=1 and ANTHROPIC_API_KEY"]
async fn smoke_anthropic_streams_one_turn() {
    if !smoke_enabled() || std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let service = EngineService::native_all(EngineOptions {
        provider: Some("anthropic".to_owned()),
        ..EngineOptions::default()
    })
    .expect("native service with anthropic credentials");

    let (controller, mut events) = SessionController::open(service, NewSessionParams::default())
        .await
        .expect("open session");

    let prompt_task = tokio::spawn({
        let controller = controller;
        async move {
            controller
                .prompt(
                    PromptInput::text("Reply with one short markdown line."),
                    TurnOptions::default(),
                )
                .await
        }
    });

    let mut saw_markdown = false;
    while let Some(event) = events.next().await {
        if matches!(
            event.payload,
            AgentEvent::MarkdownDelta { .. } | AgentEvent::AssistantMessage { .. }
        ) {
            saw_markdown = true;
            break;
        }
    }

    let summary = prompt_task.await.expect("join").expect("turn completes");
    assert!(saw_markdown, "expected markdown streaming events");
    assert_ne!(summary.stop_reason, TurnStopReason::Error);
}

#[tokio::test]
#[ignore = "live API; set AGENTLOOP_SMOKE=1 and a Copilot GitHub token"]
async fn smoke_model_catalog_includes_copilot() {
    if !smoke_enabled() {
        return;
    }
    if std::env::var("COPILOT_GITHUB_TOKEN").is_err() && std::env::var("GITHUB_TOKEN").is_err() {
        return;
    }

    let service = EngineService::native_all(EngineOptions {
        provider: Some("copilot".to_owned()),
        ..EngineOptions::default()
    })
    .expect("native service with copilot credentials");

    let catalog = ModelCatalog::fetch(service.provider_registry()).await;
    assert!(
        catalog
            .entries
            .iter()
            .any(|entry| entry.provider == ProviderId::from("copilot")),
        "catalog should list copilot models: {:?}",
        catalog.errors
    );
}

#[tokio::test]
#[ignore = "live API; set AGENTLOOP_SMOKE=1, ANTHROPIC_API_KEY, and a Copilot token"]
async fn smoke_provider_switch_two_turns() {
    if !smoke_enabled() || std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }
    if std::env::var("COPILOT_GITHUB_TOKEN").is_err() && std::env::var("GITHUB_TOKEN").is_err() {
        return;
    }

    let service = EngineService::native_all(EngineOptions::default())
        .expect("native service with multiple providers");
    assert!(
        service
            .provider_registry()
            .get(&ProviderId::from("anthropic"))
            .is_some()
            && service
                .provider_registry()
                .get(&ProviderId::from("copilot"))
                .is_some(),
        "need anthropic and copilot in registry"
    );

    let (controller, _events) = SessionController::open(service, NewSessionParams::default())
        .await
        .expect("open session");

    controller
        .prompt(
            PromptInput::text("Say 'from anthropic' only."),
            TurnOptions {
                model: Some(ModelRef::from("anthropic/claude-sonnet-4-5")),
                ..TurnOptions::default()
            },
        )
        .await
        .expect("anthropic turn");

    let catalog = ModelCatalog::fetch(controller.service().provider_registry()).await;
    let copilot_model = catalog
        .entries
        .iter()
        .find(|entry| entry.provider == ProviderId::from("copilot"))
        .expect("copilot model in catalog")
        .model_ref();

    controller
        .prompt(
            PromptInput::text("Say 'from copilot' only."),
            TurnOptions {
                model: Some(copilot_model),
                ..TurnOptions::default()
            },
        )
        .await
        .expect("copilot turn");
}

#[tokio::test]
#[ignore = "live API; set AGENTLOOP_SMOKE=1 (uses mock provider, no API keys)"]
async fn smoke_cancel_mid_turn() {
    if !smoke_enabled() {
        return;
    }

    let (turn, _ids) = MockProvider::tool_turn(&[("slow", serde_json::json!({"ms": 60_000}))]);
    let provider = Arc::new(MockProvider::with_turns([turn]));
    let store = Arc::new(MemoryStore::new());
    let mut registry = ProviderRegistry::new();
    registry.register(provider);
    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(SlowTool));
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(registry)
        .tools(tool_registry)
        .default_model(ModelRef::from("mock/mock-1"))
        .build();
    let service = EngineService::new(agent, store);
    let session = service
        .create_session(NewSessionParams::default())
        .await
        .expect("session");
    let mut events = service.subscribe(&session).expect("subscribe");

    let service_for_prompt = service.clone();
    let session_for_prompt = session.clone();
    let prompt_task = tokio::spawn(async move {
        service_for_prompt
            .prompt(
                &session_for_prompt,
                PromptInput::text("sleep"),
                TurnOptions::default(),
            )
            .await
    });

    while let Some(event) = events.next().await {
        if matches!(
            event.payload,
            AgentEvent::ToolCallUpdated { call }
                if call.tool_name == "slow"
                    && matches!(call.status, agentloop_contracts::ToolCallStatus::Running)
        ) {
            break;
        }
    }

    service.cancel(&session).await.expect("cancel succeeds");

    let summary = prompt_task
        .await
        .expect("join")
        .expect("cancelled turn resolves");
    assert_eq!(summary.stop_reason, TurnStopReason::Cancelled);
}
