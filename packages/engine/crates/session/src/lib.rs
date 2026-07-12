//! [`SessionStore`](agentloop_core::SessionStore) implementations.
//!
//! The session log is the ground truth: an append-only sequence of persisted
//! [`AgentEvent`](agentloop_contracts::AgentEvent)s per session, addressed by
//! gapless per-session sequence numbers that the store itself assigns.
//!
//! Implementations:
//! - [`MemoryStore`] — in-memory, for tests and embedded use (M1).
//! - [`JsonlStore`] — append-only files, one JSONL log per session.

mod jsonl;
mod memory;

pub use jsonl::JsonlStore;
pub use memory::MemoryStore;
