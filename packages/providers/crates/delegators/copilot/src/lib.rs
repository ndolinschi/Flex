mod config;
mod mapper;
mod profile;

pub use config::CopilotConfig;
pub use mapper::CopilotLineMapper;
pub use profile::{
    CopilotAgent, CopilotProfile, copilot_agent, copilot_agent_with_host, ephemeral_copilot_agent,
};

pub const COPILOT_AGENT_ID: &str = "copilot";
