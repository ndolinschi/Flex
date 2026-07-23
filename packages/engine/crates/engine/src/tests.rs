use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::*;
use agentloop_core::{ProviderRegistry, SessionStore};
use agentloop_loop::{LoopLimits, NativeAgentBuilder};
use agentloop_session::MemoryStore;
use agentloop_testkit::MockWorkspaces;

use crate::paths::resolve_max_iterations;
use crate::{EngineService, EngineServiceError};

#[test]
fn resolve_max_iterations_uses_configured_value_or_falls_back_to_loop_default() {
    assert_eq!(resolve_max_iterations(Some(2_000)), 2_000);
    assert_eq!(
        resolve_max_iterations(None),
        LoopLimits::default().max_iterations
    );
}

mod run_goal {
    use agentloop_testkit::{EchoTool, MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};

    use super::*;

    fn default_model() -> ModelRef {
        ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
    }

    fn goal_service(
        provider: Arc<MockProvider>,
        limits: LoopLimits,
    ) -> (EngineService, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore::new());
        let mut providers = ProviderRegistry::new();
        providers.register(provider);
        let mut tools = agentloop_core::ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let agent = NativeAgentBuilder::new(store.clone())
            .providers(providers)
            .tools(tools)
            .limits(limits)
            .system_prompt("test agent")
            .default_model(default_model())
            .build();
        (EngineService::new(agent, store.clone()), store)
    }

    fn spec(prompt: &str, max_iterations: u32, max_identical_failures: u32) -> GoalSpec {
        GoalSpec {
            prompt: prompt.to_owned(),
            max_iterations,
            max_identical_failures,
            token_budget: None,
            require_verification: false,
        }
    }

    #[tokio::test]
    async fn achieves_when_the_model_stops_calling_tools() {
        let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn(
            "all done, nothing left to do",
        )]));
        let (service, _store) = goal_service(provider, LoopLimits::default());
        let session = service
            .create_session(NewSessionParams::default())
            .await
            .expect("session");

        let outcome = service
            .run_goal(&session, spec("say hello", 5, 3))
            .await
            .expect("goal runs");

        assert_eq!(outcome.stop_reason, GoalStopReason::Achieved);
        assert_eq!(outcome.iterations, 1);
        assert_eq!(outcome.turns.len(), 1);
    }

    #[tokio::test]
    async fn stops_at_max_iterations_when_the_model_keeps_working() {
        let mut turns = Vec::new();
        for _ in 0..2 {
            let (tool_turn, _ids) =
                MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "x"}))]);
            turns.push(tool_turn);
            turns.push(MockProvider::text_turn("still working"));
        }
        let provider = Arc::new(MockProvider::with_turns(turns));
        let (service, _store) = goal_service(provider, LoopLimits::default());
        let session = service
            .create_session(NewSessionParams::default())
            .await
            .expect("session");

        let outcome = service
            .run_goal(&session, spec("keep working", 2, 10))
            .await
            .expect("goal runs");

        assert_eq!(outcome.stop_reason, GoalStopReason::MaxIterations);
        assert_eq!(outcome.iterations, 2);
    }

    #[tokio::test]
    async fn stops_at_identical_failure_ceiling() {
        let (turn_a, _) = MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "x"}))]);
        let (turn_b, _) = MockProvider::tool_turn(&[("echo", serde_json::json!({"text": "y"}))]);
        let provider = Arc::new(MockProvider::with_turns([turn_a, turn_b]));
        let (service, _store) = goal_service(
            provider,
            LoopLimits {
                max_iterations: 1,
                ..LoopLimits::default()
            },
        );
        let session = service
            .create_session(NewSessionParams::default())
            .await
            .expect("session");

        let outcome = service
            .run_goal(&session, spec("do the thing", 10, 2))
            .await
            .expect("goal runs");

        assert_eq!(outcome.stop_reason, GoalStopReason::IdenticalFailureCeiling);
        assert_eq!(outcome.iterations, 2);
        assert!(
            outcome
                .turns
                .iter()
                .all(|turn| turn.stop_reason == TurnStopReason::MaxIterations)
        );
    }
}

fn isolated_service(
    store: std::sync::Arc<MemoryStore>,
) -> (
    EngineService,
    std::sync::Arc<agentloop_loop::NativeAgent>,
    std::sync::Arc<MockWorkspaces>,
) {
    let mock = std::sync::Arc::new(MockWorkspaces::new());
    let agent = NativeAgentBuilder::new(store.clone())
        .workspace(mock.clone())
        .build();
    let mut service = EngineService::new(agent.clone(), store);
    service.workspace = Some(mock.clone());
    service.isolation_default = IsolationPolicy::Required;
    (service, agent, mock)
}

async fn open_isolated(service: &EngineService, agent: &agentloop_loop::NativeAgent) -> SessionId {
    let id = service
        .create_session(NewSessionParams {
            cwd: Some(PathBuf::from("/repo")),
            ..NewSessionParams::default()
        })
        .await
        .expect("isolated session opens");
    agent
        .ensure_workspace_for_test(&id)
        .await
        .expect("first-turn workspace provision");
    id
}

#[tokio::test]
async fn update_session_renames_title() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone()).build();
    let service = EngineService::new(agent, store);
    let id = service
        .create_session(NewSessionParams {
            title: Some("old".to_owned()),
            ..NewSessionParams::default()
        })
        .await
        .expect("session");

    let meta = service
        .update_session(
            &id,
            SessionMetaPatch {
                title: Some("renamed".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("update");
    assert_eq!(meta.title.as_deref(), Some("renamed"));
    assert_eq!(
        service
            .session_meta(&id)
            .await
            .expect("meta")
            .title
            .as_deref(),
        Some("renamed")
    );
}

#[tokio::test]
async fn delete_session_removes_from_store() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone()).build();
    let service = EngineService::new(agent, store.clone());
    let id = service
        .create_session(NewSessionParams::default())
        .await
        .expect("session");
    assert_eq!(service.list_sessions().await.expect("list").len(), 1);

    service.delete_session(&id).await.expect("delete");
    assert!(service.list_sessions().await.expect("list").is_empty());
    assert!(store.get_meta(&id).await.is_err());
}

#[tokio::test]
async fn integrate_repoints_cwd_and_records_outcome() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let (service, agent, mock) = isolated_service(store.clone());
    let id = open_isolated(&service, &agent).await;
    assert_ne!(
        store.get_meta(&id).await.expect("meta").cwd,
        PathBuf::from("/repo")
    );

    let outcome = service.integrate_session(&id).await.expect("integrate");
    assert!(matches!(outcome, IntegrationOutcome::Merged { .. }));
    assert_eq!(mock.integrate_calls(), 1);

    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(
        meta.cwd,
        PathBuf::from("/repo"),
        "cwd repointed to base after merge"
    );
    assert!(!service.is_isolated(&id).await.expect("meta"));
}

#[tokio::test]
async fn discard_repoints_cwd_to_base() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let (service, agent, mock) = isolated_service(store.clone());
    let id = open_isolated(&service, &agent).await;

    service.discard_session(&id).await.expect("discard");
    assert_eq!(mock.discard_calls(), 1);
    let meta = store.get_meta(&id).await.expect("meta");
    assert_eq!(meta.cwd, PathBuf::from("/repo"));
    assert!(!service.is_isolated(&id).await.expect("meta"));
}

#[tokio::test]
async fn status_reports_for_isolated_only() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let (service, agent, _mock) = isolated_service(store.clone());
    let id = open_isolated(&service, &agent).await;
    assert!(
        service
            .workspace_status(&id)
            .await
            .expect("status")
            .is_some()
    );
}

#[tokio::test]
async fn integrate_on_a_non_isolated_session_errors() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let mock = std::sync::Arc::new(MockWorkspaces::new());
    let agent = NativeAgentBuilder::new(store.clone()).build();
    let service = EngineService::new(agent, store.clone());
    let id = service
        .create_session(NewSessionParams {
            cwd: Some(PathBuf::from("/repo")),
            ..NewSessionParams::default()
        })
        .await
        .expect("plain session");
    assert!(matches!(
        service.integrate_session(&id).await,
        Err(EngineServiceError::NotIsolated(_))
    ));
    assert_eq!(mock.integrate_calls(), 0);
}

#[tokio::test]
async fn revert_restores_workspace_and_records_marker() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let (service, agent, mock) = isolated_service(store.clone());
    let id = open_isolated(&service, &agent).await;

    service.revert(&id, "snap-abc").await.expect("revert ok");
    assert_eq!(mock.restore_calls(), 1, "workspace restored once");

    let events = store.read(&id, 0).await.expect("events");
    assert!(
        events.iter().any(|stored| matches!(
            &stored.event,
            AgentEvent::SnapshotRestored { snapshot_id } if snapshot_id == "snap-abc"
        )),
        "a SnapshotRestored audit marker was appended to the log"
    );
}

#[tokio::test]
async fn replay_preserves_stored_ts_across_reopen_not_now() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone()).build();
    let service = EngineService::new(agent, store.clone());
    let id = service
        .create_session(NewSessionParams {
            cwd: Some(PathBuf::from("/repo")),
            ..NewSessionParams::default()
        })
        .await
        .expect("session");

    store
        .append(
            &id,
            &[AgentEvent::TurnStarted {
                turn_id: TurnId::from("t0"),
            }],
        )
        .await
        .expect("append");
    let stored = store.read(&id, 0).await.expect("read");
    let turn = stored
        .iter()
        .find(|s| matches!(s.event, AgentEvent::TurnStarted { .. }))
        .expect("turn started stored");
    let (turn_seq, stored_ts) = (turn.seq, turn.ts_ms);
    assert!(stored_ts > 0);

    let replayed = service.replay(&id, 0).await.expect("replay");
    let event = replayed
        .iter()
        .find(|e| e.seq == turn_seq)
        .expect("turn started replayed");
    assert_eq!(
        event.ts_ms, stored_ts,
        "replayed ts_ms equals the stored ts, not now_ms()"
    );

    let again = service.replay(&id, 0).await.expect("replay again");
    let again_event = again
        .iter()
        .find(|e| e.seq == turn_seq)
        .expect("turn started replayed again");
    assert_eq!(again_event.ts_ms, stored_ts);
}

#[tokio::test]
async fn revert_without_a_workspace_backend_errors() {
    let store = std::sync::Arc::new(MemoryStore::new());
    let agent = NativeAgentBuilder::new(store.clone()).build();
    let service = EngineService::new(agent, store.clone());
    let id = service
        .create_session(NewSessionParams {
            cwd: Some(PathBuf::from("/repo")),
            ..NewSessionParams::default()
        })
        .await
        .expect("plain session");
    assert!(matches!(
        service.revert(&id, "snap-x").await,
        Err(EngineServiceError::NoWorkspaceBackend)
    ));
}
