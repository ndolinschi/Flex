//! Grok Build delegator: drives `grok -p --output-format streaming-json`
//! and normalizes its output. ACP (`grok agent stdio`) remains available via
//! `--agent acp --agent-cmd "grok agent stdio"`.

mod config;
mod mapper;
mod profile;

pub use config::{GrokConfig, GrokProfile};
pub use mapper::GrokLineMapper;
pub use profile::{
    GrokAgent, GrokRuntimeProfile, ephemeral_grok_agent, grok_agent, grok_agent_with_host,
};

pub const GROK_AGENT_ID: &str = "grok";
