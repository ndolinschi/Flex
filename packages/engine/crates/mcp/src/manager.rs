use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use agentloop_core::{Tool, ToolDescriptor, ToolRegistry};

use crate::bridge::{
    McpBridgeError, McpRemoteTool, McpToolCall, McpToolClient, McpToolRef, descriptors_for,
};
use crate::client::RmcpToolClient;
use crate::config::{McpBridgeConfig, McpServerConfig};
use crate::tool::McpBridgedTool;

pub struct McpManager {
    config: McpBridgeConfig,
    client: Arc<dyn McpToolClient>,
    tools: Vec<Arc<dyn Tool>>,
    descriptors: Vec<ToolDescriptor>,
}

impl McpManager {
    pub fn empty(client: Arc<dyn McpToolClient>) -> Self {
        Self {
            config: McpBridgeConfig::default(),
            client,
            tools: Vec::new(),
            descriptors: Vec::new(),
        }
    }

    pub async fn from_config(
        config: McpBridgeConfig,
        client: Arc<dyn McpToolClient>,
    ) -> Result<Self, McpBridgeError> {
        config.validate()?;
        let cancel = CancellationToken::new();
        let mut remote_tools = Vec::new();

        for server in config.enabled_servers() {
            let span = tracing::info_span!("mcp.register_server", server = %server.name);
            let _guard = span.enter();
            match client.list_tools(server, cancel.child_token()).await {
                Ok(tools) => remote_tools.extend(tools),
                Err(err) => {
                    tracing::warn!(
                        server = %server.name,
                        error = %err,
                        "failed to list MCP tools; server skipped"
                    );
                }
            }
        }

        Self::from_remote_tools(config, client, remote_tools)
    }

    pub fn from_config_blocking(
        config: McpBridgeConfig,
        client: Arc<dyn McpToolClient>,
    ) -> Result<Self, McpBridgeError> {
        if config.servers.is_empty() {
            return Ok(Self::empty(client));
        }

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                tokio::task::block_in_place(|| handle.block_on(Self::from_config(config, client)))
            }
            Err(_) => {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| McpBridgeError::Connection {
                        server: "runtime".to_owned(),
                        message: err.to_string(),
                    })?;
                runtime.block_on(Self::from_config(config, client))
            }
        }
    }

    pub fn from_config_blocking_default(config: McpBridgeConfig) -> Result<Self, McpBridgeError> {
        let enabled: Vec<_> = config.enabled_servers().cloned().collect();
        let client = Arc::new(RmcpToolClient::from_configs(&enabled));
        Self::from_config_blocking(config, client)
    }

    fn from_remote_tools(
        config: McpBridgeConfig,
        client: Arc<dyn McpToolClient>,
        remote_tools: Vec<McpRemoteTool>,
    ) -> Result<Self, McpBridgeError> {
        let descriptors = descriptors_for(&config, &remote_tools)?;
        let servers = config
            .servers
            .iter()
            .map(|server| (server.name.as_str(), server))
            .collect::<std::collections::BTreeMap<_, _>>();

        let tools = descriptors
            .iter()
            .zip(remote_tools.iter())
            .map(|(descriptor, remote)| {
                let server = servers
                    .get(remote.server_name.as_str())
                    .copied()
                    .ok_or_else(|| McpBridgeError::UnknownServer {
                        server: remote.server_name.clone(),
                    })?;
                Ok(Arc::new(McpBridgedTool::new(
                    descriptor.clone(),
                    client.clone(),
                    server.clone(),
                    remote.name.clone(),
                )) as Arc<dyn Tool>)
            })
            .collect::<Result<Vec<_>, McpBridgeError>>()?;

        Ok(Self {
            config,
            client,
            tools,
            descriptors,
        })
    }

    pub fn config(&self) -> &McpBridgeConfig {
        &self.config
    }

    pub fn descriptors(&self) -> &[ToolDescriptor] {
        &self.descriptors
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect()
    }

    pub fn register_tools(&self, registry: &mut ToolRegistry) {
        for tool in &self.tools {
            registry.register(tool.clone());
        }
    }

    pub async fn reload(&mut self, config: McpBridgeConfig) -> Result<(), McpBridgeError> {
        self.shutdown().await;
        *self = Self::from_config(config, self.client.clone()).await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }

    pub fn server_config(&self, name: &str) -> Option<&McpServerConfig> {
        self.config
            .servers
            .iter()
            .find(|server| server.name == name)
    }

    pub async fn list_server_tools(
        &self,
        server_name: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<McpRemoteTool>, McpBridgeError> {
        let server =
            self.server_config(server_name)
                .ok_or_else(|| McpBridgeError::UnknownServer {
                    server: server_name.to_owned(),
                })?;
        self.client.list_tools(server, cancel).await
    }

    pub async fn call_server_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<agentloop_contracts::ToolOutput, McpBridgeError> {
        let server =
            self.server_config(server_name)
                .ok_or_else(|| McpBridgeError::UnknownServer {
                    server: server_name.to_owned(),
                })?;
        self.client
            .call_tool(
                McpToolCall {
                    tool_ref: McpToolRef::new(&server.name, tool_name),
                    input,
                },
                cancel,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::bridge::McpToolCall;
    use crate::config::{McpServerConfig, McpServerTransport, StreamableHttpConfig};
    use agentloop_contracts::ToolOutput;

    struct MockMcpClient {
        tools_by_server: BTreeMap<String, Vec<McpRemoteTool>>,
    }

    #[async_trait]
    impl McpToolClient for MockMcpClient {
        async fn list_tools(
            &self,
            server: &McpServerConfig,
            _cancel: CancellationToken,
        ) -> Result<Vec<McpRemoteTool>, McpBridgeError> {
            Ok(self
                .tools_by_server
                .get(&server.name)
                .cloned()
                .unwrap_or_default())
        }

        async fn call_tool(
            &self,
            call: McpToolCall,
            _cancel: CancellationToken,
        ) -> Result<ToolOutput, McpBridgeError> {
            Ok(ToolOutput::text(format!(
                "called {}__{}",
                call.tool_ref.server_name, call.tool_ref.tool_name
            )))
        }
    }

    fn sample_config() -> McpBridgeConfig {
        McpBridgeConfig {
            servers: vec![
                McpServerConfig::stdio("enabled", "mcp-enabled"),
                McpServerConfig {
                    name: "disabled".to_owned(),
                    display_name: None,
                    enabled: false,
                    transport: McpServerTransport::StreamableHttp(StreamableHttpConfig {
                        url: "https://example.test/mcp".to_owned(),
                        headers: BTreeMap::new(),
                    }),
                    tool_name_prefix: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn registers_only_enabled_server_tools() {
        let client = Arc::new(MockMcpClient {
            tools_by_server: BTreeMap::from([
                (
                    "enabled".to_owned(),
                    vec![McpRemoteTool::new("enabled", "search")],
                ),
                (
                    "disabled".to_owned(),
                    vec![McpRemoteTool::new("disabled", "ignored")],
                ),
            ]),
        });

        let manager = McpManager::from_config(sample_config(), client)
            .await
            .expect("manager should load");

        assert_eq!(manager.tool_names(), vec!["enabled__search".to_owned()]);
        assert_eq!(manager.descriptors().len(), 1);
    }

    #[test]
    fn blocking_init_registers_enabled_tools_only() {
        let client = Arc::new(MockMcpClient {
            tools_by_server: BTreeMap::from([(
                "enabled".to_owned(),
                vec![McpRemoteTool::new("enabled", "fetch")],
            )]),
        });

        let manager =
            McpManager::from_config_blocking(sample_config(), client).expect("blocking init");

        assert_eq!(manager.tool_names(), vec!["enabled__fetch".to_owned()]);
    }

    #[tokio::test]
    async fn reload_replaces_tool_set() {
        let client = Arc::new(MockMcpClient {
            tools_by_server: BTreeMap::from([(
                "enabled".to_owned(),
                vec![McpRemoteTool::new("enabled", "one")],
            )]),
        });

        let mut manager = McpManager::from_config(sample_config(), client.clone())
            .await
            .expect("initial load");
        assert_eq!(manager.tool_names(), vec!["enabled__one".to_owned()]);

        let updated = McpBridgeConfig {
            servers: vec![McpServerConfig::stdio("enabled", "mcp-enabled")],
        };
        manager.reload(updated).await.expect("reload");
        assert_eq!(manager.tool_names(), vec!["enabled__one".to_owned()]);
    }
}
