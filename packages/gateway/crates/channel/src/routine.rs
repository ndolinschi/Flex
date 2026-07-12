//! Routines: saved goal configurations run by a trigger (cron schedule or
//! webhook) instead of a human sending a prompt. Contract-only, like
//! [`crate::Channel`] — the runner that actually drives `EngineService` from
//! these specs is a composition-time concern (see `agentloop_sdk::routines`),
//! not something this crate needs to know how to do.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{GoalOutcome, GoalSpec, NewSessionParams, SessionId};

/// A saved routine: what to run (`goal`, seeded into a fresh session via
/// `session_seed`) and when to run it (`trigger`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineSpec {
    /// Stable identifier, unique per store (kebab-case recommended).
    pub id: String,
    pub goal: GoalSpec,
    /// Model/role/isolation etc. for each fresh session the routine opens.
    pub session_seed: NewSessionParams,
    pub trigger: RoutineTrigger,
}

/// What causes a routine to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RoutineTrigger {
    /// Standard 5-field cron expression, evaluated in the runner's local
    /// timezone (UTC in most server deployments).
    Cron { expr: String },
    /// Runs only when triggered by `POST /routines/{id}/trigger` on the
    /// running `flex serve` instance — `path` is reserved for a future
    /// per-routine custom mount point and unused today (every routine
    /// shares the same `/routines/{id}/trigger` route, keyed by `id`).
    Webhook { path: String },
}

/// One recorded run of a routine, for history/observability. Not part of the
/// session's own event log — the session it opens is a perfectly normal
/// session with its own log; this is metadata about *the routine*, kept by
/// the store instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRunRecord {
    pub session_id: SessionId,
    pub started_ms: u64,
    pub outcome: GoalOutcome,
}

/// Routine persistence. `agentloop_sdk` ships a file-backed implementation
/// (`~/.config/agentloop/routines/*.toml`, mirroring the `learning` plugin's
/// skill/memory directories); a client embedding the SDK differently may
/// supply its own.
#[async_trait]
pub trait RoutineStore: Send + Sync {
    async fn list(&self) -> Result<Vec<RoutineSpec>, RoutineError>;
    async fn get(&self, id: &str) -> Result<Option<RoutineSpec>, RoutineError>;
    /// Insert or replace a routine by id.
    async fn upsert(&self, spec: RoutineSpec) -> Result<(), RoutineError>;
    async fn remove(&self, id: &str) -> Result<(), RoutineError>;
    /// Best-effort: a run should still be considered successful even if its
    /// history couldn't be recorded.
    async fn record_run(&self, id: &str, record: RoutineRunRecord) -> Result<(), RoutineError>;
}

/// Routine store failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoutineError {
    #[error("routine `{0}` not found")]
    NotFound(String),
    #[error("{0}")]
    Storage(String),
}
