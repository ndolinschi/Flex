use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::{Client, Method, Response};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId, StopReason, branding};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};
use agentloop_provider_common::{
    SseDecoder, is_retryable_transport_error, retry_after_ms_from_headers, status_to_provider_error,
};
use agentloop_provider_openai::{oauth_account_id, resolve_oauth_access_token};

use crate::config::{CHATGPT_PROVIDER_ID, CODEX_ORIGINATOR, ChatgptConfig};
use crate::models::static_models;
use crate::wire::{BuiltCodexRequest, CodexStreamMapper, build_request};

const MAX_ATTEMPTS: u32 = 3;
const BASE_BACKOFF_MS: u64 = 500;
const MAX_BACKOFF_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct ChatgptProvider {
    config: Arc<ChatgptConfig>,
    client: Client,
}

impl ChatgptProvider {
    pub fn new(config: ChatgptConfig) -> Self {
        Self {
            config: Arc::new(config),
            client: Client::new(),
        }
    }

    pub fn from_oauth() -> Result<Self, ProviderError> {
        Ok(Self::new(ChatgptConfig::from_oauth(
            std::env::var("CHATGPT_MODEL").ok(),
        )?))
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(CHATGPT_PROVIDER_ID)
    }

    async fn resolve_token(&self) -> Result<String, ProviderError> {
        match resolve_oauth_access_token().await? {
            Some(token) => Ok(token),
            None => Err(ProviderError::AuthMissing {
                provider: Self::provider_id(),
                hint: "sign in with ChatGPT Plus/Pro".to_owned(),
            }),
        }
    }

    fn auth_request(
        &self,
        method: Method,
        url: &str,
        token: &str,
        lite_session_id: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(token)
            .header("Accept", "text/event-stream")
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", CODEX_ORIGINATOR)
            .header("User-Agent", branding::USER_AGENT);
        let account = self.config.account_id.clone().or_else(oauth_account_id);
        if let Some(account) = account {
            request = request
                .header("ChatGPT-Account-Id", &account)
                .header("chatgpt-account-id", &account);
        }
        if let Some(session_id) = lite_session_id {
            request = request
                .header("x-openai-internal-codex-responses-lite", "true")
                .header("session-id", session_id)
                .header("x-session-affinity", session_id);
        }
        request
    }
}

#[async_trait]
impl Provider for ChatgptProvider {
    fn id(&self) -> ProviderId {
        Self::provider_id()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            vision: true,
            documents: false,
            thinking: true,
            prompt_caching: false,
            native_json_schema_tools: true,
            max_context_tokens: Some(272_000),
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(static_models())
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream, ProviderError> {
        let provider = self.id();
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled { provider });
        }

        let model = request.model.clone();
        let built = build_request(request);

        let mut attempt: u32 = 0;
        let response = loop {
            attempt += 1;
            match self
                .send_responses_request(&provider, &model, &built, &cancel)
                .await
            {
                Ok(response) => break response,
                Err(outcome) => {
                    let retry_after_ms = outcome.retry_after_ms();
                    let is_last_attempt = attempt >= MAX_ATTEMPTS;
                    match retry_after_ms {
                        Some(_) if !is_last_attempt => {}
                        _ => return Err(outcome.into_error()),
                    }
                    let delay = retry_after_ms
                        .flatten()
                        .map(|ms| ms.min(MAX_BACKOFF_MS))
                        .unwrap_or_else(|| backoff_delay_ms(attempt));
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return Err(ProviderError::Cancelled { provider: provider.clone() });
                        }
                        _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
                    }
                }
            }
        };

        let chunks = Box::pin(
            response
                .bytes_stream()
                .map(|chunk| chunk.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())),
        );
        Ok(Box::pin(provider_stream(provider, model, chunks)))
    }
}

enum AttemptError {
    Retryable {
        error: ProviderError,
        retry_after_ms: Option<u64>,
    },
    Terminal(ProviderError),
}

impl AttemptError {
    fn retry_after_ms(&self) -> Option<Option<u64>> {
        match self {
            Self::Retryable { retry_after_ms, .. } => Some(*retry_after_ms),
            Self::Terminal(_) => None,
        }
    }

    fn into_error(self) -> ProviderError {
        match self {
            Self::Retryable { error, .. } | Self::Terminal(error) => error,
        }
    }
}

impl ChatgptProvider {
    async fn send_responses_request(
        &self,
        provider: &ProviderId,
        model: &str,
        built: &BuiltCodexRequest,
        cancel: &CancellationToken,
    ) -> Result<Response, AttemptError> {
        let token = self.resolve_token().await.map_err(AttemptError::Terminal)?;
        let request = self
            .auth_request(
                Method::POST,
                &self.config.endpoint,
                &token,
                built.lite_session_id.as_deref(),
            )
            .json(&built.body);
        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(AttemptError::Terminal(ProviderError::Cancelled {
                    provider: provider.clone(),
                }));
            }
            result = request.send() => result,
        };

        let response = response.map_err(|err| {
            let provider_err = ProviderError::Http {
                provider: provider.clone(),
                message: err.to_string(),
            };
            if is_retryable_transport_error(&err) {
                AttemptError::Retryable {
                    error: provider_err,
                    retry_after_ms: None,
                }
            } else {
                AttemptError::Terminal(provider_err)
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let retry_after_ms = retry_after_ms_from_headers(response.headers());
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            let err = status_to_provider_error(provider, status, body, Some(model));
            return Err(match err {
                ProviderError::RateLimited { .. } => AttemptError::Retryable {
                    error: err,
                    retry_after_ms,
                },
                other => AttemptError::Terminal(other),
            });
        }

        Ok(response)
    }
}

fn backoff_delay_ms(attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(6);
    BASE_BACKOFF_MS
        .saturating_mul(1u64 << exponent)
        .min(MAX_BACKOFF_MS)
}

struct StreamState {
    provider: ProviderId,
    chunks: Pin<Box<dyn Stream<Item = Result<String, reqwest::Error>> + Send>>,
    decoder: SseDecoder,
    mapper: CodexStreamMapper,
    pending: VecDeque<Result<ProviderStreamEvent, ProviderError>>,
    ended: bool,
    closed: bool,
}

fn provider_stream(
    provider: ProviderId,
    model: String,
    chunks: Pin<Box<dyn Stream<Item = Result<String, reqwest::Error>> + Send>>,
) -> impl Stream<Item = Result<ProviderStreamEvent, ProviderError>> + Send {
    let state = StreamState {
        provider,
        chunks,
        decoder: SseDecoder::new(),
        mapper: CodexStreamMapper::new(model),
        pending: VecDeque::new(),
        ended: false,
        closed: false,
    };

    futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                return Some((event, state));
            }
            if state.closed {
                return None;
            }
            match state.chunks.next().await {
                Some(Ok(chunk)) => {
                    let decoded = state.decoder.push_str(&chunk);
                    enqueue_decoded(&mut state, decoded);
                }
                Some(Err(err)) => {
                    state.closed = true;
                    return Some((
                        Err(ProviderError::Http {
                            provider: state.provider.clone(),
                            message: err.to_string(),
                        }),
                        state,
                    ));
                }
                None => {
                    let decoded = state.decoder.finish();
                    enqueue_decoded(&mut state, decoded);
                    if !state.ended {
                        state.pending.push_back(Ok(ProviderStreamEvent::MessageEnd {
                            stop_reason: StopReason::EndTurn,
                        }));
                        state.ended = true;
                    }
                    state.closed = true;
                }
            }
        }
    })
}

fn enqueue_decoded(
    state: &mut StreamState,
    decoded: Vec<Result<agentloop_provider_common::SseEvent, agentloop_provider_common::SseError>>,
) {
    for event in decoded {
        match event {
            Ok(event) if event.data.trim() == "[DONE]" => {
                if !state.ended {
                    state.pending.push_back(Ok(ProviderStreamEvent::MessageEnd {
                        stop_reason: StopReason::EndTurn,
                    }));
                    state.ended = true;
                }
            }
            Ok(event) => match state.mapper.map_json(&event.data) {
                Ok(events) => {
                    for mapped in events {
                        if matches!(mapped, ProviderStreamEvent::MessageEnd { .. }) {
                            state.ended = true;
                        }
                        state.pending.push_back(Ok(mapped));
                    }
                }
                Err(err) => state.pending.push_back(Err(ProviderError::Stream {
                    provider: state.provider.clone(),
                    message: format!("Codex stream chunk was not valid JSON: {err}"),
                })),
            },
            Err(err) => state.pending.push_back(Err(ProviderError::Stream {
                provider: state.provider.clone(),
                message: err.to_string(),
            })),
        }
    }
}
