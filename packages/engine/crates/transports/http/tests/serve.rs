#![allow(clippy::expect_used)]

use std::sync::Arc;

use agentloop_contracts::ModelRef;
use agentloop_core::ProviderRegistry;
use agentloop_engine::EngineService;
use agentloop_loop::NativeAgentBuilder;
use agentloop_session::MemoryStore;
use agentloop_testkit::{MOCK_MODEL, MOCK_PROVIDER_ID, MockProvider};
use agentloop_transport_http::{AuthToken, build_router};

fn default_model() -> ModelRef {
    ModelRef(format!("{MOCK_PROVIDER_ID}/{MOCK_MODEL}"))
}

async fn spawn_test_server(token: AuthToken) -> (String, tokio::task::JoinHandle<()>) {
    let provider = Arc::new(MockProvider::with_turns([MockProvider::text_turn(
        "hello from mock",
    )]));
    let store = Arc::new(MemoryStore::new());
    let mut providers = ProviderRegistry::new();
    providers.register(provider);
    let agent = NativeAgentBuilder::new(store.clone())
        .providers(providers)
        .system_prompt("test agent")
        .default_model(default_model())
        .build();
    let service = Arc::new(EngineService::new(agent, store));

    let router = build_router(service, token);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn full_session_lifecycle_over_http() {
    let token = AuthToken::new("test-token");
    let (base, _server) = spawn_test_server(token.clone()).await;
    let client = reqwest::Client::new();

    let health = client
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health request");
    assert_eq!(health.status(), 200, "/health needs no token");

    let unauthorized = client
        .get(format!("{base}/sessions"))
        .send()
        .await
        .expect("unauthorized request");
    assert_eq!(unauthorized.status(), 401, "no bearer token -> 401");

    let created: serde_json::Value = client
        .post(format!("{base}/sessions"))
        .bearer_auth(token.as_str())
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("create session")
        .json()
        .await
        .expect("create session body");
    let session_id = created["session_id"]
        .as_str()
        .expect("session_id")
        .to_owned();

    let prompt_response = client
        .post(format!("{base}/sessions/{session_id}/prompt"))
        .bearer_auth(token.as_str())
        .json(&serde_json::json!({"prompt": "hi"}))
        .send()
        .await
        .expect("prompt request");
    assert_eq!(prompt_response.status(), 202, "turn admitted");

    let mut stream = client
        .get(format!("{base}/sessions/{session_id}/events?from_seq=0"))
        .bearer_auth(token.as_str())
        .send()
        .await
        .expect("events request")
        .bytes_stream();

    use futures::StreamExt;
    let mut body = String::new();
    let saw_turn_completed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.expect("chunk");
            body.push_str(&String::from_utf8_lossy(&chunk));
            if body.contains("event: turn_completed") {
                return true;
            }
        }
        false
    })
    .await
    .expect("events stream did not time out");
    assert!(
        saw_turn_completed,
        "expected a turn_completed SSE event in stream, got:\n{body}"
    );
    assert!(
        body.contains("assistant_message"),
        "expected an assistant_message SSE event, got:\n{body}"
    );

    let listed: Vec<serde_json::Value> = client
        .get(format!("{base}/sessions"))
        .bearer_auth(token.as_str())
        .send()
        .await
        .expect("list sessions")
        .json()
        .await
        .expect("list sessions body");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], session_id);
}

#[tokio::test]
async fn create_session_persists_model_and_fallback_chain() {
    let token = AuthToken::new("test-token");
    let (base, _server) = spawn_test_server(token.clone()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{base}/sessions"))
        .bearer_auth(token.as_str())
        .json(&serde_json::json!({
            "model": "anthropic/claude-sonnet-4-5",
            "fallback_models": ["openai/gpt-5", "ollama/llama3"],
        }))
        .send()
        .await
        .expect("create session")
        .json()
        .await
        .expect("create session body");
    let session_id = created["session_id"].as_str().expect("session_id");

    let fetched: serde_json::Value = client
        .get(format!("{base}/sessions/{session_id}"))
        .bearer_auth(token.as_str())
        .send()
        .await
        .expect("get session")
        .json()
        .await
        .expect("get session body");
    assert_eq!(fetched["model"], "anthropic/claude-sonnet-4-5");
    assert_eq!(
        fetched["fallback_models"],
        serde_json::json!(["openai/gpt-5", "ollama/llama3"])
    );
}
