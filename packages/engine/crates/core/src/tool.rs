//! The `Tool` trait and helpers.
//!
//! Tool design is prompt design: descriptions are written for the model
//! (examples, edge cases), parameters are poka-yoke (hard to misuse), and
//! error messages teach the correct next step. Outputs are token-efficient —
//! truncate with explicit markers, never silently.

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

/// Permission grouping for a tool.
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

/// When a tool needs a permission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionHint {
    /// Never asks (safe read-only operations).
    Never,
    /// Asks only when the call would mutate state.
    IfMutating,
    /// Always asks.
    Always,
}

/// Static description of one tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolDescriptor {
    /// Snake_case, verb-first where sensible. This is the name the model sees
    /// and the name permission rules match against.
    pub name: String,
    /// Written for the model: what it does, when to use it, edge cases.
    pub description: String,
    /// Full JSON Schema of the input object.
    pub input_schema: serde_json::Value,
    /// Read-only calls may run concurrently; mutating calls run sequentially.
    pub read_only: bool,
    pub category: ToolCategory,
    pub needs_permission: PermissionHint,
}

impl ToolDescriptor {
    /// The agent-facing spec sent to providers.
    pub fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
        }
    }
}

/// Everything a tool execution may need from its surroundings.
/// The reserved tool name the engine loop intercepts to spawn subagents.
/// The `tools` crate ships only the descriptor; execution is loop-owned.
pub const SUBAGENT_TOOL_NAME: &str = "Agent";

/// The reserved tool name the engine loop intercepts to spawn an independent
/// verifier (a `verifier`-role subagent seeded with only a rubric and
/// artifact paths — never the maker's reasoning). The `tools` crate ships
/// only the descriptor; execution is loop-owned, same as [`SUBAGENT_TOOL_NAME`].
pub const VERIFIER_TOOL_NAME: &str = "Verify";

/// The tool a `verifier`-role subagent calls to report its outcome. A normal,
/// role-restricted tool (not loop-intercepted): its only effect is ending the
/// verifier's turn with a structured `agentloop_contracts::VerificationVerdict`
/// carried on the result.
pub const SUBMIT_VERDICT_TOOL_NAME: &str = "SubmitVerdict";

/// The reserved tool name the engine loop intercepts to run a declarative
/// pipeline of subagent steps (sequential `Task` steps and barrier
/// `Parallel` steps) — the model describes the plan as data rather than
/// calling `Agent` once per turn. The `tools` crate ships only the
/// descriptor; execution is loop-owned, same as [`SUBAGENT_TOOL_NAME`]. Not
/// registered unless `EngineConfig.enable_workflow_tool` is set.
pub const WORKFLOW_TOOL_NAME: &str = "RunWorkflow";

/// The tool name the engine loop watches for in Plan permission mode: a
/// successful call hands a finished plan to the user and ends the turn
/// immediately (see `agentloop_loop::turn::iteration`), rather than letting
/// the model keep iterating after the plan is already ready for approval.
/// The `tools` crate ships only the descriptor and its (read-only, no-op)
/// execution; the turn-ending behavior is loop-owned, same as
/// [`SUBAGENT_TOOL_NAME`].
pub const EXIT_PLAN_MODE_TOOL_NAME: &str = "ExitPlanMode";

pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub call_id: ToolCallId,
    /// The session's working directory. Path-taking tools resolve and
    /// sandbox against this.
    pub cwd: PathBuf,
    /// Cancelled when the turn is interrupted. Tools must be cancel-safe.
    pub cancel: CancellationToken,
    /// Emit `ToolProgress` (and other) events into the session stream.
    pub events: EventSink,
}

/// A tool-level failure. `InvalidInput` and `Execution` become `is_error`
/// results fed back to the model; `Timeout` and `Cancelled` surface as the
/// corresponding `ToolCall` statuses.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolError {
    /// The input didn't match the schema. The message must teach the model
    /// how to fix the call.
    #[error("{0}")]
    InvalidInput(String),
    /// The tool ran and failed. The message must be actionable.
    #[error("{0}")]
    Execution(String),
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("cancelled")]
    Cancelled,
}

/// One executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError>;
}

/// Build a [`Tool`] from a typed async closure. The input schema is derived
/// with `schemars`; parse failures become teaching `InvalidInput` errors —
/// authors never touch raw JSON.
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
