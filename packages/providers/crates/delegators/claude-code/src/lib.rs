mod agent;
mod config;
mod host;
mod mapper;

pub use agent::{
    ClaudeCodeAgent, ClaudeCodeProfile, claude_code_agent, claude_code_agent_with_host,
    ephemeral_claude_code_agent,
};
pub use agentloop_delegator_common::{DelegatorHostError, DelegatorProbeStatus};
pub use config::{ClaudeCodeConfig, PromptTransport};
pub use host::TokioCommandHost;
pub use mapper::ClaudeCodeLineMapper;

pub const CLAUDE_CODE_AGENT_ID: &str = "claude-code";
