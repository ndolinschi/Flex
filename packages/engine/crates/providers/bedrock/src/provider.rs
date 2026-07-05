//! AWS Bedrock provider implementation (Converse streaming).

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

use crate::config::{BEDROCK_PROVIDER_ID, BedrockConfig};
use crate::eventstream::{EventStreamDecoder, RawEvent};
use crate::wire::{ConverseStreamMapper, build_request, static_models};

#[derive(Debug, Clone)]
pub struct BedrockProvider {
    config: Arc<BedrockConfig>,
    client: Client,
}

impl BedrockProvider {
    pub fn new(config: BedrockConfig) -> Self {
        Self {
            config: Arc::new(config),
            client: Client::new(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(BedrockConfig::from_env())
    }

    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(BEDROCK_PROVIDER_ID)
    }
}

#[async_trait]
impl Provider for BedrockProvider {
    fn id(&self) -> ProviderId {
        Self::provider_id()
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            tool_use: true,
            parallel_tool_use: true,
            vision: false,
            documents: false,
            thinking: true,
            prompt_caching: true,
            native_json_schema_tools: true,
            max_context_tokens: None,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // Bedrock's model catalog is a SigV4 control-plane call; with only a
        // bearer token we serve a curated static list instead.
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
        if self.config.api_key.trim().is_empty() {
            return Err(ProviderError::AuthMissing {
                provider,
                hint: "set AWS_BEARER_TOKEN_BEDROCK to a Bedrock API key".to_owned(),
            });
        }

        let model = request.model.clone();
        let url = self.config.converse_stream_url(&model);
        let body = build_request(request);

        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            result = self
                .client
                .request(Method::POST, url)
                .header("content-type", "application/json")
                .header("accept", "application/vnd.amazon.eventstream")
                .bearer_auth(&self.config.api_key)
                .json(&body)
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
                .map(|chunk| chunk.map(|bytes| bytes.to_vec())),
        );
        Ok(Box::pin(provider_stream(provider, model, chunks)))
    }
}

type ByteChunks = Pin<Box<dyn Stream<Item = Result<Vec<u8>, reqwest::Error>> + Send>>;

struct StreamState {
    provider: ProviderId,
    chunks: ByteChunks,
    decoder: EventStreamDecoder,
    mapper: ConverseStreamMapper,
    pending: VecDeque<Result<ProviderStreamEvent, ProviderError>>,
    closed: bool,
}

fn provider_stream(
    provider: ProviderId,
    model: String,
    chunks: ByteChunks,
) -> impl Stream<Item = Result<ProviderStreamEvent, ProviderError>> + Send {
    let state = StreamState {
        provider,
        chunks,
        decoder: EventStreamDecoder::new(),
        mapper: ConverseStreamMapper::new(model),
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
                    state.decoder.push(&chunk);
                    drain_frames(&mut state);
                }
                Some(Err(err)) => {
                    state.closed = true;
                    state.pending.push_back(Err(ProviderError::Http {
                        provider: state.provider.clone(),
                        message: err.to_string(),
                    }));
                }
                None => {
                    if !state.mapper.ended() {
                        state.pending.push_back(Ok(ProviderStreamEvent::MessageEnd {
                            stop_reason: state.mapper.stop_reason(),
                        }));
                    }
                    state.closed = true;
                }
            }
        }
    })
}

/// Pull every complete frame currently buffered and enqueue its mapped events.
fn drain_frames(state: &mut StreamState) {
    loop {
        match state.decoder.next_message() {
            Ok(Some(event)) => process_event(state, event),
            Ok(None) => return,
            Err(message) => {
                state.pending.push_back(Err(ProviderError::Stream {
                    provider: state.provider.clone(),
                    message,
                }));
                state.closed = true;
                return;
            }
        }
    }
}

fn process_event(state: &mut StreamState, event: RawEvent) {
    // Bedrock signals in-band errors as `:message-type: exception`.
    let is_exception =
        event.message_type.as_deref() == Some("exception") || event.exception_type.is_some();
    if is_exception {
        let name = event
            .exception_type
            .clone()
            .or_else(|| event.event_type.clone())
            .unwrap_or_else(|| "unknown".to_owned());
        state.pending.push_back(Err(exception_to_error(
            &state.provider,
            &name,
            &event.payload,
        )));
        state.closed = true;
        return;
    }
    let Some(event_type) = event.event_type.as_deref() else {
        return; // non-event frames (e.g. keep-alives) carry no routing type
    };
    match state.mapper.map_event(event_type, &event.payload) {
        Ok(events) => {
            for mapped in events {
                state.pending.push_back(Ok(mapped));
            }
        }
        Err(err) => state.pending.push_back(Err(ProviderError::Stream {
            provider: state.provider.clone(),
            message: format!("Bedrock stream event `{event_type}` was not valid JSON: {err}"),
        })),
    }
}

fn exception_to_error(provider: &ProviderId, name: &str, payload: &[u8]) -> ProviderError {
    let message = serde_json::from_slice::<serde_json::Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| name.to_owned());
    let lower = name.to_ascii_lowercase();
    if lower.contains("throttl") {
        ProviderError::RateLimited {
            provider: provider.clone(),
            retry_after_ms: None,
        }
    } else if lower.contains("validation") {
        ProviderError::InvalidRequest {
            provider: provider.clone(),
            message,
        }
    } else {
        ProviderError::Stream {
            provider: provider.clone(),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::StopReason;

    #[tokio::test]
    async fn missing_api_key_is_reported() {
        let provider = BedrockProvider::new(BedrockConfig::new("us-east-1", "", None));
        let err = provider
            .stream_chat(ChatRequest::new("m", vec![]), CancellationToken::new())
            .await
            .err()
            .expect("should error without a key");
        assert!(matches!(err, ProviderError::AuthMissing { .. }));
    }

    #[tokio::test]
    async fn cancelled_before_send() {
        let provider = BedrockProvider::new(BedrockConfig::new("us-east-1", "key", None));
        let token = CancellationToken::new();
        token.cancel();
        let err = provider
            .stream_chat(ChatRequest::new("m", vec![]), token)
            .await
            .err()
            .expect("should error when cancelled");
        assert!(matches!(err, ProviderError::Cancelled { .. }));
    }

    #[test]
    fn lists_static_models() {
        let models = static_models();
        assert!(models.iter().any(|m| m.id.starts_with("anthropic.claude")));
    }

    #[test]
    fn end_turn_is_the_default_stop_reason() {
        let mapper = ConverseStreamMapper::new("m");
        assert_eq!(mapper.stop_reason(), StopReason::EndTurn);
    }
}
