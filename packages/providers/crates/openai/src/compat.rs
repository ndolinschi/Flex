use futures::StreamExt;

use agentloop_contracts::{ModelInfo, ProviderId};
use agentloop_core::{ChatRequest, ProviderError, ProviderStream};

use crate::provider::provider_stream;
use crate::wire::{ModelList, build_request, models_from_response};

pub fn chat_body(request: ChatRequest) -> serde_json::Value {
    serde_json::to_value(build_request(request)).unwrap_or_else(|_| serde_json::json!({}))
}

pub fn stream_response(
    provider: ProviderId,
    model: String,
    response: reqwest::Response,
) -> ProviderStream {
    let chunks = Box::pin(
        response
            .bytes_stream()
            .map(|chunk| chunk.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())),
    );
    Box::pin(provider_stream(provider, model, chunks))
}

pub fn models_from_json(
    provider: &ProviderId,
    value: serde_json::Value,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let list: ModelList = serde_json::from_value(value).map_err(|err| ProviderError::Stream {
        provider: provider.clone(),
        message: format!("models response was not valid Chat Completions JSON: {err}"),
    })?;
    Ok(models_from_response(list))
}
