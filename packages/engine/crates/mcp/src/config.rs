use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpBridgeConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<McpServerConfig>,
}

impl McpBridgeConfig {
    pub fn validate(&self) -> Result<(), McpConfigError> {
        let mut seen = BTreeSet::new();
        for server in &self.servers {
            server.validate()?;
            if !seen.insert(server.name.clone()) {
                return Err(McpConfigError::DuplicateServer {
                    name: server.name.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn enabled_servers(&self) -> impl Iterator<Item = &McpServerConfig> {
        self.servers.iter().filter(|server| server.enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(flatten)]
    pub transport: McpServerTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name_prefix: Option<String>,
}

impl McpServerConfig {
    pub fn stdio(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            enabled: true,
            transport: McpServerTransport::Stdio(StdioServerConfig {
                command: command.into(),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
            }),
            tool_name_prefix: None,
        }
    }

    pub fn tool_prefix(&self) -> &str {
        self.tool_name_prefix
            .as_deref()
            .unwrap_or(self.name.as_str())
    }

    pub fn validate(&self) -> Result<(), McpConfigError> {
        if self.name.trim().is_empty() {
            return Err(McpConfigError::EmptyServerName);
        }
        if self.tool_prefix().trim().is_empty() {
            return Err(McpConfigError::EmptyToolPrefix {
                server: self.name.clone(),
            });
        }
        self.transport.validate(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "transport", rename_all = "snake_case")]
#[non_exhaustive]
pub enum McpServerTransport {
    Stdio(StdioServerConfig),
    StreamableHttp(StreamableHttpConfig),
    Sse(StreamableHttpConfig),
}

impl McpServerTransport {
    fn validate(&self, server: &str) -> Result<(), McpConfigError> {
        match self {
            Self::Stdio(config) if config.command.trim().is_empty() => {
                Err(McpConfigError::MissingCommand {
                    server: server.to_owned(),
                })
            }
            Self::StreamableHttp(config) | Self::Sse(config) if config.url.trim().is_empty() => {
                Err(McpConfigError::MissingUrl {
                    server: server.to_owned(),
                })
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StdioServerConfig {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StreamableHttpConfig {
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpConfigError {
    #[error("MCP server name cannot be empty")]
    EmptyServerName,
    #[error("MCP server `{server}` has an empty tool name prefix")]
    EmptyToolPrefix { server: String },
    #[error("duplicate MCP server `{name}`")]
    DuplicateServer { name: String },
    #[error("MCP stdio server `{server}` is missing a command")]
    MissingCommand { server: String },
    #[error("MCP HTTP/SSE server `{server}` is missing a URL")]
    MissingUrl { server: String },
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_distinct_enabled_servers() {
        let config = McpBridgeConfig {
            servers: vec![
                McpServerConfig::stdio("fs", "mcp-files"),
                McpServerConfig {
                    name: "docs".to_owned(),
                    display_name: Some("Docs".to_owned()),
                    enabled: false,
                    transport: McpServerTransport::StreamableHttp(StreamableHttpConfig {
                        url: "https://example.test/mcp".to_owned(),
                        headers: BTreeMap::new(),
                    }),
                    tool_name_prefix: None,
                },
            ],
        };

        assert!(config.validate().is_ok());
        assert_eq!(config.enabled_servers().count(), 1);
    }

    #[test]
    fn rejects_duplicate_server_names() {
        let config = McpBridgeConfig {
            servers: vec![
                McpServerConfig::stdio("fs", "mcp-files"),
                McpServerConfig::stdio("fs", "other"),
            ],
        };

        assert!(matches!(
            config.validate(),
            Err(McpConfigError::DuplicateServer { name }) if name == "fs"
        ));
    }

    #[test]
    fn rejects_missing_transport_details() {
        let config = McpBridgeConfig {
            servers: vec![McpServerConfig::stdio("fs", "")],
        };

        assert!(matches!(
            config.validate(),
            Err(McpConfigError::MissingCommand { server }) if server == "fs"
        ));
    }
}
