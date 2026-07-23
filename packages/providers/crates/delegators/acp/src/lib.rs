mod agent;
mod client;
mod mapper;
mod profile;
mod protocol;

pub use agent::{AcpAgent, acp_agent};
pub use client::{AcpClient, AcpClientError, AcpPermissionPolicy};
pub use mapper::AcpLineMapper;
pub use profile::{AcpAgentProfile, AcpLaunchConfig};
pub use protocol::{
    AcpClientCapabilities, AcpJsonRpcId, AcpMcpServer, AcpNotification, AcpRequest,
    AcpSessionNewParams,
};

pub const ACP_AGENT_ID: &str = "acp";
