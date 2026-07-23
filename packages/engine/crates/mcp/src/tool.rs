use std::sync::Arc;

use async_trait::async_trait;

use agentloop_contracts::ToolOutput;
use agentloop_core::{Tool, ToolContext, ToolDescriptor, ToolError};

use crate::bridge::{McpBridgeError, McpToolCall, McpToolClient, McpToolRef};
use crate::config::McpServerConfig;

pub struct McpBridgedTool {
    descriptor: ToolDescriptor,
    client: Arc<dyn McpToolClient>,
    tool_ref: McpToolRef,
    _server: McpServerConfig,
}

impl McpBridgedTool {
    pub fn new(
        descriptor: ToolDescriptor,
        client: Arc<dyn McpToolClient>,
        server: McpServerConfig,
        remote_tool_name: impl Into<String>,
    ) -> Self {
        let remote_tool_name = remote_tool_name.into();
        Self {
            tool_ref: McpToolRef::new(server.name.clone(), remote_tool_name),
            descriptor,
            client,
            _server: server,
        }
    }

    pub fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }
}

#[async_trait]
impl Tool for McpBridgedTool {
    fn descriptor(&self) -> ToolDescriptor {
        self.descriptor.clone()
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        let call = McpToolCall {
            tool_ref: self.tool_ref.clone(),
            input,
        };

        match self.client.call_tool(call, ctx.cancel).await {
            Ok(output) => Ok(output),
            Err(err) => Err(map_bridge_error(err)),
        }
    }
}

fn map_bridge_error(err: McpBridgeError) -> ToolError {
    match err {
        McpBridgeError::Cancelled => ToolError::Cancelled,
        McpBridgeError::ToolCall(message) | McpBridgeError::Transport { message, .. } => {
            ToolError::Execution(message)
        }
        McpBridgeError::Connection { message, .. } => ToolError::Execution(message),
        McpBridgeError::InvalidHeader { message, .. } => ToolError::Execution(message),
        McpBridgeError::UnknownServer { server } => ToolError::Execution(format!(
            "MCP server `{server}` is not connected; reload MCP configuration and retry."
        )),
        McpBridgeError::UnknownBridgedTool { name } => ToolError::InvalidInput(format!(
            "Unknown MCP tool `{name}`; list available MCP tools and retry."
        )),
        McpBridgeError::Config(err) => ToolError::Execution(err.to_string()),
        McpBridgeError::EmptyToolName { server } => ToolError::Execution(format!(
            "MCP server `{server}` returned a tool with an empty name."
        )),
        McpBridgeError::DuplicateBridgeToolName { name } => ToolError::Execution(format!(
            "Duplicate MCP tool name `{name}` in configuration."
        )),
    }
}

pub fn parse_bridge_tool_name(
    servers: &[McpServerConfig],
    bridged_name: &str,
) -> Result<(McpServerConfig, String), McpBridgeError> {
    use crate::bridge::DEFAULT_TOOL_NAME_SEPARATOR;

    for server in servers {
        let prefix = format!(
            "{prefix}{DEFAULT_TOOL_NAME_SEPARATOR}",
            prefix = server.tool_prefix()
        );
        if let Some(remote) = bridged_name.strip_prefix(&prefix) {
            if remote.is_empty() {
                return Err(McpBridgeError::UnknownBridgedTool {
                    name: bridged_name.to_owned(),
                });
            }
            return Ok((server.clone(), remote.to_owned()));
        }
    }

    Err(McpBridgeError::UnknownBridgedTool {
        name: bridged_name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpBridgeConfig;

    #[test]
    fn parses_bridged_tool_name() {
        let config = McpBridgeConfig {
            servers: vec![McpServerConfig::stdio("docs", "mcp-docs")],
        };
        let (server, remote) =
            parse_bridge_tool_name(&config.servers, "docs__search").expect("parse bridged name");
        assert_eq!(server.name, "docs");
        assert_eq!(remote, "search");
    }
}
