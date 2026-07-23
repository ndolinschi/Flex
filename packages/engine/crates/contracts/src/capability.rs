use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::permission::PermissionMode;

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ModelRef(pub String);

impl ModelRef {
    pub fn split(&self) -> (Option<&str>, &str) {
        match self.0.split_once('/') {
            Some((provider, model)) if !provider.is_empty() && !model.is_empty() => {
                (Some(provider), model)
            }
            _ => (None, &self.0),
        }
    }
}

impl From<&str> for ModelRef {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelDiscovery {
    Static { models: Vec<ModelInfo> },
    Dynamic,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PermissionCaps {
    pub interactive: bool,
    pub modes: Vec<PermissionMode>,
    pub tool_scoping: bool,
}

impl Default for PermissionCaps {
    fn default() -> Self {
        Self {
            interactive: false,
            modes: vec![PermissionMode::Default],
            tool_scoping: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StreamingGranularity {
    TokenDeltas,
    MessageLevel,
    SnapshotOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ResumeSupport {
    Native,
    Replay,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpPassthrough {
    Flag,
    SessionNew,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CancelSupport {
    Graceful,
    KillOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentCaps {
    pub images: bool,
    pub files: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CommandSource {
    Builtin,
    User,
    Project,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_hint: Option<String>,
    pub source: CommandSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModeInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentInfo {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentCaps {
    pub models: ModelDiscovery,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<ModeInfo>,
    pub permissions: PermissionCaps,
    pub reasoning_visible: bool,
    pub streaming: StreamingGranularity,
    pub resume: ResumeSupport,
    pub attachments: AttachmentCaps,
    pub mcp_passthrough: McpPassthrough,
    pub subagents: bool,
    pub cost_reporting: bool,
    pub cancellation: CancelSupport,
    pub emits_structured_events: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandInfo>,
}

impl Default for AgentCaps {
    fn default() -> Self {
        Self {
            models: ModelDiscovery::None,
            modes: Vec::new(),
            permissions: PermissionCaps::default(),
            reasoning_visible: false,
            streaming: StreamingGranularity::MessageLevel,
            resume: ResumeSupport::None,
            attachments: AttachmentCaps::default(),
            mcp_passthrough: McpPassthrough::None,
            subagents: false,
            cost_reporting: false,
            cancellation: CancelSupport::KillOnly,
            emits_structured_events: true,
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderCaps {
    pub tool_use: bool,
    pub parallel_tool_use: bool,
    pub vision: bool,
    pub documents: bool,
    pub thinking: bool,
    pub prompt_caching: bool,
    pub native_json_schema_tools: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u32>,
}
