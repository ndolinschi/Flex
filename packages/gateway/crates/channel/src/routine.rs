use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use agentloop_contracts::{GoalOutcome, GoalSpec, NewSessionParams, SessionId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineSpec {
    pub id: String,
    pub goal: GoalSpec,
    pub session_seed: NewSessionParams,
    pub trigger: RoutineTrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RoutineTrigger {
    Cron { expr: String },
    Webhook { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRunRecord {
    pub session_id: SessionId,
    pub started_ms: u64,
    pub outcome: GoalOutcome,
}

#[async_trait]
pub trait RoutineStore: Send + Sync {
    async fn list(&self) -> Result<Vec<RoutineSpec>, RoutineError>;
    async fn get(&self, id: &str) -> Result<Option<RoutineSpec>, RoutineError>;
    async fn upsert(&self, spec: RoutineSpec) -> Result<(), RoutineError>;
    async fn remove(&self, id: &str) -> Result<(), RoutineError>;
    async fn record_run(&self, id: &str, record: RoutineRunRecord) -> Result<(), RoutineError>;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoutineError {
    #[error("routine `{0}` not found")]
    NotFound(String),
    #[error("{0}")]
    Storage(String),
}
