use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, ToolCategory, ToolDescriptor};

use crate::config::{McpBridgeConfig, McpConfigError, McpServerConfig};

pub const DEFAULT_TOOL_NAME_SEPARATOR: &str = "__";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpToolRef {
    pub server_name: String,
    pub tool_name: String,
}

impl McpToolRef {
    pub fn new(server_name: impl Into<String>, tool_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            tool_name: tool_name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct McpRemoteTool {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// Conservative until a server declares a stronger capability.
    #[serde(default)]
    pub read_only: bool,
}

impl McpRemoteTool {
    pub fn new(server_name: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            name: name.into(),
            description: String::new(),
            input_schema: serde_json::json!({ "type": "object" }),
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct McpToolCall {
    pub tool_ref: McpToolRef,
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct McpToolResult {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
}

impl McpToolResult {
    pub fn into_tool_output(self) -> ToolOutput {
        let content = self
            .content
            .into_iter()
            .map(McpContent::into_tool_result_block)
            .collect();
        ToolOutput {
            content,
            is_error: self.is_error,
            structured: self.structured,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpContent {
    Text { text: String },
    Json { value: serde_json::Value },
}

impl McpContent {
    fn into_tool_result_block(self) -> ToolResultBlock {
        match self {
            Self::Text { text } => ToolResultBlock::markdown(text),
            Self::Json { value } => ToolResultBlock::Json { value },
        }
    }
}

#[async_trait]
pub trait McpToolClient: Send + Sync {
    async fn list_tools(
        &self,
        server: &McpServerConfig,
        cancel: CancellationToken,
    ) -> Result<Vec<McpRemoteTool>, McpBridgeError>;

    async fn call_tool(
        &self,
        call: McpToolCall,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, McpBridgeError>;
}

pub fn bridge_tool_name(
    server: &McpServerConfig,
    remote_tool_name: &str,
) -> Result<String, McpBridgeError> {
    if remote_tool_name.trim().is_empty() {
        return Err(McpBridgeError::EmptyToolName {
            server: server.name.clone(),
        });
    }
    Ok(format!(
        "{}{DEFAULT_TOOL_NAME_SEPARATOR}{remote_tool_name}",
        server.tool_prefix()
    ))
}

pub fn descriptors_for<'a>(
    config: &McpBridgeConfig,
    remote_tools: impl IntoIterator<Item = &'a McpRemoteTool>,
) -> Result<Vec<ToolDescriptor>, McpBridgeError> {
    config.validate()?;
    let servers = config
        .enabled_servers()
        .map(|server| (server.name.as_str(), server))
        .collect::<BTreeMap<_, _>>();
    let mut names = BTreeSet::new();
    let mut descriptors = Vec::new();

    for remote in remote_tools {
        let Some(server) = servers.get(remote.server_name.as_str()) else {
            return Err(McpBridgeError::UnknownServer {
                server: remote.server_name.clone(),
            });
        };
        let descriptor = descriptor_for(server, remote)?;
        if !names.insert(descriptor.name.clone()) {
            return Err(McpBridgeError::DuplicateBridgeToolName {
                name: descriptor.name,
            });
        }
        descriptors.push(descriptor);
    }

    Ok(descriptors)
}

fn descriptor_for(
    server: &McpServerConfig,
    remote: &McpRemoteTool,
) -> Result<ToolDescriptor, McpBridgeError> {
    let name = bridge_tool_name(server, &remote.name)?;
    let description = if remote.description.trim().is_empty() {
        format!(
            "Calls MCP tool `{}` from server `{}`.",
            remote.name, remote.server_name
        )
    } else {
        format!(
            "Calls MCP tool `{}` from server `{}`.\n\n{}",
            remote.name, remote.server_name, remote.description
        )
    };

    Ok(ToolDescriptor {
        name,
        description,
        input_schema: remote.input_schema.clone(),
        read_only: remote.read_only,
        category: ToolCategory::Mcp,
        needs_permission: if remote.read_only {
            PermissionHint::Never
        } else {
            PermissionHint::IfMutating
        },
    })
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpBridgeError {
    #[error(transparent)]
    Config(#[from] McpConfigError),
    #[error("MCP server `{server}` is not configured or is disabled")]
    UnknownServer { server: String },
    #[error("MCP server `{server}` returned a tool with an empty name")]
    EmptyToolName { server: String },
    #[error("duplicate bridged MCP tool name `{name}`")]
    DuplicateBridgeToolName { name: String },
    #[error("MCP bridge runtime is not implemented yet: {hint}")]
    NotImplemented { hint: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::McpServerConfig;

    #[test]
    fn maps_remote_tool_to_mcp_descriptor() {
        let config = McpBridgeConfig {
            servers: vec![McpServerConfig::stdio("docs", "mcp-docs")],
        };
        let tools = vec![McpRemoteTool {
            server_name: "docs".to_owned(),
            name: "search".to_owned(),
            description: "Search project docs.".to_owned(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
            read_only: true,
        }];

        let descriptors = descriptors_for(&config, &tools);

        match descriptors {
            Ok(descriptors) => {
                assert_eq!(descriptors.len(), 1);
                assert_eq!(descriptors[0].name, "docs__search");
                assert_eq!(descriptors[0].category, ToolCategory::Mcp);
                assert_eq!(descriptors[0].needs_permission, PermissionHint::Never);
                assert!(descriptors[0].description.contains("Search project docs."));
            }
            Err(err) => panic!("descriptor mapping should succeed: {err}"),
        }
    }

    #[test]
    fn detects_colliding_bridge_names() {
        let mut server = McpServerConfig::stdio("one", "mcp-one");
        server.tool_name_prefix = Some("shared".to_owned());
        let mut other = McpServerConfig::stdio("two", "mcp-two");
        other.tool_name_prefix = Some("shared".to_owned());
        let config = McpBridgeConfig {
            servers: vec![server, other],
        };
        let tools = vec![
            McpRemoteTool::new("one", "search"),
            McpRemoteTool::new("two", "search"),
        ];

        assert!(matches!(
            descriptors_for(&config, &tools),
            Err(McpBridgeError::DuplicateBridgeToolName { name }) if name == "shared__search"
        ));
    }

    #[test]
    fn converts_mcp_result_to_tool_output() {
        let result = McpToolResult {
            content: vec![
                McpContent::Text {
                    text: "hello".to_owned(),
                },
                McpContent::Json {
                    value: serde_json::json!({ "ok": true }),
                },
            ],
            is_error: false,
            structured: Some(serde_json::json!({ "count": 1 })),
        };

        let output = result.into_tool_output();

        assert_eq!(output.render_text(), "hello\n{\n  \"ok\": true\n}");
        assert!(!output.is_error);
        assert_eq!(output.structured, Some(serde_json::json!({ "count": 1 })));
    }
}
