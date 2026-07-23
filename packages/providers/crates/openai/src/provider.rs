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
    SseDecoder, authenticated_request, is_retryable_transport_error, retry_after_ms_from_headers,
    status_to_provider_error,
};

use crate::config::{OPENAI_PROVIDER_ID, OpenAiConfig};
use crate::wire::{ModelList, OpenAiStreamMapper, build_request, models_from_response};

const MAX_ATTEMPTS: u32 = 3;

const BASE_BACKOFF_MS: u64 = 500;

const MAX_BACKOFF_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    id: ProviderId,
    config: Arc<OpenAiConfig>,

    static_models: Vec<ModelInfo>,

    thinking: bool,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self::with_identity(OPENAI_PROVIDER_ID, config, Vec::new(), false)
    }

    pub fn with_identity(
        id: impl Into<ProviderId>,
        config: OpenAiConfig,
        static_models: Vec<ModelInfo>,
        thinking: bool,
    ) -> Self {
        Self {
            id: id.into(),
            config: Arc::new(config),
            static_models,
            thinking,
            client: Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, ProviderError> {
        Ok(Self::new(OpenAiConfig::from_env()?))
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn fetch_models_live(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let provider = self.id();
        let token = self.resolve_auth_token().await?;
        let response = self
            .auth_request(Method::GET, &self.config.models_url(), &token)
            .send()
            .await
            .map_err(|err| ProviderError::Http {
                provider: provider.clone(),
                message: err.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(&provider, status, body, None));
        }
        let models = response
            .json::<ModelList>()
            .await
            .map_err(|err| ProviderError::Stream {
                provider: provider.clone(),
                message: format!("OpenAI models response was not valid JSON: {err}"),
            })?;
        Ok(models_from_response(models))
    }

    async fn resolve_auth_token(&self) -> Result<String, ProviderError> {
        if self.config.oauth_account_id.is_some() {
            if let Some(token) = crate::oauth::resolve_oauth_access_token().await? {
                return Ok(token);
            }
        }
        Ok(self.config.api_key.clone())
    }

    fn auth_request(&self, method: Method, url: &str, token: &str) -> reqwest::RequestBuilder {
        let mut request = authenticated_request(&self.client, method, url, token)
            .header("User-Agent", branding::USER_AGENT);
        if let Some(account) = &self.config.oauth_account_id {
            request = request.header("ChatGPT-Account-Id", account);
        }
        request
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            vision: true,
            documents: false,
            thinking: self.thinking,
            prompt_caching: false,
            native_json_schema_tools: true,
            max_context_tokens: None,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        if !self.static_models.is_empty() {
            return Ok(self.static_models.clone());
        }
        match self.fetch_models_live().await {
            Ok(models) => Ok(models),
            Err(err) => {
                let default = self.config.default_model.trim();
                if default.is_empty() {
                    return Err(err);
                }
                Ok(vec![ModelInfo {
                    id: default.to_owned(),
                    display_name: None,
                    context_window: None,
                    reasoning: false,
                    vision: false,
                }])
            }
        }
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
        let body = build_request(request);

        let mut attempt: u32 = 0;
        let response = loop {
            attempt += 1;
            match self
                .send_chat_request(&provider, &model, &body, &cancel)
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

impl OpenAiProvider {
    async fn send_chat_request(
        &self,
        provider: &ProviderId,
        model: &str,
        body: &(impl serde::Serialize + Sync),
        cancel: &CancellationToken,
    ) -> Result<Response, AttemptError> {
        let token = self
            .resolve_auth_token()
            .await
            .map_err(AttemptError::Terminal)?;
        let url = self.config.chat_completions_url();
        let request = self.auth_request(Method::POST, &url, &token).json(body);
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
    mapper: OpenAiStreamMapper,
    pending: VecDeque<Result<ProviderStreamEvent, ProviderError>>,
    ended: bool,
    closed: bool,
}

pub(crate) fn provider_stream(
    provider: ProviderId,
    model: String,
    chunks: Pin<Box<dyn Stream<Item = Result<String, reqwest::Error>> + Send>>,
) -> impl Stream<Item = Result<ProviderStreamEvent, ProviderError>> + Send {
    let state = StreamState {
        provider,
        chunks,
        decoder: SseDecoder::new(),
        mapper: OpenAiStreamMapper::new(model),
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
                    message: format!("OpenAI stream chunk was not valid JSON: {err}"),
                })),
            },
            Err(err) => state.pending.push_back(Err(ProviderError::Stream {
                provider: state.provider.clone(),
                message: err.to_string(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OpenAiConfig {
        match OpenAiConfig::from_values(
            "sk-test".to_owned(),
            Some("https://example.test/v1".to_owned()),
            None,
        ) {
            Ok(config) => config,
            Err(err) => panic!("config should build: {err}"),
        }
    }

    fn model(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        }
    }

    #[test]
    fn new_keeps_the_builtin_identity_and_caps() {
        let provider = OpenAiProvider::new(config());
        assert_eq!(provider.id().as_str(), OPENAI_PROVIDER_ID);
        assert!(!provider.capabilities().thinking);
    }

    #[test]
    fn with_identity_registers_under_the_custom_id() {
        let provider = OpenAiProvider::with_identity("deepseek", config(), Vec::new(), true);
        assert_eq!(provider.id().as_str(), "deepseek");
        let caps = provider.capabilities();
        assert!(caps.thinking);
        assert!(caps.tool_use);
    }

    #[tokio::test]
    async fn static_models_are_served_without_a_network_call() {
        let models = vec![model("deepseek-chat"), model("deepseek-reasoner")];
        let provider = OpenAiProvider::with_identity("deepseek", config(), models.clone(), false);
        match provider.list_models().await {
            Ok(listed) => assert_eq!(listed, models),
            Err(err) => panic!("static models must not hit the network: {err}"),
        }
    }
}
