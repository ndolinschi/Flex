//! Cursor delegator: drives `cursor-agent --print --output-format stream-json`
//! and normalizes its output. The workspace-bridge integration remains a
//! profile placeholder that returns actionable errors.

mod config;
mod mapper;
mod profile;

pub use config::{
    CursorCliConfig, CursorConfigError, CursorIntegrationKind, CursorLaunchConfig, CursorProfile,
};
pub use mapper::CursorLineMapper;
pub use profile::{
    CursorAgent, CursorRuntimeProfile, cursor_agent, cursor_agent_with_host, ephemeral_cursor_agent,
};

pub const CURSOR_AGENT_ID: &str = "cursor";
