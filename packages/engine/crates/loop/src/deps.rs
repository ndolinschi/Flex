//! `TurnDeps`: everything a turn needs, `Arc`-shared so turn execution can
//! move onto child tasks without borrowing [`crate::NativeAgent`].

use std::sync::{Arc, Weak};

use agentloop_contracts::{Answer, ModelRef, PermissionDecision, PermissionRequestId, QuestionId};
use agentloop_core::{Hook, PendingMap, ProviderRegistry, SessionStore, ToolRegistry, Workspaces};

use crate::builder::LoopLimits;
use crate::permission::PermissionPolicy;

/// Immutable (or internally synchronized) dependencies shared by every turn
/// of a [`crate::NativeAgent`].
pub(crate) struct TurnDeps {
    /// Spawned tool execution: bounded, panic-isolated (see [`crate::pool`]).
    pub(crate) pool: Arc<crate::pool::ToolWorkerPool>,
    /// Role definitions for subagent spawning and failover chains.
    pub(crate) roles: Arc<crate::roles::RoleRegistry>,
    /// Back-reference to the owning agent, for spawning child sessions from
    /// the Task tool. A `Weak` avoids the `Arc` cycle (agent → deps → agent).
    pub(crate) agent: Weak<crate::agent::NativeAgent>,
    pub(crate) agent_id: String,
    pub(crate) providers: ProviderRegistry,
    pub(crate) tools: ToolRegistry,
    pub(crate) store: Arc<dyn SessionStore>,
    pub(crate) hooks: Vec<Arc<dyn Hook>>,
    pub(crate) policy: PermissionPolicy,
    pub(crate) limits: LoopLimits,
    pub(crate) system_prompt: String,
    pub(crate) default_model: Option<ModelRef>,
    /// Optional isolation backend. When set, root sessions whose effective
    /// policy asks for isolation are provisioned an isolated workspace.
    pub(crate) workspace: Option<Arc<dyn Workspaces>>,
    pub(crate) pending_permissions: Arc<PendingMap<PermissionRequestId, PermissionDecision>>,
    pub(crate) pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
}
