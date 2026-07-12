//! Normalized errors. Every provider and delegator maps its failures into
//! [`EngineError`] so consumers see one vocabulary regardless of the source.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::ProviderId;

/// Machine-readable failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    /// No credentials configured for the selected provider/agent.
    AuthMissing,
    /// Credentials present but rejected.
    AuthExpired,
    RateLimited,
    ModelUnavailable,
    PermissionDenied,
    Cancelled,
    /// A delegated agent's process died unexpectedly.
    ProcessCrashed,
    /// The remote side violated its own wire protocol.
    ProtocolViolation,
    Timeout,
    /// Required external CLI is not installed.
    NotInstalled,
    InvalidRequest,
    ContextOverflow,
    Unknown,
}

/// Where a failure originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Provenance {
    /// A native provider client.
    Native { provider: ProviderId },
    /// A delegated external agent.
    Delegator {
        agent_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// Last lines of the process's stderr, for diagnosis.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stderr_tail: Option<String>,
    },
    /// The engine itself.
    Engine,
}

/// A normalized failure. The `message` must be actionable: what failed, why,
/// and what to do about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    /// Whether retrying the same request may succeed.
    #[serde(default)]
    pub retryable: bool,
    pub provenance: Provenance,
    /// Suggested wait before retrying (from rate-limit headers etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Source-specific payload for debugging; never required to handle.
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
