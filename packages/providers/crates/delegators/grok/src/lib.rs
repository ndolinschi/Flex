mod config;
mod mapper;
mod profile;

pub use config::{GrokConfig, GrokProfile};
pub use mapper::GrokLineMapper;
pub use profile::{
    GrokAgent, GrokRuntimeProfile, ephemeral_grok_agent, grok_agent, grok_agent_with_host,
};

pub const GROK_AGENT_ID: &str = "grok";
