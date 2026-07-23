use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{ModelRef, SessionId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskProfile {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskSpawnRequest {
    pub profile_id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskHandle {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
}

#[async_trait]
pub trait TaskSpawner: Send + Sync {
    async fn spawn_task(
        &self,
        request: TaskSpawnRequest,
        cancel: CancellationToken,
    ) -> Result<TaskHandle, TaskSpawnError>;
}

#[derive(Debug, Clone, Default)]
pub struct DisabledTaskSpawner;

#[async_trait]
impl TaskSpawner for DisabledTaskSpawner {
    async fn spawn_task(
        &self,
        request: TaskSpawnRequest,
        _cancel: CancellationToken,
    ) -> Result<TaskHandle, TaskSpawnError> {
        Err(TaskSpawnError::NotImplemented {
            profile_id: request.profile_id,
            hint: "subagent spawning is scaffolded but no task runtime is registered".to_owned(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TaskSpawnError {
    #[error("task profile `{profile_id}` is not available: {hint}")]
    ProfileUnavailable { profile_id: String, hint: String },
    #[error("task profile `{profile_id}` cannot spawn yet: {hint}")]
    NotImplemented { profile_id: String, hint: String },
    #[error("task spawn was cancelled")]
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_spawner_returns_actionable_error() {
        let spawner = DisabledTaskSpawner;
        let err = spawner
            .spawn_task(
                TaskSpawnRequest {
                    profile_id: "reviewer".to_owned(),
                    prompt: "review this change".to_owned(),
                    cwd: None,
                    model: None,
                    metadata: BTreeMap::new(),
                },
                CancellationToken::new(),
            )
            .await;

        assert!(matches!(
            err,
            Err(TaskSpawnError::NotImplemented { profile_id, hint })
                if profile_id == "reviewer" && hint.contains("no task runtime")
        ));
    }
}
