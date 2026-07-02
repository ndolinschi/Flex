//! Capability declarations — how agents and providers advertise what they
//! support, so clients render only what exists and the engine degrades
//! explicitly instead of silently.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::permission::PermissionMode;

/// Reference to a model, optionally provider-qualified:
/// `"anthropic/claude-sonnet-4-5"` or a bare `"claude-sonnet-4-5"`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ModelRef(pub String);

impl ModelRef {
    /// Split into `(provider, model)` if provider-qualified.
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

/// One selectable model.
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

/// How an agent's model list is discovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelDiscovery {
    /// A fixed list known up front.
    Static { models: Vec<ModelInfo> },
    /// Probed at runtime (e.g. opencode's `/config/providers`).
    Dynamic,
    /// The agent owns model selection; the engine cannot choose.
    None,
}

/// Permission-related capabilities of an agent implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PermissionCaps {
    /// Can surface live permission round-trips (vs static policy only).
    pub interactive: bool,
    /// Which modes the agent honors.
    pub modes: Vec<PermissionMode>,
    /// Supports `ToolName(specifier)` scoping.
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

/// Granularity of the event stream an agent emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StreamingGranularity {
    /// Token-level deltas.
    TokenDeltas,
    /// Whole messages as they complete.
    MessageLevel,
    /// Full-text snapshots that supersede earlier ones.
    SnapshotOnly,
}

/// Whether and how a session can be resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ResumeSupport {
    /// The backing agent resumes its own session natively.
    Native,
    /// Resumed by replaying seed history into a fresh session.
    Replay,
    None,
}

/// How MCP server configuration reaches a delegated agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpPassthrough {
    /// Via a launch flag (e.g. `--mcp-config`).
    Flag,
    /// Via the protocol's session setup (e.g. ACP `session/new.mcpServers`).
    SessionNew,
    None,
}

/// How a turn can be interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CancelSupport {
    /// Clean interrupt; the session survives.
    Graceful,
    /// Only by killing the backing process.
    KillOnly,
}

/// Which attachment kinds a prompt may carry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentCaps {
    pub images: bool,
    pub files: bool,
}

/// Where a slash command comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CommandSource {
    /// Shipped with the engine (template or engine-control command).
    Builtin,
    /// From the user's config directory.
    User,
    /// From the project's config directory.
    Project,
    /// Native to a delegated agent, passed through.
    Agent,
}

/// One available slash command, declared so clients can render autocomplete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandInfo {
    /// Name without the leading slash.
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_hint: Option<String>,
    pub source: CommandSource,
}

/// An operating mode a delegated agent exposes (e.g. build/plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModeInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Identity of an agent implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentInfo {
    /// Stable key: `"native"`, `"claude-code"`, `"acp:gemini"`, ...
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Everything an `Agent` implementation declares about itself.
///
/// Merged from static profile defaults and startup probes; flows to clients
/// verbatim via the `Hello` handshake and transport initialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentCaps {
    pub models: ModelDiscovery,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<ModeInfo>,
    pub permissions: PermissionCaps,
    /// Whether reasoning/thinking output is visible in the stream.
    pub reasoning_visible: bool,
    pub streaming: StreamingGranularity,
    pub resume: ResumeSupport,
    pub attachments: AttachmentCaps,
    pub mcp_passthrough: McpPassthrough,
    pub subagents: bool,
    pub cost_reporting: bool,
    pub cancellation: CancelSupport,
    /// False for agents that emit only plain markdown (no structured events).
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

/// Everything a `Provider` implementation declares about itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderCaps {
    pub tool_use: bool,
    pub parallel_tool_use: bool,
    pub vision: bool,
    /// Native document/file input (e.g. Anthropic PDF blocks).
    pub documents: bool,
    pub thinking: bool,
    pub prompt_caching: bool,
    /// Accepts full JSON Schema in tool definitions without lossy conversion.
    pub native_json_schema_tools: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u32>,
}
