use std::sync::{Arc, Weak};

use agentloop_contracts::{
    Answer, ModeSwitchId, ModelRef, PermissionDecision, PermissionRequestId, QuestionId,
};
use agentloop_core::{
    Hook, PendingMap, ProviderRegistry, RoutingTable, SessionStore, ToolRegistry, Workspaces,
};

use crate::builder::LoopLimits;
use crate::permission::PermissionPolicy;

pub(crate) struct TurnDeps {
    pub(crate) pool: Arc<crate::pool::ToolWorkerPool>,
    pub(crate) roles: Arc<crate::roles::RoleRegistry>,
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
    pub(crate) default_fallback_models: Vec<ModelRef>,
    pub(crate) workspace: Option<Arc<dyn Workspaces>>,
    pub(crate) executor_id: Option<String>,
    pub(crate) pending_permissions: Arc<PendingMap<PermissionRequestId, PermissionDecision>>,
    pub(crate) pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    pub(crate) pending_mode_switches: Arc<PendingMap<ModeSwitchId, bool>>,
    pub(crate) routing: Arc<RoutingTable>,
}
