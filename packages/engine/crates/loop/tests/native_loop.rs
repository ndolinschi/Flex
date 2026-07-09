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

#[tokio::test]
async fn completed_turn_emits_a_workspace_snapshot() {
    use agentloop_testkit::MockWorkspaces;
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn("done")]));
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider))
        .tools(registry_with(Vec::<Arc<dyn Tool>>::new()))
        .workspace(mock.clone())
        .system_prompt("test agent")
        .default_model(default_model())
        .build();
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session");
    agent
        .prompt(&session, PromptInput::text("hi"), TurnOptions::default())
        .await
        .expect("turn succeeds");

    assert_eq!(mock.snapshot_calls(), 1, "one snapshot per completed turn");
    let events = store.read(&session, 0).await.expect("events");
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, AgentEvent::SnapshotCreated { .. })),
        "a SnapshotCreated event follows TurnCompleted"
    );
}

#[tokio::test]
async fn snapshot_unavailable_emits_no_event_but_completes() {
    use agentloop_testkit::MockWorkspaces;
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn("done")]));
    let store = Arc::new(MemoryStore::new());
    let mock = Arc::new(MockWorkspaces::new().without_snapshots());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider))
        .tools(registry_with(Vec::<Arc<dyn Tool>>::new()))
        .workspace(mock.clone())
        .system_prompt("test agent")
        .default_model(default_model())
        .build();
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session");
    agent
        .prompt(&session, PromptInput::text("hi"), TurnOptions::default())
        .await
        .expect("turn still succeeds");

    assert_eq!(mock.snapshot_calls(), 1, "snapshot was attempted");
    let events = store.read(&session, 0).await.expect("events");
    assert!(
        !events
            .iter()
            .any(|(_, e)| matches!(e, AgentEvent::SnapshotCreated { .. })),
        "no SnapshotCreated event when snapshots are unavailable"
    );
}

#[tokio::test]
async fn turn_without_a_workspace_backend_emits_no_snapshot() {
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn("done")]));
    let (agent, store) = create_agent(provider, Vec::new(), Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session");
    agent
        .prompt(&session, PromptInput::text("hi"), TurnOptions::default())
        .await
        .expect("turn succeeds");
    let events = store.read(&session, 0).await.expect("events");
    assert!(
        !events
            .iter()
            .any(|(_, e)| matches!(e, AgentEvent::SnapshotCreated { .. })),
        "no snapshot without a workspace backend"
    );
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
async fn force_ask_tool_still_asks_under_bypass_permissions() {
    // A governance checkpoint (e.g. the learning plugin's
    // require_human_approval) must survive --bypass-permissions, which would
    // otherwise auto-allow this tool with no prompt at all.
    let (turn, _ids) = MockProvider::tool_turn(&[("needs_permission", serde_json::json!({}))]);
    let provider = Arc::new(MockProvider::with_turns([
        turn,
        MockProvider::text_turn("approved after all"),
    ]));
    let store = Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(provider_registry(provider))
        .tools(registry_with(vec![Arc::new(PermissionTool)]))
        .system_prompt("You are a test agent.")
        .default_model(default_model())
        .policy(
            agentloop_loop::PermissionPolicy::new(PermissionMode::Default)
                .with_force_ask(["needs_permission".to_owned()]),
        )
        .build();
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

    let request_id = loop {
        let event = stream.next().await.expect("permission event arrives");
        if let AgentEvent::PermissionRequested { id, .. } = event.payload {
            break id;
        }
    };
    agent
        .respond_permission(&session, request_id, PermissionDecision::AllowOnce)
        .await
        .expect("permission response succeeds");

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
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
async fn session_level_fallback_chain_is_used_when_turn_options_specify_none() {
    use agentloop_testkit::ScriptedError;

    let failing = Arc::new(MockProvider::with_id("mock-a"));
    failing.push_turn(Err(ScriptedError::RateLimited {
        retry_after_ms: Some(1_000),
    }));
    let healthy = Arc::new(MockProvider::with_id("mock-b"));
    healthy.push_turn(MockProvider::text_turn("rescued via session default"));

    let store = Arc::new(MemoryStore::new());
    let mut providers = ProviderRegistry::new();
    providers.register(failing.clone());
    providers.register(healthy.clone());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(providers)
        .system_prompt("You are a test agent.")
        .default_model(ModelRef::from("mock-a/model-one"))
        .build();

    // The fallback chain comes from NewSessionParams, not TurnOptions —
    // the session-level default lets a client set it once instead of on
    // every prompt.
    let session = agent
        .create_session(NewSessionParams {
            fallback_models: vec![ModelRef::from("mock-b/model-two")],
            ..NewSessionParams::default()
        })
        .await
        .expect("session is created");
    let summary = agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .expect("turn survives the failing provider");

    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(
        healthy.requests().len(),
        1,
        "fallback model served from the session's own chain"
    );

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ModelFallback { from, to: Some(to), .. }
                if from.0 == "mock-a/model-one" && to.0 == "mock-b/model-two"
        )),
        "fallback event is persisted"
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

#[tokio::test]
async fn truncated_stream_without_terminal_event_surfaces_as_error_not_phantom_stop() {
    use agentloop_contracts::MessageId;
    use agentloop_core::ProviderStreamEvent;

    fn truncated_turn() -> agentloop_testkit::ScriptedTurn {
        Ok(vec![
            ProviderStreamEvent::MessageStart {
                message_id: MessageId::generate(),
                model: MOCK_MODEL.to_owned(),
            },
            ProviderStreamEvent::MarkdownDelta {
                text: "This response got cut off mid".to_owned(),
            },
        ])
    }
    let provider = Arc::new(MockProvider::with_turns(
        std::iter::repeat_with(truncated_turn).take(3),
    ));
    let (agent, store) = create_agent(provider.clone(), Vec::new(), Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let result = agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await;
    assert!(
        result.is_err(),
        "a stream that closes without MessageEnd/Usage must surface as an error, \
         not a phantom successful Stop(EndTurn)"
    );

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        !events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::AssistantMessage { content, .. }
                if content.iter().any(|block| matches!(
                    block,
                    agentloop_contracts::ContentBlock::Markdown { text }
                        if text.contains("cut off mid")
                ))
        )),
        "truncated partial content must not be persisted as a successful final answer"
    );
    assert!(
        events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::SessionError { .. })),
        "the truncated stream is recorded as a session error, like other provider failures"
    );
}

#[tokio::test]
async fn mid_stream_failure_is_retried_on_same_model_before_falling_back() {
    use agentloop_testkit::ScriptedError;

    let provider = Arc::new(MockProvider::with_turns(vec![
        Err(ScriptedError::Stream(
            "connection reset mid-frame".to_owned(),
        )),
        MockProvider::text_turn("recovered"),
    ]));
    let (agent, store) = create_agent(provider.clone(), Vec::new(), Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(&session, PromptInput::text("hello"), TurnOptions::default())
        .await
        .expect("the turn survives a single transient mid-stream failure via same-model retry");

    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(
        provider.requests().len(),
        2,
        "exactly one retry against the same model, not more"
    );

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        !events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::ModelFallback { .. })),
        "a same-model retry is not a fallback and must not be recorded as one"
    );
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::AssistantMessage { content, .. }
                if content.iter().any(|block| matches!(
                    block,
                    agentloop_contracts::ContentBlock::Markdown { text } if text == "recovered"
                ))
        )),
        "the recovered answer materializes once the retry succeeds"
    );
}

#[tokio::test]
async fn panic_in_tool_fails_call_not_turn() {
    use agentloop_testkit::PanickingTool;

    let (turn, ids) = MockProvider::tool_turn(&[("panicking", serde_json::json!({"text": "x"}))]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        turn,
        MockProvider::text_turn("recovered"),
    ]));
    let (agent, store) =
        create_agent(provider.clone(), vec![Arc::new(PanickingTool)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(
            &session,
            PromptInput::text("go panic"),
            TurnOptions::default(),
        )
        .await
        .expect("the turn survives a panicking tool");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id == ids[0]
                    && matches!(&call.status, ToolCallStatus::Failed { error } if error.contains("panicked"))
        )),
        "the panicking call fails with a panic message"
    );
    assert_eq!(
        provider.requests().len(),
        2,
        "the model saw the failure and continued"
    );
}

#[tokio::test]
async fn read_only_batch_runs_in_parallel_on_the_pool() {
    let (turn, _ids) = MockProvider::tool_turn(&[
        ("slow", serde_json::json!({"ms": 200})),
        ("slow", serde_json::json!({"ms": 200})),
        ("slow", serde_json::json!({"ms": 200})),
        ("slow", serde_json::json!({"ms": 200})),
    ]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        turn,
        MockProvider::text_turn("done"),
    ]));
    let (agent, _store) = create_agent(
        provider,
        vec![Arc::new(agentloop_testkit::SlowTool)],
        Vec::new(),
    )
    .await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let started = std::time::Instant::now();
    let summary = agent
        .prompt(&session, PromptInput::text("sleep"), TurnOptions::default())
        .await
        .expect("turn succeeds");
    let elapsed = started.elapsed();

    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(summary.num_tool_calls, 4);
    assert!(
        elapsed < std::time::Duration::from_millis(700),
        "4x200ms read-only calls overlap on the pool (took {elapsed:?})"
    );
}

/// A descriptor-only stand-in for the Task tool: the loop intercepts by name.
struct TaskStub;

#[async_trait::async_trait]
impl Tool for TaskStub {
    fn descriptor(&self) -> agentloop_core::ToolDescriptor {
        agentloop_core::ToolDescriptor {
            name: agentloop_core::tool::SUBAGENT_TOOL_NAME.to_owned(),
            description: "delegate to a subagent".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
            read_only: true,
            category: agentloop_core::ToolCategory::Agent,
            needs_permission: agentloop_core::PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: agentloop_core::ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, agentloop_core::ToolError> {
        unreachable!("the loop must intercept Task calls")
    }
}

#[tokio::test]
async fn task_call_spawns_child_and_returns_final_text() {
    let (turn, ids) = MockProvider::tool_turn(&[(
        "Agent",
        serde_json::json!({
            "role": "searcher",
            "description": "map the code",
            "prompt": "Find the answer and report it.",
        }),
    )]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        turn,
        MockProvider::text_turn("child result: 42"),
        MockProvider::text_turn("done"),
    ]));
    let (agent, store) = create_agent(provider.clone(), vec![Arc::new(TaskStub)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(
            &session,
            PromptInput::text("research this"),
            TurnOptions::default(),
        )
        .await
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    let child = events
        .iter()
        .find_map(|(_, event)| match event {
            AgentEvent::SubagentStarted {
                child_session,
                call_id,
                role,
                ..
            } => {
                assert_eq!(call_id.as_ref(), Some(&ids[0]));
                assert_eq!(role.as_deref(), Some("searcher"));
                Some(child_session.clone())
            }
            _ => None,
        })
        .expect("SubagentStarted is persisted in the parent log");
    assert!(
        events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::SubagentCompleted { child_session, .. } if *child_session == child)),
        "SubagentCompleted is persisted"
    );
    let child_events = store.read(&child, 0).await.expect("child log");
    assert!(
        child_events
            .iter()
            .any(|(_, event)| matches!(event, AgentEvent::AssistantMessage { .. })),
        "child materialized its answer"
    );
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id == ids[0]
                    && matches!(call.status, ToolCallStatus::Completed)
                    && call.result.as_ref().map(ToolOutput::render_text).unwrap_or_default().contains("child result: 42")
        )),
        "Task returned the child's final message"
    );
    assert_eq!(provider.requests().len(), 3);
}

struct VerifyStub;

#[async_trait::async_trait]
impl Tool for VerifyStub {
    fn descriptor(&self) -> agentloop_core::ToolDescriptor {
        agentloop_core::ToolDescriptor {
            name: agentloop_core::tool::VERIFIER_TOOL_NAME.to_owned(),
            description: "run an independent verifier".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
            read_only: true,
            category: agentloop_core::ToolCategory::Agent,
            needs_permission: agentloop_core::PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: agentloop_core::ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, agentloop_core::ToolError> {
        unreachable!("the loop must intercept Verify calls")
    }
}

struct SubmitVerdictStub;

#[async_trait::async_trait]
impl Tool for SubmitVerdictStub {
    fn descriptor(&self) -> agentloop_core::ToolDescriptor {
        agentloop_core::ToolDescriptor {
            name: agentloop_core::tool::SUBMIT_VERDICT_TOOL_NAME.to_owned(),
            description: "report a verification outcome".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
            read_only: true,
            category: agentloop_core::ToolCategory::Agent,
            needs_permission: agentloop_core::PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: agentloop_core::ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, agentloop_core::ToolError> {
        // Real behavior (structured verdict on the output) lives in
        // `agentloop_verifier::submit_verdict_tool`; this stub only needs to be
        // callable so the verifier's tool call round-trips in the test.
        agentloop_verifier::submit_verdict_tool()
            .run(_ctx, input)
            .await
    }
}

#[tokio::test]
async fn verify_call_spawns_a_verifier_and_carries_the_structured_verdict() {
    let (root_turn, root_ids) = MockProvider::tool_turn(&[(
        "Verify",
        serde_json::json!({
            "rubric": "The file exists and contains a greeting.",
            "artifacts": ["hello.txt"],
        }),
    )]);
    let (verifier_turn, verifier_ids) = MockProvider::tool_turn(&[(
        "SubmitVerdict",
        serde_json::json!({
            "outcome": "pass",
            "findings": ["hello.txt:1 contains a greeting, matching the rubric"],
            "confidence": 0.95,
        }),
    )]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        root_turn,
        verifier_turn,
        MockProvider::text_turn("verified: pass"),
        MockProvider::text_turn("done"),
    ]));
    let (agent, store) = create_agent(
        provider.clone(),
        vec![Arc::new(VerifyStub), Arc::new(SubmitVerdictStub)],
        Vec::new(),
    )
    .await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(
            &session,
            PromptInput::text("finish the task, then verify it"),
            TurnOptions::default(),
        )
        .await
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    assert_eq!(provider.requests().len(), 4);

    let events = store.read(&session, 0).await.expect("events replay");
    let child = events
        .iter()
        .find_map(|(_, event)| match event {
            AgentEvent::SubagentStarted {
                child_session,
                role,
                ..
            } => {
                assert_eq!(role.as_deref(), Some("verifier"));
                Some(child_session.clone())
            }
            _ => None,
        })
        .expect("SubagentStarted with role=verifier is persisted in the parent log");

    let child_events = store.read(&child, 0).await.expect("child log");
    assert!(
        child_events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.tool_name == agentloop_core::tool::SUBMIT_VERDICT_TOOL_NAME
                    && matches!(call.status, ToolCallStatus::Completed)
        )),
        "the verifier's SubmitVerdict call completed in its own log"
    );

    let verify_call = events
        .iter()
        .rev()
        .find_map(|(_, event)| match event {
            AgentEvent::ToolCallUpdated { call } if call.id == root_ids[0] => Some(call.clone()),
            _ => None,
        })
        .expect("the Verify call is recorded in the parent log");
    assert!(matches!(verify_call.status, ToolCallStatus::Completed));
    let structured = verify_call
        .result
        .as_ref()
        .and_then(|output| output.structured.clone())
        .expect("Verify's ToolOutput carries the extracted structured verdict");
    assert_eq!(structured["outcome"], "pass");
    assert_eq!(
        structured["findings"][0],
        "hello.txt:1 contains a greeting, matching the rubric"
    );

    // Sanity: the verifier's own SubmitVerdict call id is distinct from the
    // parent's Verify call id — they live in different sessions.
    assert_ne!(root_ids[0], verifier_ids[0]);
}

#[tokio::test]
async fn max_depth_denies_grandchild_spawn() {
    let (parent_turn, parent_ids) = MockProvider::tool_turn(&[(
        "Agent",
        serde_json::json!({
            "role": "worker",
            "description": "do some work",
            "prompt": "Spawn a helper and report back.",
        }),
    )]);
    let (child_turn, child_ids) = MockProvider::tool_turn(&[(
        "Agent",
        serde_json::json!({
            "role": "worker",
            "description": "grandchild work",
            "prompt": "This should never actually run.",
        }),
    )]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        parent_turn,
        child_turn,
        MockProvider::text_turn("child done, no grandchild spawned"),
        MockProvider::text_turn("parent done"),
    ]));
    let (agent, store) = create_agent(provider.clone(), vec![Arc::new(TaskStub)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");

    let summary = agent
        .prompt(
            &session,
            PromptInput::text("delegate this"),
            TurnOptions::default(),
        )
        .await
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    let child = events
        .iter()
        .find_map(|(_, event)| match event {
            AgentEvent::SubagentStarted {
                child_session,
                call_id,
                ..
            } if call_id.as_ref() == Some(&parent_ids[0]) => Some(child_session.clone()),
            _ => None,
        })
        .expect("the parent's Task call spawned exactly one child");

    assert!(
        !events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::SubagentStarted { child_session, .. } if *child_session != child
        )),
        "no grandchild subagent should have been started — max_depth must be enforced"
    );

    let child_events = store.read(&child, 0).await.expect("child log");
    assert!(
        child_events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id == child_ids[0]
                    && matches!(call.status, ToolCallStatus::Completed)
                    && call
                        .result
                        .as_ref()
                        .map(ToolOutput::render_text)
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains("max_depth")
        )),
        "the child's grandchild Task attempt must be denied with a max_depth error"
    );
    assert_eq!(provider.requests().len(), 4);
}

#[tokio::test]
async fn child_permission_relays_and_routes() {
    let (turn, ids) = MockProvider::tool_turn(&[(
        "Agent",
        serde_json::json!({
            "role": "worker",
            "description": "do protected work",
            "prompt": "Run the protected tool.",
        }),
    )]);
    let (child_turn, _) = MockProvider::tool_turn(&[("needs_permission", serde_json::json!({}))]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        turn,
        child_turn,
        MockProvider::text_turn("child finished protected work"),
        MockProvider::text_turn("done"),
    ]));
    let (agent, store) = create_agent(
        provider,
        vec![Arc::new(TaskStub), Arc::new(PermissionTool)],
        Vec::new(),
    )
    .await;
    assert!(
        agent.capabilities().subagents,
        "native agent advertises subagents"
    );
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
                PromptInput::text("delegate protected work"),
                TurnOptions {
                    permission_mode: Some(PermissionMode::Default),
                    ..TurnOptions::default()
                },
            )
            .await
    });

    let (child, request_id) = loop {
        let event = stream.next().await.expect("relayed ask arrives");
        if let AgentEvent::SubagentEvent {
            child_session,
            event,
        } = event.payload
        {
            if let AgentEvent::PermissionRequested { id, .. } = *event {
                break (child_session, id);
            }
        }
    };
    agent
        .respond_permission(&child, request_id.clone(), PermissionDecision::AllowOnce)
        .await
        .expect("responding on the child session unblocks it");

    loop {
        let event = stream.next().await.expect("relayed resolution arrives");
        if let AgentEvent::SubagentEvent { event, .. } = event.payload {
            if matches!(
                *event,
                AgentEvent::PermissionResolved { ref id, .. }
                    if id.as_str() == request_id.as_str()
            ) {
                break;
            }
        }
    }

    let summary = prompt_task
        .await
        .expect("prompt task joins")
        .expect("turn succeeds");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);
    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id == ids[0] && matches!(call.status, ToolCallStatus::Completed)
        )),
        "the parent Task call completed after the child's ask resolved"
    );
}

async fn split_agent(
    split: bool,
) -> (
    Arc<agentloop_loop::NativeAgent>,
    Arc<MemoryStore>,
    Arc<MockProvider>,
    Arc<MockProvider>,
) {
    use agentloop_loop::roles::RoleSpec;

    let mock_a = Arc::new(MockProvider::with_id("mock-a"));
    let (turn, _) = MockProvider::tool_turn(&[
        (
            "Agent",
            serde_json::json!({"role": "searcher", "description": "left", "prompt": "left half"}),
        ),
        (
            "Agent",
            serde_json::json!({"role": "searcher", "description": "right", "prompt": "right half"}),
        ),
    ]);
    mock_a.push_turn(turn);
    let mock_b = Arc::new(MockProvider::with_id("mock-b"));
    if split {
        mock_a.push_turn(MockProvider::text_turn("left result"));
        mock_b.push_turn(MockProvider::text_turn("right result"));
    } else {
        mock_a.push_turn(MockProvider::text_turn("left result"));
        mock_a.push_turn(MockProvider::text_turn("right result"));
    }
    mock_a.push_turn(MockProvider::text_turn("done"));

    let store = Arc::new(MemoryStore::new());
    let mut providers = ProviderRegistry::new();
    providers.register(mock_a.clone());
    providers.register(mock_b.clone());
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(providers)
        .tools(registry_with(vec![Arc::new(TaskStub)]))
        .roles(vec![RoleSpec {
            models: vec![ModelRef::from("mock-a/m1"), ModelRef::from("mock-b/m2")],
            split,
            ..RoleSpec::new("searcher")
        }])
        .system_prompt("You are a test agent.")
        .default_model(ModelRef::from("mock-a/model-parent"))
        .build();
    (agent, store, mock_a, mock_b)
}

async fn spawned_child_models(
    agent: &Arc<agentloop_loop::NativeAgent>,
    store: &Arc<MemoryStore>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let session = agent.create_session(NewSessionParams::default()).await?;
    let summary = agent
        .prompt(
            &session,
            PromptInput::text("split the work"),
            TurnOptions::default(),
        )
        .await?;
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await?;
    let children: Vec<_> = events
        .iter()
        .filter_map(|(_, event)| match event {
            AgentEvent::SubagentStarted { child_session, .. } => Some(child_session.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(children.len(), 2, "both Task calls spawned children");
    let mut models = Vec::new();
    for child in children {
        let meta = store.get_meta(&child).await?;
        models.push(meta.model.ok_or("child meta records its model")?.0);
    }
    Ok(models)
}

#[tokio::test]
async fn parallel_tasks_round_robin_models() {
    let (agent, store, mock_a, mock_b) = split_agent(true).await;
    let mut models = spawned_child_models(&agent, &store)
        .await
        .expect("children spawn and report models");
    models.sort();
    assert_eq!(
        models,
        vec!["mock-a/m1".to_owned(), "mock-b/m2".to_owned()],
        "split=true rotates the batch across the role chain"
    );
    assert_eq!(mock_b.requests().len(), 1, "second chain model served");
    assert_eq!(mock_a.requests().len(), 3);
}

#[tokio::test]
async fn split_false_pins_first_chain_model() {
    let (agent, store, _mock_a, mock_b) = split_agent(false).await;
    let models = spawned_child_models(&agent, &store)
        .await
        .expect("children spawn and report models");
    assert_eq!(
        models,
        vec!["mock-a/m1".to_owned(), "mock-a/m1".to_owned()],
        "split=false keeps every child on chain[0]"
    );
    assert!(mock_b.requests().is_empty());
}

#[tokio::test]
async fn unknown_role_teaches_and_turn_continues() {
    let (turn, ids) = MockProvider::tool_turn(&[(
        "Agent",
        serde_json::json!({
            "role": "nonexistent",
            "description": "x",
            "prompt": "y",
        }),
    )]);
    let provider = Arc::new(MockProvider::with_turns(vec![
        turn,
        MockProvider::text_turn("ok"),
    ]));
    let (agent, store) = create_agent(provider, vec![Arc::new(TaskStub)], Vec::new()).await;
    let session = agent
        .create_session(NewSessionParams::default())
        .await
        .expect("session is created");
    let summary = agent
        .prompt(&session, PromptInput::text("go"), TurnOptions::default())
        .await
        .expect("turn survives the bad role");
    assert_eq!(summary.stop_reason, TurnStopReason::EndTurn);

    let events = store.read(&session, 0).await.expect("events replay");
    assert!(
        events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ToolCallUpdated { call }
                if call.id == ids[0]
                    && call.result.as_ref().map(ToolOutput::render_text).unwrap_or_default().contains("Available roles")
        )),
        "bad role produces a teaching error result"
    );
}
