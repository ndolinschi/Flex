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
    /// Instance identity: `openai` for [`OpenAiProvider::new`], the caller's
    /// id for [`OpenAiProvider::with_identity`]. Errors and stream events are
    /// attributed to this id.
    id: ProviderId,
    config: Arc<OpenAiConfig>,
    /// Static catalog served when non-empty (endpoints without `/models`).
    static_models: Vec<ModelInfo>,
    /// Advertise + forward extended-thinking config (DeepSeek-style endpoints).
    thinking: bool,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self::with_identity(OPENAI_PROVIDER_ID, config, Vec::new(), false)
    }

    /// An OpenAI-compatible provider registering under a custom id
    /// (e.g. `deepseek`, `glm`) with its own base URL and key.
    ///
    /// When `static_models` is non-empty, [`Provider::list_models`] serves it
    /// without a network call; when `thinking` is true the provider advertises
    /// the extended-thinking capability and forwards
    /// [`ChatRequest::thinking`](agentloop_core::ChatRequest) on the wire.
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
        // The config points at an unroutable host: a network attempt would
        // error, so an Ok result proves the static catalog short-circuits.
        let models = vec![model("deepseek-chat"), model("deepseek-reasoner")];
        let provider = OpenAiProvider::with_identity("deepseek", config(), models.clone(), false);
        match provider.list_models().await {
            Ok(listed) => assert_eq!(listed, models),
            Err(err) => panic!("static models must not hit the network: {err}"),
        }
    }
}
