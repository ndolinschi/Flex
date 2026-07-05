//! AWS Bedrock provider implementation (Converse streaming + dynamic models).

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::{Client, Method, RequestBuilder};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelInfo, ProviderCaps, ProviderId};
use agentloop_core::{ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent};
use agentloop_provider_common::status_to_provider_error;

use crate::config::{BEDROCK_PROVIDER_ID, BedrockAuth, BedrockConfig};
use crate::eventstream::{EventStreamDecoder, RawEvent};
use crate::models::{merge_dedup, parse_foundation_models, parse_inference_profiles};
use crate::sigv4::{self, Sigv4Credentials};
use crate::wire::{ConverseStreamMapper, build_request, static_models};

/// AWS service names used in the SigV4 credential scope.
const RUNTIME_SERVICE: &str = "bedrock-runtime";
const CONTROL_SERVICE: &str = "bedrock";

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

    /// Whether a usable credential (Bedrock API key or SigV4 creds) is present.
    /// Bedrock is unusable without one, so callers gate on this before use.
    pub fn has_credentials(&self) -> bool {
        self.config.auth.is_present()
    }

    fn provider_id() -> ProviderId {
        ProviderId::from(BEDROCK_PROVIDER_ID)
    }

    /// Apply the configured auth to a request: bearer token, or SigV4 signing
    /// over the exact `payload`/headers that will be sent.
    fn apply_auth(
        &self,
        builder: RequestBuilder,
        method: &str,
        url: &str,
        service: &str,
        content_type: Option<&str>,
        payload: &[u8],
    ) -> Result<RequestBuilder, ProviderError> {
        match &self.config.auth {
            BedrockAuth::Bearer(token) => {
                let mut builder = builder.bearer_auth(token);
                if let Some(ct) = content_type {
                    builder = builder.header("content-type", ct);
                }
                Ok(builder)
            }
            BedrockAuth::SigV4 {
                access_key_id,
                secret_access_key,
                session_token,
            } => {
                let parsed = reqwest::Url::parse(url).map_err(|err| ProviderError::Http {
                    provider: self.id(),
                    message: format!("invalid Bedrock URL `{url}`: {err}"),
                })?;
                let host = match (parsed.host_str(), parsed.port()) {
                    (Some(host), Some(port)) => format!("{host}:{port}"),
                    (Some(host), None) => host.to_owned(),
                    (None, _) => String::new(),
                };
                let creds = Sigv4Credentials {
                    access_key_id: access_key_id.clone(),
                    secret_access_key: secret_access_key.clone(),
                    session_token: session_token.clone(),
                };
                let (amz_datetime, amz_date) = sigv4::amz_timestamps(SystemTime::now());
                let headers = sigv4::signed_headers(
                    &creds,
                    &self.config.region,
                    service,
                    method,
                    &host,
                    parsed.path(),
                    parsed.query().unwrap_or(""),
                    content_type,
                    payload,
                    &amz_datetime,
                    &amz_date,
                );
                let mut builder = builder;
                for (name, value) in headers {
                    builder = builder.header(name, value);
                }
                Ok(builder)
            }
            BedrockAuth::None => Err(self.missing_auth()),
        }
    }

    fn missing_auth(&self) -> ProviderError {
        ProviderError::AuthMissing {
            provider: self.id(),
            hint: "set `AWS_BEARER_TOKEN_BEDROCK` (Bedrock API key) or AWS SigV4 credentials \
                   (`AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`)"
                .to_owned(),
        }
    }

    /// GET a control-plane endpoint and return the JSON body.
    async fn control_get(&self, url: &str) -> Result<String, ProviderError> {
        let builder = self
            .client
            .request(Method::GET, url)
            .header("accept", "application/json");
        let builder = self.apply_auth(builder, "GET", url, CONTROL_SERVICE, None, b"")?;
        let response = builder.send().await.map_err(|err| ProviderError::Http {
            provider: self.id(),
            message: err.to_string(),
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(status_to_provider_error(&self.id(), status, body, None));
        }
        response.text().await.map_err(|err| ProviderError::Http {
            provider: self.id(),
            message: err.to_string(),
        })
    }

    /// Fetch the live model catalog: on-demand foundation models plus active
    /// cross-region inference profiles (best-effort — profiles failing is
    /// non-fatal). Errors bubble up so the caller can fall back to static.
    async fn fetch_dynamic_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let foundation_body = self
            .control_get(&self.config.foundation_models_url())
            .await?;
        let foundation =
            parse_foundation_models(&foundation_body).map_err(|err| ProviderError::Stream {
                provider: self.id(),
                message: format!("ListFoundationModels response was not valid JSON: {err}"),
            })?;
        let profiles = match self
            .control_get(&self.config.inference_profiles_url())
            .await
        {
            Ok(body) => parse_inference_profiles(&body).unwrap_or_default(),
            Err(err) => {
                tracing::debug!(target: "bedrock", %err, "inference profiles unavailable");
                Vec::new()
            }
        };
        Ok(merge_dedup([foundation, profiles]))
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
        // Without credentials the control plane is unreachable; serve the
        // curated fallback so the picker still has entries.
        if !self.config.auth.is_present() {
            return Ok(static_models());
        }
        match self.fetch_dynamic_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => Ok(static_models()),
            Err(err) => {
                tracing::debug!(target: "bedrock", %err, "dynamic model list failed; using static fallback");
                Ok(static_models())
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
        if !self.config.auth.is_present() {
            return Err(self.missing_auth());
        }

        let model = request.model.clone();
        let url = self.config.converse_stream_url(&model);
        // Serialize once so the SigV4 payload hash matches the sent bytes.
        let payload = serde_json::to_vec(&build_request(request)).map_err(|err| {
            ProviderError::InvalidRequest {
                provider: provider.clone(),
                message: format!("could not encode Bedrock request: {err}"),
            }
        })?;

        let builder = self
            .client
            .request(Method::POST, url.as_str())
            .header("accept", "application/vnd.amazon.eventstream");
        let builder = self.apply_auth(
            builder,
            "POST",
            &url,
            RUNTIME_SERVICE,
            Some("application/json"),
            &payload,
        )?;

        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Err(ProviderError::Cancelled { provider });
            }
            result = builder.body(payload).send() => {
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
    async fn missing_credentials_are_reported() {
        let provider = BedrockProvider::new(BedrockConfig::bearer("us-east-1", "", None));
        let err = provider
            .stream_chat(ChatRequest::new("m", vec![]), CancellationToken::new())
            .await
            .err()
            .expect("should error without credentials");
        assert!(matches!(err, ProviderError::AuthMissing { .. }));
    }

    #[tokio::test]
    async fn list_models_without_credentials_falls_back_to_static() {
        let provider = BedrockProvider::new(BedrockConfig::bearer("us-east-1", "", None));
        let models = provider.list_models().await.expect("static fallback");
        assert!(models.iter().any(|m| m.id.starts_with("anthropic.claude")));
    }

    #[tokio::test]
    async fn cancelled_before_send() {
        let provider = BedrockProvider::new(BedrockConfig::bearer("us-east-1", "key", None));
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
    fn has_credentials_reflects_config() {
        let without = BedrockProvider::new(BedrockConfig::bearer("us-east-1", "", None));
        assert!(!without.has_credentials());
        let with = BedrockProvider::new(BedrockConfig::bearer("us-east-1", "key", None));
        assert!(with.has_credentials());
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
