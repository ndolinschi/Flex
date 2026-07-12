//! Content blocks — Layer A of the unified stream format.
//!
//! Text is CommonMark markdown *by contract*: [`ContentBlock::Markdown`] is
//! the only text kind. Sources that emit ANSI, HTML, or provider-specific rich
//! text must convert before their output enters the engine. Plain text is
//! valid markdown, so plain-text sources pass through unchanged.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{ProviderId, ToolCallId};

/// Who authored a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    User,
    Assistant,
    System,
}

/// Where binary data (images, files) comes from.
///
/// Transports may send small payloads inline (`Base64`), reference remote
/// content (`Url`), or point at local files (`Path`). The engine resolves
/// `Path` sources (read + base64, size-capped) before content reaches a
/// provider, so provider clients only ever see `Base64` or `Url`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BlobSource {
    Base64 { data: String },
    Url { url: String },
    Path { path: PathBuf },
}

/// One block of message content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    /// CommonMark text. The only text kind in the canonical model.
    Markdown { text: String },
    /// Image input/output (png/jpeg/webp/gif).
    Image {
        media_type: String,
        data: BlobSource,
    },
    /// File attachment as prompt input (pdf, documents, arbitrary files).
    File {
        name: String,
        media_type: String,
        data: BlobSource,
    },
    /// Model reasoning. `signature` preserves provider-verifiable blobs
    /// (e.g. Anthropic thinking signatures) so they round-trip on resume.
    Thinking {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// The model requested a tool invocation.
    ToolUse {
        id: ToolCallId,
        name: String,
        input: serde_json::Value,
    },
    /// A tool result being fed back to the model.
    ToolResult {
        tool_use_id: ToolCallId,
        content: Vec<ToolResultBlock>,
        #[serde(default)]
        is_error: bool,
    },
    /// Provider-specific block that must round-trip verbatim (e.g. Anthropic
    /// `redacted_thinking`). Keyed by provider; the owning provider re-encodes
    /// it, everyone else treats it as inert.
    Opaque {
        provider: ProviderId,
        data: serde_json::Value,
    },
}

impl ContentBlock {
    pub fn markdown(text: impl Into<String>) -> Self {
        Self::Markdown { text: text.into() }
    }
}

/// Content inside a tool result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolResultBlock {
    Markdown {
        text: String,
    },
    Image {
        media_type: String,
        data: BlobSource,
    },
    /// Structured output for tools that return machine-readable data.
    Json {
        value: serde_json::Value,
    },
}

impl ToolResultBlock {
    pub fn markdown(text: impl Into<String>) -> Self {
        Self::Markdown { text: text.into() }
    }
}

/// One message in a conversation, as sent to providers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
    /// Prompt-cache breakpoint hint. Providers that support prompt caching
    /// place a cache boundary after this message; others ignore it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cache_hint: bool,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::markdown(text)],
            cache_hint: false,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::markdown(text)],
            cache_hint: false,
        }
    }
}
