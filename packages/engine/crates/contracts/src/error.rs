use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::ProviderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    AuthMissing,
    AuthExpired,
    RateLimited,
    ModelUnavailable,
    PermissionDenied,
    Cancelled,
    ProcessCrashed,
    ProtocolViolation,
    Timeout,
    NotInstalled,
    InvalidRequest,
    ContextOverflow,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Provenance {
    Native {
        provider: ProviderId,
    },
    Delegator {
        agent_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stderr_tail: Option<String>,
    },
    Engine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl EngineError {
    pub fn engine(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            provenance: Provenance::Engine,
            retry_after_ms: None,
            detail: None,
        }
    }
}
