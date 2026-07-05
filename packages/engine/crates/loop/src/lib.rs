//! The native agent loop — the engine's own [`Agent`](agentloop_core::Agent)
//! implementation over any provider, tool registry, and session store.
//!
//! Turn shape: prompt → model streams (deltas broadcast live) → tool calls
//! execute (consecutive read-only calls concurrently, mutating calls
//! sequentially, every call a tracked `ToolCall` with permission gating and
//! timing) → results feed back → repeat until the model stops, a bound hits,
//! or the turn is cancelled. Every step lands in the append-only session log;
//! metrics and tracing derive from the same events.

mod agent;
mod attachments;
mod builder;
mod compaction;
mod context_budget;
mod deps;
mod draft;
pub mod effort;
mod manager;
mod messages;
mod permission;
mod pool;
pub mod roles;
mod rules;
mod session_handle;
mod subagent;
mod tool_results;
mod turn;

pub use agent::NativeAgent;
pub use builder::{LoopLimits, NativeAgentBuilder};
pub use manager::{InvalidTransition, ToolCallManager};
pub use messages::transcript_to_messages;
pub use permission::{PermissionPolicy, Verdict};
pub use rules::{CallFacts, any_rule_matches, rule_matches};
