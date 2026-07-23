use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{SessionId, ToolCallId, ToolOutput, TurnId};

use crate::event_sink::EventSink;
use crate::provider::ToolSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCategory {
    Fs,
    Shell,
    Web,
    Agent,
    Mcp,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionHint {
    Never,
    IfMutating,
    Always,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub read_only: bool,
    pub category: ToolCategory,
    pub needs_permission: PermissionHint,
}

impl ToolDescriptor {
    pub fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
        }
    }
}

pub const SUBAGENT_TOOL_NAME: &str = "Agent";

pub const VERIFIER_TOOL_NAME: &str = "Verify";

pub const SUBMIT_VERDICT_TOOL_NAME: &str = "SubmitVerdict";

pub const WORKFLOW_TOOL_NAME: &str = "RunWorkflow";

pub const EXIT_PLAN_MODE_TOOL_NAME: &str = "ExitPlanMode";

pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub call_id: ToolCallId,
    pub cwd: PathBuf,
    pub cancel: CancellationToken,
    pub events: EventSink,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Execution(String),
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError>;
}

pub fn typed_tool<I, F, Fut>(
    name: &str,
    description: &str,
    read_only: bool,
    category: ToolCategory,
    needs_permission: PermissionHint,
    f: F,
) -> Arc<dyn Tool>
where
    I: JsonSchema + DeserializeOwned + Send + 'static,
    F: Fn(ToolContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
{
    Arc::new(TypedTool {
        descriptor: ToolDescriptor {
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema: serde_json::to_value(schemars::schema_for!(I))
                .unwrap_or_else(|_| serde_json::json!({"type": "object"})),
            read_only,
            category,
            needs_permission,
        },
        f,
        _marker: std::marker::PhantomData,
    })
}

struct TypedTool<I, F> {
    descriptor: ToolDescriptor,
    f: F,
    _marker: std::marker::PhantomData<fn(I)>,
}

#[async_trait]
impl<I, F, Fut> Tool for TypedTool<I, F>
where
    I: JsonSchema + DeserializeOwned + Send + 'static,
    F: Fn(ToolContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
{
    fn descriptor(&self) -> ToolDescriptor {
        self.descriptor.clone()
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: I = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `{}` does not match its schema: {err}. \
                 Check required fields and types, then retry.",
                self.descriptor.name
            ))
        })?;
        (self.f)(ctx, parsed).await
    }
}
