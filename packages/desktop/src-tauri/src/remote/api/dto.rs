
use agentloop_contracts::SessionMeta;
use serde::{Deserialize, Serialize};

pub const MAX_PROMPT_CHARS: usize = 32_768;

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl From<SessionMeta> for SessionSummary {
    fn from(meta: SessionMeta) -> Self {
        Self {
            id: meta.id.as_str().to_owned(),
            title: meta.title,
            created_at_ms: meta.created_at_ms,
            updated_at_ms: meta.updated_at_ms,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    pub protocol_version: u32,
    pub app_version: String,
    pub device_name: String,
    pub device_id: String,
    pub capabilities: Vec<String>,
    pub openapi_url: String,
}
