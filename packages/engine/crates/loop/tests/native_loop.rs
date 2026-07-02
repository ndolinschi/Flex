use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use pretty_assertions::assert_eq;

use agentloop_contracts::{
    AgentEvent, ContentBlock, HookPoint, ModelRef, NewSessionParams, PermissionDecision,
    PermissionMode, PromptInput, ProviderCaps, SessionEvent, ThinkingConfig, ToolCallStatus,
    ToolOutput, TurnOptions, TurnStopReason,
};
use agentloop_core::{
    Agent, Hook, HookContext, HookData, HookError, HookOutcome, PermissionHint, ProviderRegistry,
    SessionStore, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolRegistry,
};
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::{EchoTool, MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider, SlowTool};

fn registry_with(tools: Vec<Arc<dyn Tool>>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for tool in tools {
        registry.register(tool);
    }
    registry
}

fn provider_registry(provider: Arc<MockProvider>) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(provider);
    providers
}

fn default_model() -> ModelRef {
    ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
}

async fn create_agent(
    provider: Arc<MockProvider>,
    tools: Vec<Arc<dyn Tool>>,
    hooks: Vec<Arc<dyn Hook>>,
) -> (Arc<agentloop_loop::NativeAgent>, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider))
        .tools(registry_with(tools))
        .hooks(hooks)
        .system_prompt("You are a test agent.")
        .default_model(default_model())
        .build();
    (agent, store)
}

fn roundtrip_scenario() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testkit/scenarios/tool_roundtrip.json")
}

#[tokio::test]
async fn tool_roundtrip_feeds_result_back_to_model() {
    let provider =
        Arc::new(MockProvider::from_scenario_file(&roundtrip_scenario()).expect("scenario loads"));
    let (agent, store) = create_agent(provider.clone(), vec![Arc::new(EchoTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(
            &session,
            PromptInput::text("echo ping"),
            TurnOptions::default(),
        )
        .await
        .expect("turn succeeds");

    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(summary.num_model_calls, 2);
    assert_eq!(summary.num_tool_calls, 1);
    assert_eq!(provider.requests().len(), 2);

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::ToolCallUpdated { call }
            if call.tool_name == "echo"
                && matches!(call.status, ToolCallStatus::Completed)
                && call.result.as_ref().map(ToolOutput::render_text).as_deref() == Some("ping")
    )));
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::AssistantMessage { content, .. }
            if content.iter().any(|block| matches!(
                block,
                ContentBlock::Markdown { text } if text.contains("The echo tool returned: ping.")
            ))
    )));
}

#[derive(Debug)]
struct PermissionTool;

#[async_trait]
impl Tool for PermissionTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "needs_permission".to_owned(),
            description: "Test tool that always asks for permission.".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
            read_only: false,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("allowed"))
    }
}

#[tokio::test]
async fn permission_ask_can_be_resolved() {
    let (turn, _ids) = MockProvider::tool_turn(&[("needs_permission", serde_json::json!({}))]);
    let provider = Arc::new(MockProvider::with_turns([
        turn,
        MockProvider::text_turn("permission flow done"),
    ]));
    let (agent, store) = create_agent(provider, vec![Arc::new(PermissionTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let mut stream = agent.events(&session).expect("subscribe succeeds");
    let prompt_agent = agent.clone();
    let prompt_session = session.clone();
    let prompt_task = tokio::spawn(async move {
        prompt_agent
            .prompt(
                &prompt_session,
                PromptInput::text("run protected tool"),
                TurnOptions {
                    permission_mode: Some(PermissionMode::Default),
                    ..TurnOptions::default()
                },
            )
            .await
    });

    let request_id = loop {
        let event = stream.next().await.expect("permission event arrives");
        if let AgentEvent::PermissionRequested { id, .. } = event.payload {
            break id;
        }
    };
    agent
        .respond_permission(&session, request_id.clone(), PermissionDecision::AllowOnce)
        .await
        .expect("permission response succeeds");

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::PermissionResolved { id, decision }
            if id.as_str() == request_id.as_str() && matches!(decision, PermissionDecision::AllowOnce)
    )));
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::ToolCallUpdated { call }
            if call.tool_name == "needs_permission"
                && matches!(call.status, ToolCallStatus::Completed)
                && call.result.as_ref().map(ToolOutput::render_text).as_deref() == Some("allowed")
    )));
}

#[tokio::test]
async fn bypass_permissions_skips_ask() {
    let (turn, _ids) = MockProvider::tool_turn(&[("needs_permission", serde_json::json!({}))]);
    let provider = Arc::new(MockProvider::with_turns([
        turn,
        MockProvider::text_turn("bypass done"),
    ]));
    let (agent, store) = create_agent(provider, vec![Arc::new(PermissionTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let mut stream = agent.events(&session).expect("subscribe succeeds");
    let prompt_agent = agent.clone();
    let prompt_session = session.clone();
    let prompt_task = tokio::spawn(async move {
        prompt_agent
            .prompt(
                &prompt_session,
                PromptInput::text("run protected tool"),
                TurnOptions {
                    permission_mode: Some(PermissionMode::BypassPermissions),
                    ..TurnOptions::default()
                },
            )
            .await
    });

    while let Some(event) = stream.next().await {
        if matches!(event.payload, AgentEvent::PermissionRequested { .. }) {
            panic!("bypass mode must not surface permission prompts");
        }
        if matches!(event.payload, AgentEvent::TurnCompleted { .. }) {
            break;
        }
    }

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        !events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::PermissionRequested { .. }))
    );
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::ToolCallUpdated { call }
            if call.tool_name == "needs_permission"
                && matches!(call.status, ToolCallStatus::Completed)
    )));
}

#[tokio::test]
async fn set_turn_permission_mode_applies_to_later_tools_in_same_turn() {
    let (turn, _ids) = MockProvider::tool_turn(&[
        ("needs_permission", serde_json::json!({})),
        ("needs_permission", serde_json::json!({})),
    ]);
    let provider = Arc::new(MockProvider::with_turns([
        turn,
        MockProvider::text_turn("mid-turn bypass done"),
    ]));
    let (agent, store) = create_agent(provider, vec![Arc::new(PermissionTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let mut stream = agent.events(&session).expect("subscribe succeeds");
    let prompt_agent = agent.clone();
    let prompt_session = session.clone();
    let prompt_task = tokio::spawn(async move {
        prompt_agent
            .prompt(
                &prompt_session,
                PromptInput::text("run protected tools"),
                TurnOptions {
                    permission_mode: Some(PermissionMode::Default),
                    ..TurnOptions::default()
                },
            )
            .await
    });

    let mut permission_events = 0u32;
    while let Some(event) = stream.next().await {
        if let AgentEvent::PermissionRequested { id, .. } = &event.payload {
            permission_events += 1;
            agent
                .set_turn_permission_mode(&session, Some(PermissionMode::BypassPermissions))
                .expect("live permission update succeeds");
            agent
                .respond_permission(&session, id.clone(), PermissionDecision::AllowOnce)
                .await
                .expect("permission response succeeds");
        }
        if matches!(event.payload, AgentEvent::TurnCompleted { .. }) {
            break;
        }
    }

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(
        permission_events, 1,
        "only the first protected tool should prompt before bypass took effect"
    );

    let events = store.read(&session, 0).await.expect("events replay");
    let completed = events
        .iter()
        .filter(|(_, event)| {
            matches!(
                event,
                AgentEvent::ToolCallUpdated { call }
                    if call.tool_name == "needs_permission"
                        && matches!(call.status, ToolCallStatus::Completed)
            )
        })
        .count();
    assert_eq!(completed, 2);
}

#[tokio::test]
async fn cancellation_marks_in_flight_tool_cancelled() {
    let (turn, _ids) = MockProvider::tool_turn(&[("slow", serde_json::json!({"ms": 60_000}))]);
    let provider = Arc::new(MockProvider::with_turns([turn]));
    let (agent, store) = create_agent(provider, vec![Arc::new(SlowTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let mut stream = agent.events(&session).expect("subscribe succeeds");
    let prompt_agent = agent.clone();
    let prompt_session = session.clone();
    let prompt_task = tokio::spawn(async move {
        prompt_agent
            .prompt(
                &prompt_session,
                PromptInput::text("sleep"),
                TurnOptions::default(),
            )
            .await
    });

    loop {
        let SessionEvent { payload, .. } = stream.next().await.expect("tool event arrives");
        if matches!(
            payload,
            AgentEvent::ToolCallUpdated { call }
                if call.tool_name == "slow" && matches!(call.status, ToolCallStatus::Running)
        ) {
            break;
        }
    }
    agent.cancel(&session).await.expect("cancel succeeds");

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("cancelled turn resolves Ok");
    assert_eq!(summary.stop_reason, TurnStopReason::Cancelled);
    let events = store.read(&session, 0).await.expect("events replay");
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::ToolCallUpdated { call }
            if call.tool_name == "slow" && matches!(call.status, ToolCallStatus::Cancelled)
    )));
}

#[derive(Debug)]
struct AppendPromptHook;

#[async_trait]
impl Hook for AppendPromptHook {
    fn interests(&self) -> &[HookPoint] {
        &[HookPoint::UserPromptSubmit]
    }

    async fn on(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        if let HookData::UserPrompt { input } = &mut ctx.data {
            input.parts.push(ContentBlock::markdown("hook-added"));
            return Ok(HookOutcome::Mutated);
        }
        Err(HookError {
            point,
            message: "unexpected hook data".to_owned(),
        })
    }
}

#[tokio::test]
async fn prompt_submit_hook_can_mutate_user_message() {
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn("done")]));
    let (agent, store) = create_agent(provider, Vec::new(), vec![Arc::new(AppendPromptHook)]).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    agent
        .prompt(
            &session,
            PromptInput::text("original"),
            TurnOptions::default(),
        )
        .await
        .expect("turn succeeds");

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        AgentEvent::UserMessage { content, .. }
            if content.iter().any(|block| matches!(
                block,
                ContentBlock::Markdown { text } if text == "hook-added"
            ))
    )));
}

fn thinking_opts(budget_tokens: u32) -> TurnOptions {
    TurnOptions {
        thinking: Some(ThinkingConfig { budget_tokens }),
        ..TurnOptions::default()
    }
}

#[tokio::test]
async fn thinking_option_is_forwarded_to_thinking_capable_providers() {
    let provider = Arc::new(MockProvider::with_caps(ProviderCaps {
        thinking: true,
        ..MockProvider::default_caps()
    }));
    let (agent, _store) = create_agent(provider.clone(), Vec::new(), Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    agent
        .prompt(
            &session,
            PromptInput::text("think hard"),
            thinking_opts(2048),
        )
        .await
        .expect("turn succeeds");

    let requests = provider.requests();
    assert_eq!(
        requests[0].thinking,
        Some(ThinkingConfig {
            budget_tokens: 2048
        })
    );
}

#[tokio::test]
async fn thinking_option_is_dropped_for_non_thinking_providers() {
    // Default mock caps declare `thinking: false`.
    let provider = Arc::new(MockProvider::new());
    let (agent, _store) = create_agent(provider.clone(), Vec::new(), Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    agent
        .prompt(
            &session,
            PromptInput::text("think hard"),
            thinking_opts(2048),
        )
        .await
        .expect("turn succeeds");

    let requests = provider.requests();
    assert_eq!(requests[0].thinking, None);
}

#[tokio::test]
async fn provider_failure_falls_back_to_next_chain_model() {
    use agentloop_testkit::ScriptedError;

    let failing = Arc::new(MockProvider::with_id("mock-a"));
    failing.push_turn(Err(ScriptedError::RateLimited {
        retry_after_ms: Some(1_000),
    }));
    let healthy = Arc::new(MockProvider::with_id("mock-b"));
    healthy.push_turn(MockProvider::text_turn("rescued"));

    let store = Arc::new(MemoryStore::new());
    let mut providers = ProviderRegistry::new();
    providers.register(failing.clone());
    providers.register(healthy.clone());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(providers)
        .system_prompt("You are a test agent.")
        .default_model(ModelRef::from("mock-a/model-one"))
        .build();

    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let opts = TurnOptions {
        fallback_models: vec![ModelRef::from("mock-b/model-two")],
        ..TurnOptions::default()
    };
    let summary = agent
        .prompt(&session, PromptInput::text("hello"), opts)
        .await
        .expect("turn survives the failing provider");

    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(healthy.requests().len(), 1, "fallback model served");

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ModelFallback { from, to: Some(to), .. }
                if from.0 == "mock-a/model-one" && to.0 == "mock-b/model-two"
        )),
        "fallback event is persisted"
    );
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::AssistantMessage { content, .. }
                if content.iter().any(|block| matches!(
                    block,
                    agentloop_contracts::ContentBlock::Markdown { text } if text == "rescued"
                ))
        )),
        "the healthy provider's answer materializes"
    );
}

#[tokio::test]
async fn exhausted_chain_surfaces_the_error() {
    use agentloop_testkit::ScriptedError;

    let failing = Arc::new(MockProvider::with_id("mock-a"));
    failing.push_turn(Err(ScriptedError::RateLimited {
        retry_after_ms: None,
    }));
    let store = Arc::new(MemoryStore::new());
    let mut providers = ProviderRegistry::new();
    providers.register(failing);
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(providers)
        .system_prompt("You are a test agent.")
        .default_model(ModelRef::from("mock-a/model-one"))
        .build();

    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let result = agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await;
    assert!(result.is_err(), "no chain configured: the error surfaces");

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::ModelFallback { to: None, .. })),
        "exhaustion is recorded with to: None"
    );
}
