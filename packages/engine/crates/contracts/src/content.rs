use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{ProviderId, ToolCallId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BlobSource {
    Base64 { data: String },
    Url { url: String },
    Path { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    Markdown {
        text: String,
    },
    Image {
        media_type: String,
        data: BlobSource,
    },
    File {
        name: String,
        media_type: String,
        data: BlobSource,
    },
    Thinking {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: ToolCallId,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: ToolCallId,
        content: Vec<ToolResultBlock>,
        #[serde(default)]
        is_error: bool,
    },
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
    Json {
        value: serde_json::Value,
    },
}

impl ToolResultBlock {
    pub fn markdown(text: impl Into<String>) -> Self {
        Self::Markdown { text: text.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
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
