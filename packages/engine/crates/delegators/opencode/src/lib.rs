//! opencode delegator scaffolding.
//!
//! The crate is intentionally pure for now: launch profile data and line
//! mapping tests only, with no runtime adapter registered in the engine.

mod config;
mod mapper;

pub use config::{OpencodeConfig, OpencodeProfile, PromptTransport};
pub use mapper::OpencodeLineMapper;

pub const OPENCODE_AGENT_ID: &str = "opencode";
