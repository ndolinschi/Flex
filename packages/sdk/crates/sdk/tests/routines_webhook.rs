// This whole file is a test binary; see the identical justification in
// transports/http/tests/serve.rs — clippy's expect_used/unwrap_used lints
// don't treat plain helper fns here as test code even though they're only
// ever reachable from #[tokio::test] functions below.
#![allow(clippy::expect_used)]

use std::sync::Arc;

use agentloop_channel::{RoutineSpec, RoutineStore, RoutineTrigger};
use agentloop_contracts::{GoalSpec, ModelRef, NewSessionParams};
use agentloop_core::ProviderRegistry;
use agentloop_engine::{EngineConfig, EngineService};
use agentloop_sdk::routines::{FileRoutineStore, RoutineRunner, routine_webhook_router};
use agentloop_testkit::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};
use agentloop_transport_http::{AuthToken, build_router_with_extra};

fn default_model() -> ModelRef {
    ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
}

async fn spawn_test_server(
    token: AuthToken,
    store: FileRoutineStore,
) -> (String, tokio::task::JoinHandle<()>) {
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn(
        "webhook-triggered run finished",
    )]));
    let mut providers = ProviderRegistry::new();
    providers.register(provider);
    let engine = Arc::new(
        EngineService::native(providers, Some(default_model()), EngineConfig::default())
            .expect("engine builds"),
    );
    let runner = Arc::new(RoutineRunner::new(engine.clone(), Arc::new(store)));

    let extra = routine_webhook_router(runner, token.clone());
    let router = build_router_with_extra(engine, token, extra);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (format!("http://{addr}"), handle)
}

fn sample_spec(id: &str) -> RoutineSpec {
    RoutineSpec {
        id: id.to_owned(),
        goal: GoalSpec {
            prompt: "say hello".to_owned(),
            max_iterations: 3,
            max_identical_failures: 3,
            token_budget: None,
            require_verification: false,
        },
        session_seed: NewSessionParams::default(),
        trigger: RoutineTrigger::Webhook {
            path: "unused".to_owned(),
        },
    }
}

#[tokio::test]
async fn webhook_trigger_requires_a_bearer_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FileRoutineStore::new(dir.path());
    store.upsert(sample_spec("r1")).await.expect("upsert");

    let token = AuthToken::new("test-token");
    let (base, _server) = spawn_test_server(token, store).await;
    let client = reqwest::Client::new();

    let unauthorized = client
        .post(format!("{base}/routines/r1/trigger"))
        .send()
        .await
        .expect("request completes");
    assert_eq!(unauthorized.status(), 401);
}

#[tokio::test]
async fn webhook_trigger_accepts_a_known_routine_with_a_valid_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FileRoutineStore::new(dir.path());
    store.upsert(sample_spec("r1")).await.expect("upsert");

    let token = AuthToken::new("test-token");
    let (base, _server) = spawn_test_server(token.clone(), store).await;
    let client = reqwest::Client::new();

    let accepted = client
        .post(format!("{base}/routines/r1/trigger"))
        .bearer_auth(token.as_str())
        .send()
        .await
        .expect("request completes");
    assert_eq!(accepted.status(), 202);
}

#[tokio::test]
async fn webhook_trigger_404s_for_an_unknown_routine() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = FileRoutineStore::new(dir.path());

    let token = AuthToken::new("test-token");
    let (base, _server) = spawn_test_server(token.clone(), store).await;
    let client = reqwest::Client::new();

    let missing = client
        .post(format!("{base}/routines/does-not-exist/trigger"))
        .bearer_auth(token.as_str())
        .send()
        .await
        .expect("request completes");
    assert_eq!(missing.status(), 404);
}
