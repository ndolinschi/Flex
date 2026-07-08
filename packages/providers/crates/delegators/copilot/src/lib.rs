//! GitHub Copilot CLI delegator.
//!
//! Drives the `copilot` CLI (npm `@github/copilot`) in its programmatic mode
//! (`copilot -p "<prompt>"`). Copilot prints plain markdown — no structured
//! event stream — so this is the degenerate-but-valid case of the unified
//! stream format: the whole response materializes as one assistant message
//! per turn (`emits_structured_events: false`, snapshot-only streaming).
//!
//! Tool permissions are conservative by default: Copilot's own tool use stays
//! disabled unless [`CopilotConfig::allow_all_tools`] is explicitly enabled,
//! because a delegated run has no interactive approval channel.

mod config;
mod mapper;
mod profile;

pub use config::CopilotConfig;
pub use mapper::CopilotLineMapper;
pub use profile::{
    CopilotAgent, CopilotProfile, copilot_agent, copilot_agent_with_host, ephemeral_copilot_agent,
};

pub const COPILOT_AGENT_ID: &str = "copilot";
