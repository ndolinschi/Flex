//! Wire DTOs for the desktop Remote Access HTTP API (snake_case).

use agentloop_contracts::{PermissionDecision, PermissionMode, SessionMeta};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub fallback_models: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
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

#[derive(Debug, Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
    pub permission_mode: Option<String>,
}

pub fn parse_permission_mode(value: &str) -> Result<PermissionMode, String> {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionResolveDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Deserialize)]
pub struct PermissionResolveRequest {
    pub decision: PermissionResolveDecision,
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

#[derive(Debug, Deserialize)]
pub struct QuestionAnswerDto {
    pub question: String,
    pub selected: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuestionRespondRequest {
    pub answers: Vec<QuestionAnswerDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerBody {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub secret_env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub secret_args: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub configured_secret_env: Vec<String>,
    #[serde(default)]
    pub has_secret_args: bool,
}

fn default_true() -> bool {
    true
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
