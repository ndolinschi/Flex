//! MCP bridge scaffolding.
//!
//! This crate owns MCP configuration, pure tool descriptor mapping, and task
//! spawn interfaces. It intentionally does not open sockets or launch MCP
//! servers yet; concrete clients can implement the traits here without leaking
//! MCP wire details into `core` or `loop`.

pub mod bridge;
pub mod config;
pub mod task;

pub use bridge::{
    DEFAULT_TOOL_NAME_SEPARATOR, McpBridgeError, McpContent, McpRemoteTool, McpToolCall,
    McpToolClient, McpToolRef, McpToolResult, bridge_tool_name, descriptors_for,
};
pub use config::{
    McpBridgeConfig, McpConfigError, McpServerConfig, McpServerTransport, StdioServerConfig,
    StreamableHttpConfig,
};
pub use task::{
    DisabledTaskSpawner, TaskHandle, TaskProfile, TaskSpawnError, TaskSpawnRequest, TaskSpawner,
};
