//! Integration: ChatgptProvider streams Codex Responses SSE via wiremock.

use agentloop_core::{ChatRequest, Provider};
use agentloop_provider_chatgpt::{CHATGPT_PROVIDER_ID, ChatgptConfig, ChatgptProvider};
use agentloop_provider_openai::{OpenAiOAuthTokens, store_oauth_tokens};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn streams_codex_responses_sse() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_home = dir.path().to_path_buf();

    temp_env::async_with_vars(
        [
            ("XDG_CONFIG_HOME", Some(config_home.as_os_str())),
            ("HOME", None::<&std::ffi::OsStr>),
        ],
        async {
            let tokens = OpenAiOAuthTokens {
                access_token: "tok_test".to_owned(),
                refresh_token: "ref_test".to_owned(),
                expires_at_ms: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("time")
                        .as_millis() as u64
                        + 3_600_000,
                ),
                account_id: Some("acct_1".to_owned()),
            };
            store_oauth_tokens(&tokens).expect("store oauth");

            let server = MockServer::start().await;
            let sse = concat!(
                "event: response.created\n",
                "data: {\"type\":\"response.created\",\"response\":{\"model\":\"gpt-5.4\"}}\n\n",
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":1},\"output\":[]}}\n\n",
            );
            Mock::given(method("POST"))
                .and(path("/backend-api/codex/responses"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .mount(&server)
                .await;

            let config = ChatgptConfig {
                default_model: "gpt-5.4".to_owned(),
                endpoint: format!("{}/backend-api/codex/responses", server.uri()),
                account_id: Some("acct_1".to_owned()),
            };
            let provider = ChatgptProvider::new(config);
            assert_eq!(provider.id().as_str(), CHATGPT_PROVIDER_ID);

            let request =
                ChatRequest::new("gpt-5.4", vec![agentloop_contracts::Message::user("hi")]);
            let mut stream = provider
                .stream_chat(request, CancellationToken::new())
                .await
                .expect("stream");

            let mut saw_markdown = false;
            let mut saw_end = false;
            while let Some(item) = stream.next().await {
                let event = item.expect("event");
                match event {
                    agentloop_core::ProviderStreamEvent::MarkdownDelta { text } => {
                        assert_eq!(text, "Hello");
                        saw_markdown = true;
                    }
                    agentloop_core::ProviderStreamEvent::MessageEnd { .. } => saw_end = true,
                    _ => {}
                }
            }
            assert!(saw_markdown);
            assert!(saw_end);
        },
    )
    .await;
}
