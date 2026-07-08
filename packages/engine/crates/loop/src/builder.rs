//! Builder and limits for [`NativeAgent`](crate::NativeAgent).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentloop_contracts::{Answer, CommandInfo, ModelRef, PermissionMode, QuestionId};
use agentloop_core::{Hook, PendingMap, ProviderRegistry, SessionStore, ToolRegistry, Workspaces};
use agentloop_mcp::McpManager;

use crate::agent::NativeAgent;
use crate::deps::TurnDeps;
use crate::permission::PermissionPolicy;

/// Bounds on a turn.
#[derive(Debug, Clone)]
pub struct LoopLimits {
    /// Model-call iterations per turn.
    pub max_iterations: u32,
    /// Concurrent read-only tool executions.
    pub tool_concurrency: usize,
    /// Hard per-tool-call timeout.
    pub tool_timeout: Duration,
    /// Optional global cross-session cap on concurrently running tools.
    /// `None` (default) keeps today's per-session bound only.
    pub tool_pool_size: Option<usize>,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self {
            max_iterations: 500,
            tool_concurrency: 4,
            tool_timeout: Duration::from_secs(600),
            tool_pool_size: None,
        }
    }
}

/// Builder for [`NativeAgent`].
pub struct NativeAgentBuilder {
    store: Arc<dyn SessionStore>,
    providers: ProviderRegistry,
    tools: ToolRegistry,
    hooks: Vec<Arc<dyn Hook>>,
    policy: PermissionPolicy,
    limits: LoopLimits,
    system_prompt: String,
    default_model: Option<ModelRef>,
    command_infos: Vec<CommandInfo>,
    roles: Vec<crate::roles::RoleSpec>,
    pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    mcp: Option<std::sync::Arc<McpManager>>,
    workspace: Option<Arc<dyn Workspaces>>,
}

impl NativeAgentBuilder {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            store,
            providers: ProviderRegistry::new(),
            tools: ToolRegistry::new(),
            hooks: Vec::new(),
            policy: PermissionPolicy::new(PermissionMode::Default),
            limits: LoopLimits::default(),
            system_prompt: String::new(),
            default_model: None,
            command_infos: Vec::new(),
            roles: Vec::new(),
            pending_questions: Arc::new(PendingMap::new()),
            mcp: None,
            workspace: None,
        }
    }

    pub fn providers(mut self, providers: ProviderRegistry) -> Self {
        self.providers = providers;
        self
    }

    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    pub fn hooks(mut self, hooks: Vec<Arc<dyn Hook>>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn policy(mut self, policy: PermissionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn limits(mut self, limits: LoopLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn default_model(mut self, model: ModelRef) -> Self {
        self.default_model = Some(model);
        self
    }

    pub fn commands(mut self, commands: Vec<CommandInfo>) -> Self {
        self.command_infos = commands;
        self
    }

    /// Share the pending-question map with the `AskUserQuestion` tool: build
    /// the map first, hand a clone to the tool, and a clone here.
    pub fn questions(mut self, pending: Arc<PendingMap<QuestionId, Vec<Answer>>>) -> Self {
        self.pending_questions = pending;
        self
    }

    /// Role definitions for multi-agent orchestration (built-ins are always
    /// present; these override or extend them).
    pub fn roles(mut self, roles: Vec<crate::roles::RoleSpec>) -> Self {
        self.roles = roles;
        self
    }

    /// Register bridged MCP tools from a loaded manager after base tools.
    pub fn mcp(mut self, manager: std::sync::Arc<McpManager>) -> Self {
        self.mcp = Some(manager);
        self
    }

    /// Inject an isolation backend. When set, a root session whose effective
    /// policy asks for isolation runs in an isolated workspace it provisions.
    pub fn workspace(mut self, workspace: Arc<dyn Workspaces>) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn build(self) -> Arc<NativeAgent> {
        let mut tools = self.tools;
        if let Some(manager) = &self.mcp {
            manager.register_tools(&mut tools);
        }
        let roles = Arc::new(
            crate::roles::RoleRegistry::with_defaults(self.roles)
                .unwrap_or_else(|_| crate::roles::RoleRegistry::default()),
        );
        Arc::new_cyclic(|weak| NativeAgent {
            deps: Arc::new(TurnDeps {
                pool: Arc::new(crate::pool::ToolWorkerPool::new(self.limits.tool_pool_size)),
                roles,
                agent: weak.clone(),
                agent_id: "native".to_owned(),
                providers: self.providers,
                tools,
                store: self.store,
                hooks: self.hooks,
                policy: self.policy,
                limits: self.limits,
                system_prompt: self.system_prompt,
                default_model: self.default_model,
                workspace: self.workspace,
                pending_permissions: Arc::new(PendingMap::new()),
                pending_questions: self.pending_questions,
            }),
            command_infos: self.command_infos,
            sessions: Mutex::new(HashMap::new()),
        })
    }
}
