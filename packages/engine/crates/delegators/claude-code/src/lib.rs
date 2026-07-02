//! Claude Code delegator runtime slice.
//!
//! The crate can probe and launch a Claude Code compatible command, normalize
//! its JSON-line output into canonical events, and expose that bridge behind
//! the shared [`agentloop_core::Agent`] trait. The engine resolver does not
//! register it yet; live CLI coverage remains ignored/env-gated.

mod agent;
mod config;
mod host;
mod mapper;

pub use agent::ClaudeCodeAgent;
pub use config::{ClaudeCodeConfig, PromptTransport};
pub use host::TokioCommandHost;
pub use mapper::ClaudeCodeLineMapper;
// Probe outcomes surface to composition roots (resolver, doctor).
pub use agentloop_delegator_common::{DelegatorHostError, DelegatorProbeStatus};

pub const CLAUDE_CODE_AGENT_ID: &str = "claude-code";
