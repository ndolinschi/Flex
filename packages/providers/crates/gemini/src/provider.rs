use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::{Client, Method, RequestBuilder};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId, StopReason};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};
use agentloop_provider_common::{SseDecoder, status_to_provider_error};

use crate::config::{GEMINI_PROVIDER_ID, GeminiConfig};
use crate::wire::{GeminiStreamMapper, ModelList, build_request, models_from_response};

#[derive(Debug, Clone)]
pub struct GeminiProvider {
    config: Arc<GeminiConfig>,
    client: Client,
}

impl GeminiProvider {
    pub fn new(config: GeminiConfig) -> Self {
        Self {
            config: Arc::new(config),
            client: Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, ProviderError> {
        Ok(Self::new(GeminiConfig::from_env()?))
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(GEMINI_PROVIDER_ID)
    }

    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client
            .request(method, url)
            .header("x-goog-api-key", &self.config.api_key)
            .header("accept", "application/json")
    }
}

#[async_trait]
impl Provider for GeminiProvider {
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
        let response = self
            .request(Method::GET, &self.config.models_url())
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
                message: format!("Gemini models response was not valid JSON: {err}"),
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
            result = self
                .request(Method::POST, &self.config.stream_generate_content_url(&model))
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
    mapper: GeminiStreamMapper,
    pending: VecDeque<Result<ProviderStreamEvent, ProviderError>>,
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
        mapper: GeminiStreamMapper::new(model),
        pending: VecDeque::new(),
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
                    if !state.mapper.ended() {
                        state.pending.push_back(Ok(ProviderStreamEvent::MessageEnd {
                            stop_reason: StopReason::EndTurn,
                        }));
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
            Ok(event) => match state.mapper.map_json(&event.data) {
                Ok(events) => {
                    for mapped in events {
                        state.pending.push_back(Ok(mapped));
                    }
                }
                Err(err) => state.pending.push_back(Err(ProviderError::Stream {
                    provider: state.provider.clone(),
                    message: format!("Gemini stream chunk was not valid JSON: {err}"),
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

    #[tokio::test]
    #[ignore]
    async fn live_list_models_with_gemini_api_key() {
        let has_key = std::env::var("GEMINI_API_KEY")
            .ok()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !has_key {
            return;
        }

        let provider = match GeminiProvider::from_env() {
            Ok(provider) => provider,
            Err(err) => panic!("Gemini provider should load from env: {err}"),
        };
        match provider.list_models().await {
            Ok(models) => assert!(!models.is_empty()),
            Err(err) => panic!("Gemini live list_models should succeed: {err}"),
        }
    }
}
