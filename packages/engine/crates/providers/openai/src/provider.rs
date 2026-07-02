//! OpenAI provider implementation.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::{Client, Method};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId, StopReason};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};
use agentloop_provider_common::{SseDecoder, authenticated_request, status_to_provider_error};

use crate::config::{OPENAI_PROVIDER_ID, OpenAiConfig};
use crate::wire::{ModelList, OpenAiStreamMapper, build_request, models_from_response};

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    config: Arc<OpenAiConfig>,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            config: Arc::new(config),
            client: Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, ProviderError> {
        Ok(Self::new(OpenAiConfig::from_env()?))
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(OPENAI_PROVIDER_ID)
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        Self::provider_id()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            vision: true,
            documents: false,
            thinking: false,
            prompt_caching: false,
            native_json_schema_tools: true,
            max_context_tokens: None,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let provider = self.id();
        let response = authenticated_request(
            &self.client,
            Method::GET,
            &self.config.models_url(),
            &self.config.api_key,
        )
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
        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            result = authenticated_request(
                &self.client,
                Method::POST,
                &self.config.chat_completions_url(),
                &self.config.api_key,
            )
            .json(&build_request(request))
            .send() => {
                result.map_err(|err| ProviderError::Http {
                    provider: provider.clone(),
                    message: err.to_string(),
                })?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(
                &provider,
                status,
                body,
                Some(&model),
            ));
        }

        let chunks = Box::pin(
            response
                .bytes_stream()
                .map(|chunk| chunk.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())),
        );
        Ok(Box::pin(provider_stream(provider, model, chunks)))
    }
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

fn provider_stream(
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
