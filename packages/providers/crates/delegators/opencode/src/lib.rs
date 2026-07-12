//! opencode delegator: drives `opencode run --json` and normalizes its output.

mod config;
mod mapper;
mod profile;

pub use config::{OpencodeConfig, OpencodeProfile, PromptTransport};
pub use mapper::OpencodeLineMapper;
pub use profile::{
    OpencodeAgent, OpencodeRuntimeProfile, ephemeral_opencode_agent, opencode_agent,
    opencode_agent_with_host,
};

pub const OPENCODE_AGENT_ID: &str = "opencode";
