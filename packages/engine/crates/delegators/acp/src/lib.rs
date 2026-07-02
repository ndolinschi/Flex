//! ACP delegator scaffolding.
//!
//! This crate defines only ACP-facing profiles and pure line mapping. It does
//! not launch ACP agents or register an adapter with the engine runtime.

mod mapper;
mod profile;
mod protocol;

pub use mapper::AcpLineMapper;
pub use profile::{AcpAgentProfile, AcpLaunchConfig};
pub use protocol::{
    AcpClientCapabilities, AcpJsonRpcId, AcpMcpServer, AcpNotification, AcpRequest,
    AcpSessionNewParams,
};

pub const ACP_AGENT_ID: &str = "acp";
