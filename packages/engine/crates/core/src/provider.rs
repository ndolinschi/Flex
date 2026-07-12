//! The `Provider` trait: a thin, streaming client for one LLM API.
//!
//! Providers are the *only* place a model's wire format exists. Their single
//! obligation beyond transport is normalization: every stream maps into
//! [`ProviderStreamEvent`]s (the unified stream format), and provider quirks
//! that must round-trip travel as [`agentloop_contracts::ContentBlock::Opaque`] blocks or the
//! namespaced [`ChatRequest::extra`] passthrough — never as new core concepts.

use std::collections::BTreeMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    EngineError, ErrorCode, Message, MessageId, ModelInfo, Provenance, ProviderCaps, ProviderId,
    StopReason, TokenUsage, ToolCallId,
};

/// A streaming response: normalized events until `MessageEnd` (or an error).
pub type ProviderStream =
    Pin<Box<dyn Stream<Item = Result<ProviderStreamEvent, ProviderError>> + Send + 'static>>;

/// One normalized streaming event from a provider.
///
/// The loop turns these into canonical `AgentEvent`s; the accumulated message
/// is materialized when `MessageEnd` arrives.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderStreamEvent {
    MessageStart {
        message_id: MessageId,
        model: String,
    },
    /// Canonical text is markdown — plain-text models pass through unchanged.
    MarkdownDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    /// A signed/opaque thinking block completed (providers that sign
    /// reasoning emit this so the signature can round-trip).
    ThinkingSignature {
        signature: String,
    },
    ToolCallStart {
        call_id: ToolCallId,
        name: String,
    },
    ToolCallArgsDelta {
        call_id: ToolCallId,
        json_fragment: String,
    },
    /// Arguments are now complete and parseable.
    ToolCallEnd {
        call_id: ToolCallId,
    },
    /// May arrive multiple times; later reports supersede earlier ones.
    Usage(TokenUsage),
    MessageEnd {
        stop_reason: StopReason,
    },
}

/// Agent-facing tool definition sent to the model (no `run`, just the spec).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// Full JSON Schema of the input object.
    pub input_schema: serde_json::Value,
}

/// How the model is allowed to use tools for one request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Named(String),
}

/// Re-export: the canonical extended-thinking configuration lives in
/// contracts (it is part of the `TurnOptions` wire type); providers keep
/// importing it from here.
pub use agentloop_contracts::ThinkingConfig;

/// One chat request in canonical form. Providers map this onto their wire.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub tool_choice: ToolChoice,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub thinking: Option<ThinkingConfig>,
    /// Namespaced passthrough: `{ "anthropic": {...}, "openai": {...} }`.
    /// Each provider reads only its own key; core never inspects it.
    pub extra: BTreeMap<ProviderId, serde_json::Value>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            system: None,
            messages,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            max_tokens: None,
            temperature: None,
            thinking: None,
            extra: BTreeMap::new(),
        }
    }
}

/// A failure from a provider client. Retryable failures (429/529) are retried
/// *inside* the provider with bounded backoff; the loop only sees terminal
/// errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("authentication missing for {provider}: {hint}")]
    AuthMissing { provider: ProviderId, hint: String },
    #[error("authentication rejected for {provider}: {message}")]
    AuthRejected {
        provider: ProviderId,
        message: String,
    },
    #[error("rate limited by {provider} (retry after {retry_after_ms:?} ms)")]
    RateLimited {
        provider: ProviderId,
        retry_after_ms: Option<u64>,
    },
    #[error("model {model} unavailable on {provider}: {message}")]
    ModelUnavailable {
        provider: ProviderId,
        model: String,
        message: String,
    },
    #[error("context window exceeded on {provider}: {message}")]
    ContextOverflow {
        provider: ProviderId,
        message: String,
    },
    #[error("invalid request to {provider}: {message}")]
    InvalidRequest {
        provider: ProviderId,
        message: String,
    },
    #[error("HTTP failure talking to {provider}: {message}")]
    Http {
        provider: ProviderId,
        message: String,
    },
    #[error("malformed stream from {provider}: {message}")]
    Stream {
        provider: ProviderId,
        message: String,
    },
    #[error("request to {provider} cancelled")]
    Cancelled { provider: ProviderId },
}

impl ProviderError {
    /// Normalize into the wire-level [`EngineError`].
    pub fn to_engine_error(&self) -> EngineError {
        let (code, retryable, retry_after_ms) = match self {
            Self::AuthMissing { .. } => (ErrorCode::AuthMissing, false, None),
            Self::AuthRejected { .. } => (ErrorCode::AuthExpired, false, None),
            Self::RateLimited { retry_after_ms, .. } => {
                (ErrorCode::RateLimited, true, *retry_after_ms)
            }
            Self::ModelUnavailable { .. } => (ErrorCode::ModelUnavailable, false, None),
            Self::ContextOverflow { .. } => (ErrorCode::ContextOverflow, false, None),
            Self::InvalidRequest { .. } => (ErrorCode::InvalidRequest, false, None),
            Self::Http { .. } => (ErrorCode::Unknown, true, None),
            Self::Stream { .. } => (ErrorCode::ProtocolViolation, false, None),
            Self::Cancelled { .. } => (ErrorCode::Cancelled, false, None),
        };
        EngineError {
            code,
            message: self.to_string(),
            retryable,
            provenance: Provenance::Native {
                provider: self.provider().clone(),
            },
            retry_after_ms,
            detail: None,
        }
    }

    pub fn provider(&self) -> &ProviderId {
        match self {
            Self::AuthMissing { provider, .. }
            | Self::AuthRejected { provider, .. }
            | Self::RateLimited { provider, .. }
            | Self::ModelUnavailable { provider, .. }
            | Self::ContextOverflow { provider, .. }
            | Self::InvalidRequest { provider, .. }
            | Self::Http { provider, .. }
            | Self::Stream { provider, .. }
            | Self::Cancelled { provider } => provider,
        }
    }
}

/// A thin, streaming client for one LLM API.
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    fn capabilities(&self) -> ProviderCaps;

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    /// The single entry point — always streaming (non-streaming consumers
    /// collect the stream). Must emit unified-stream-format events only.
    async fn stream_chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream, ProviderError>;
}
