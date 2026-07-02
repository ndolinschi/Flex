//! Ollama provider implementation.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::{Client, Method};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};
use agentloop_provider_common::status_to_provider_error;

use crate::config::{OLLAMA_PROVIDER_ID, OllamaConfig};
use crate::wire::{ModelList, OllamaStreamMapper, build_request, models_from_response};

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    config: Arc<OllamaConfig>,
    client: Client,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Self {
        Self {
            config: Arc::new(config),
            client: Client::new(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(OllamaConfig::from_env())
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(OLLAMA_PROVIDER_ID)
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn id(&self) -> ProviderId {
        Self::provider_id()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: false,
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
            .client
            .request(Method::GET, self.config.tags_url())
            .header("accept", "application/json")
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
                message: format!("Ollama tags response was not valid JSON: {err}"),
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
                .client
                .request(Method::POST, self.config.chat_url())
                .header("accept", "application/json")
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
    decoder: LineDecoder,
    mapper: OllamaStreamMapper,
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
        decoder: LineDecoder::new(),
        mapper: OllamaStreamMapper::new(model),
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
                    for line in state.decoder.push_str(&chunk) {
                        enqueue_line(&mut state, line);
                    }
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
                    for line in state.decoder.finish() {
                        enqueue_line(&mut state, line);
                    }
                    if !state.mapper.ended() {
                        state.pending.push_back(Ok(ProviderStreamEvent::MessageEnd {
                            stop_reason: agentloop_contracts::StopReason::EndTurn,
                        }));
                    }
                    state.closed = true;
                }
            }
        }
    })
}

fn enqueue_line(state: &mut StreamState, line: String) {
    match state.mapper.map_json(&line) {
        Ok(events) => {
            for mapped in events {
                state.pending.push_back(Ok(mapped));
            }
        }
        Err(err) => state.pending.push_back(Err(ProviderError::Stream {
            provider: state.provider.clone(),
            message: format!("Ollama stream line was not valid JSON: {err}"),
        })),
    }
}

#[derive(Debug, Default)]
struct LineDecoder {
    buffer: String,
}

impl LineDecoder {
    fn new() -> Self {
        Self::default()
    }

    fn push_str(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut lines = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let line = self.buffer[..index].trim().to_owned();
            self.buffer.drain(..=index);
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }

    fn finish(&mut self) -> Vec<String> {
        let line = self.buffer.trim().to_owned();
        self.buffer.clear();
        if line.is_empty() {
            Vec::new()
        } else {
            vec![line]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_list_models_with_ollama_env() {
        let has_ollama_env = std::env::var("OLLAMA_HOST")
            .ok()
            .or_else(|| std::env::var("OLLAMA_MODEL").ok())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if !has_ollama_env {
            return;
        }

        let provider = OllamaProvider::from_env();
        match provider.list_models().await {
            Ok(models) => assert!(!models.is_empty()),
            Err(err) => panic!("Ollama live list_models should succeed: {err}"),
        }
    }
}
