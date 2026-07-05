//! End-to-end streaming through a custom identity: a DeepSeek-style SSE
//! response (reasoning + text + tool call + usage) served by wiremock is
//! normalized into the unified provider stream, and the outgoing request
//! carries the Bearer key and the extended-thinking config.

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agentloop_contracts::{ContentBlock, Message, Role, StopReason, TokenUsage};
use agentloop_core::{ChatRequest, Provider, ProviderStreamEvent, ThinkingConfig};
use agentloop_provider_openai::{OpenAiConfig, OpenAiProvider};

const DEEPSEEK_STREAM: &str = include_str!("fixtures/deepseek_stream.sse");

fn user_message(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::markdown(text)],
        cache_hint: false,
    }
}

#[tokio::test]
async fn custom_identity_streams_deepseek_dialect_end_to_end() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(DEEPSEEK_STREAM, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;

    let config = OpenAiConfig::from_values(
        "sk-deepseek-test".to_owned(),
        Some(format!("{}/v1", server.uri())),
        Some("deepseek-reasoner".to_owned()),
    )
    .expect("config builds from values");
    let provider = OpenAiProvider::with_identity("deepseek", config, Vec::new(), true);
    assert_eq!(provider.id().as_str(), "deepseek");

    let mut request = ChatRequest::new("deepseek-reasoner", vec![user_message("read the readme")]);
    request.thinking = Some(ThinkingConfig {
        budget_tokens: 4096,
    });

    let stream = provider
        .stream_chat(request, CancellationToken::new())
        .await
        .expect("stream_chat succeeds");
    let events: Vec<ProviderStreamEvent> = stream
        .map(|item| item.expect("fixture stream contains no errors"))
        .collect()
        .await;

    // Normalized event order: start, thinking, text, tool call, end, usage.
    assert!(
        matches!(&events[0], ProviderStreamEvent::MessageStart { model, .. } if model == "deepseek-reasoner"),
        "expected MessageStart first: {events:?}"
    );
    assert!(
        matches!(&events[1], ProviderStreamEvent::ThinkingDelta { text } if text == "Consider")
    );
    assert!(
        matches!(&events[2], ProviderStreamEvent::ThinkingDelta { text } if text == " the file.")
    );
    assert!(matches!(&events[3], ProviderStreamEvent::MarkdownDelta { text } if text == "Reading"));
    assert!(
        matches!(&events[4], ProviderStreamEvent::MarkdownDelta { text } if text == " it now.")
    );
    assert!(matches!(
        &events[5],
        ProviderStreamEvent::ToolCallStart { call_id, name }
            if call_id.as_str() == "call_1" && name == "Read"
    ));
    assert!(matches!(
        &events[6],
        ProviderStreamEvent::ToolCallArgsDelta { call_id, json_fragment }
            if call_id.as_str() == "call_1" && json_fragment == "{\"file_path\":"
    ));
    assert!(matches!(
        &events[7],
        ProviderStreamEvent::ToolCallArgsDelta { json_fragment, .. }
            if json_fragment == "\"README.md\"}"
    ));
    assert!(matches!(
        &events[8],
        ProviderStreamEvent::MessageEnd {
            stop_reason: StopReason::ToolUse
        }
    ));
    assert!(matches!(
        &events[9],
        ProviderStreamEvent::Usage(TokenUsage {
            input: 12,
            output: 34,
            reasoning: Some(7),
            ..
        })
    ));
    assert_eq!(events.len(), 10, "no stray events: {events:?}");

    // The outgoing request carried the Bearer key and the thinking config.
    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 1);
    let sent = &requests[0];
    assert_eq!(
        sent.headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer sk-deepseek-test")
    );
    let body: serde_json::Value = sent.body_json().expect("request body is JSON");
    assert_eq!(body["model"], "deepseek-reasoner");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 4096);
}
