//! `stream_chat` must retry transient failures (429, transport errors)
//! instead of surfacing a hard error on the very first hiccup, since a
//! single-provider session (the common case — no `fallback_models`) has no
//! other candidate to fall back to.

use std::sync::Mutex;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use agentloop_contracts::{ContentBlock, Message, Role};
use agentloop_core::{ChatRequest, Provider, ProviderStreamEvent};
use agentloop_provider_openai::{OpenAiConfig, OpenAiProvider};

const DEEPSEEK_STREAM: &str = include_str!("fixtures/deepseek_stream.sse");

fn user_message(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::markdown(text)],
        cache_hint: false,
    }
}

#[allow(clippy::expect_used)]
fn config(server: &MockServer) -> OpenAiConfig {
    OpenAiConfig::from_values(
        "sk-test".to_owned(),
        Some(format!("{}/v1", server.uri())),
        Some("deepseek-reasoner".to_owned()),
    )
    .expect("config builds from values")
}

/// Replays a fixed sequence of responses, repeating the last one — same
/// pattern used by the Copilot device-flow tests.
struct SequenceResponder(Mutex<Vec<ResponseTemplate>>);

impl SequenceResponder {
    fn new(responses: Vec<ResponseTemplate>) -> Self {
        Self(Mutex::new(responses))
    }
}

impl Respond for SequenceResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let mut queue = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if queue.len() > 1 {
            queue.remove(0)
        } else {
            queue[0].clone()
        }
    }
}

#[tokio::test]
async fn a_429_with_retry_after_is_retried_and_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("{\"error\":\"rate limited\"}"),
            ResponseTemplate::new(200).set_body_raw(DEEPSEEK_STREAM, "text/event-stream"),
        ]))
        .expect(2)
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_identity("deepseek", config(&server), Vec::new(), true);
    let request = ChatRequest::new("deepseek-reasoner", vec![user_message("hi")]);

    let stream = provider
        .stream_chat(request, CancellationToken::new())
        .await
        .expect("stream_chat retries the 429 and succeeds on the second attempt");
    let events: Vec<ProviderStreamEvent> = stream
        .map(|item| item.expect("fixture stream contains no errors"))
        .collect()
        .await;

    assert!(
        matches!(&events[0], ProviderStreamEvent::MessageStart { .. }),
        "expected a normal stream after the retry succeeded: {events:?}"
    );
    assert!(matches!(events.last(), Some(ProviderStreamEvent::Usage(_))));

    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(
        requests.len(),
        2,
        "one failed attempt, one successful retry"
    );
}

#[tokio::test]
async fn repeated_429s_exhaust_retries_and_surface_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("{\"error\":\"rate limited\"}"),
        )
        .expect(3)
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_identity("deepseek", config(&server), Vec::new(), true);
    let request = ChatRequest::new("deepseek-reasoner", vec![user_message("hi")]);

    let err = match provider
        .stream_chat(request, CancellationToken::new())
        .await
    {
        Ok(_) => panic!("all attempts 429 so stream_chat must give up eventually"),
        Err(err) => err,
    };
    assert!(
        matches!(err, agentloop_core::ProviderError::RateLimited { .. }),
        "expected a terminal RateLimited error after exhausting attempts: {err}"
    );

    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 3, "bounded at MAX_ATTEMPTS, not infinite");
}

#[tokio::test]
async fn auth_failure_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("{\"error\":\"bad key\"}"))
        .expect(1)
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_identity("deepseek", config(&server), Vec::new(), true);
    let request = ChatRequest::new("deepseek-reasoner", vec![user_message("hi")]);

    let err = match provider
        .stream_chat(request, CancellationToken::new())
        .await
    {
        Ok(_) => panic!("401 is terminal"),
        Err(err) => err,
    };
    assert!(
        matches!(err, agentloop_core::ProviderError::AuthRejected { .. }),
        "expected AuthRejected without any retry attempts: {err}"
    );

    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 1, "auth failures must not be retried");
}
