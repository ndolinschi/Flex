//! MCP bridge: configuration, rmcp client, dynamic tool registration, and task
//! spawn interfaces.

pub mod bridge;
pub mod client;
pub mod config;
pub mod manager;
pub mod task;
pub mod tool;

pub use bridge::{
    DEFAULT_TOOL_NAME_SEPARATOR, McpBridgeError, McpContent, McpRemoteTool, McpToolCall,
    McpToolClient, McpToolRef, McpToolResult, bridge_tool_name, descriptors_for,
};
pub use client::RmcpToolClient;
pub use config::{
    McpBridgeConfig, McpConfigError, McpServerConfig, McpServerTransport, StdioServerConfig,
    StreamableHttpConfig,
};
pub use manager::McpManager;
pub use task::{
    DisabledTaskSpawner, TaskHandle, TaskProfile, TaskSpawnError, TaskSpawnRequest, TaskSpawner,
};
pub use tool::{McpBridgedTool, parse_bridge_tool_name};
