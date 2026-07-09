//! Wire DTOs for the HTTP transport. Deliberately separate from
//! `agentloop_contracts` types: `contracts` depends on nothing heavy
//! (serde/schemars/uuid only), and `utoipa::ToSchema` derives stay confined
//! to this crate's own request/response boundary.

use agentloop_contracts::{PermissionDecision, PermissionMode, SessionMeta};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateSessionRequest {
    /// Working directory for the session; defaults to the server's own cwd.
    pub cwd: Option<String>,
    pub title: Option<String>,
    /// Role this root session runs as (e.g. `searcher`); `None` = main.
    pub role: Option<String>,
    /// Model for this session, optionally `provider/`-qualified; `None`
    /// defers to the role's model chain, then the server's `--model` default.
    #[serde(default)]
    pub model: Option<String>,
    /// This session's fallback chain, in order; empty defers to the server's
    /// `--fallback-model` default (if any). A `provider/`-qualified entry
    /// naming a provider the server didn't register fails when the chain
    /// reaches it, not at session creation.
    #[serde(default)]
    pub fallback_models: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub cwd: String,
    pub role: Option<String>,
    pub model: Option<String>,
    pub fallback_models: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl From<SessionMeta> for SessionSummary {
    fn from(meta: SessionMeta) -> Self {
        Self {
            id: meta.id.as_str().to_owned(),
            title: meta.title,
            cwd: meta.cwd.display().to_string(),
            role: meta.role,
            model: meta.model.map(|m| m.0),
            fallback_models: meta.fallback_models.into_iter().map(|m| m.0).collect(),
            created_at_ms: meta.created_at_ms,
            updated_at_ms: meta.updated_at_ms,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct PromptRequest {
    pub prompt: String,
    /// One of `default`, `accept_edits`, `plan`, `dont_ask`,
    /// `bypass_permissions`; omit to keep the session's current mode.
    pub permission_mode: Option<String>,
}

/// Parse the wire string for `permission_mode`. Kept local (not a
/// `From`/`TryFrom` on the contracts type) so contracts stays free of any
/// HTTP-transport-specific parsing concerns.
pub(crate) fn parse_permission_mode(value: &str) -> Result<PermissionMode, String> {
    match value {
        "default" => Ok(PermissionMode::Default),
        "accept_edits" => Ok(PermissionMode::AcceptEdits),
        "plan" => Ok(PermissionMode::Plan),
        "dont_ask" => Ok(PermissionMode::DontAsk),
        "bypass_permissions" => Ok(PermissionMode::BypassPermissions),
        other => Err(format!(
            "unknown permission_mode `{other}`; expected one of: default, accept_edits, plan, \
             dont_ask, bypass_permissions"
        )),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionResolveDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PermissionResolveRequest {
    pub decision: PermissionResolveDecision,
    /// Only meaningful when `decision` is `deny`.
    pub reason: Option<String>,
}

impl From<PermissionResolveRequest> for PermissionDecision {
    fn from(request: PermissionResolveRequest) -> Self {
        match request.decision {
            PermissionResolveDecision::AllowOnce => PermissionDecision::AllowOnce,
            PermissionResolveDecision::AllowAlways => PermissionDecision::AllowAlways,
            PermissionResolveDecision::Deny => PermissionDecision::Deny {
                reason: request.reason,
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            error: message.into(),
        }
    }
}
