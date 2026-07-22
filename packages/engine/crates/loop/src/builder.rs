//! Builder and limits for [`NativeAgent`](crate::NativeAgent).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentloop_contracts::{
    Answer, CommandInfo, CompactionMode, ModeSwitchId, ModelRef, PermissionMode, QuestionId,
};
use agentloop_core::{
    Hook, PendingMap, ProviderRegistry, RoutingTable, SessionStore, ToolRegistry, Workspaces,
};
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
    /// Escalating backoff schedule for provider/network failures that
    /// survive a dropped connection instead of failing the turn or burning a
    /// fallback model. See [`RetryPolicy`].
    pub retry: RetryPolicy,
    /// Enable proactive auto-compaction when estimated context usage nears
    /// `auto_compact_threshold_percent`. Default `true`. When `false`,
    /// compaction is still triggered reactively on a hard context-overflow
    /// error from the provider.
    pub auto_compact: bool,
    /// Percentage (1–100) of the resolved context window that triggers
    /// proactive auto-compaction. Default 85.
    pub auto_compact_threshold_percent: u64,
    /// How the conversation is condensed during compaction.
    pub compaction_mode: CompactionMode,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self {
            max_iterations: 500,
            tool_concurrency: 4,
            tool_timeout: Duration::from_secs(600),
            tool_pool_size: None,
            retry: RetryPolicy::default(),
            auto_compact: true,
            auto_compact_threshold_percent: 85,
            compaction_mode: CompactionMode::Standard,
        }
    }
}

/// Escalating same-model retry schedule for RETRYABLE provider/network
/// failures (timeouts, dropped connections, mid-stream cuts, 5xx, rate
/// limits) that occur while calling the model.
///
/// This is deliberately separate from the mid-stream `stream_retries` bound
/// in [`crate::turn::iteration`]: that one is a handful of fast, low-backoff
/// retries for a corrupted frame on an otherwise healthy connection. This
/// policy is the outer, patient layer — it survives a connection actually
/// dropping (Wi-Fi hiccup, VPN reset, provider outage) by holding the turn
/// open across the fallback-chain boundary, sleeping the scheduled delay,
/// and retrying the *same* model before the existing fallback/model-exhausted
/// semantics ever see the failure.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Delay before each successive retry attempt, in order. The default is
    /// approximately 10 attempts total (initial call + `schedule.len()`
    /// retries): three 30s spacings, two 60s spacings, then four 300s (5m)
    /// spacings for a patient tail — enough to ride out a laptop sleep/wake
    /// or a several-minute network blip without failing the turn.
    pub schedule: Vec<Duration>,
}

impl RetryPolicy {
    /// The default escalating schedule: `[30s, 30s, 30s, 60s, 60s, 300s,
    /// 300s, 300s, 300s]` — 9 retries plus the initial attempt, ~10 total.
    pub fn default_schedule() -> Vec<Duration> {
        vec![
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(60),
            Duration::from_secs(300),
            Duration::from_secs(300),
            Duration::from_secs(300),
            Duration::from_secs(300),
        ]
    }

    /// Total attempts the schedule allows, counting the initial call.
    pub fn max_attempts(&self) -> u32 {
        self.schedule.len() as u32 + 1
    }

    /// The delay for the given 1-indexed retry attempt (`1` = first retry,
    /// after the initial call fails), or `None` once the schedule is
    /// exhausted.
    pub fn delay_for(&self, attempt: u32) -> Option<Duration> {
        let index = attempt.checked_sub(1)? as usize;
        self.schedule.get(index).copied()
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            schedule: Self::default_schedule(),
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
    default_fallback_models: Vec<ModelRef>,
    command_infos: Vec<CommandInfo>,
    roles: Vec<crate::roles::RoleSpec>,
    pending_questions: Arc<PendingMap<QuestionId, Vec<Answer>>>,
    pending_mode_switches: Arc<PendingMap<ModeSwitchId, bool>>,
    mcp: Option<std::sync::Arc<McpManager>>,
    workspace: Option<Arc<dyn Workspaces>>,
    executor_id: Option<String>,
    routing: Arc<RoutingTable>,
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
            default_fallback_models: Vec::new(),
            command_infos: Vec::new(),
            roles: Vec::new(),
            pending_questions: Arc::new(PendingMap::new()),
            pending_mode_switches: Arc::new(PendingMap::new()),
            mcp: None,
            workspace: None,
            executor_id: None,
            routing: Arc::new(RoutingTable::new()),
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

    /// Engine-wide default fallback chain, used by a session created with an
    /// empty `NewSessionParams.fallback_models`.
    pub fn default_fallback_models(mut self, models: Vec<ModelRef>) -> Self {
        self.default_fallback_models = models;
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

    /// Share the pending mode-switch map with the `SwitchMode` tool: build
    /// the map first, hand a clone to the tool, and a clone here.
    pub fn mode_switches(mut self, pending: Arc<PendingMap<ModeSwitchId, bool>>) -> Self {
        self.pending_mode_switches = pending;
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

    /// Record the id of the command-execution backend shell tools run
    /// through, stamped onto each session's metadata. `None` (default) =
    /// host execution.
    pub fn executor_id(mut self, id: impl Into<String>) -> Self {
        self.executor_id = Some(id.into());
        self
    }

    /// Share a [`RoutingTable`] with the `SetRouting` tool: build the table
    /// first, hand a clone to the tool, and a clone here so the loop sees
    /// mid-turn overrides written by the tool.
    pub fn routing(mut self, routing: Arc<RoutingTable>) -> Self {
        self.routing = routing;
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
                default_fallback_models: self.default_fallback_models,
                workspace: self.workspace,
                executor_id: self.executor_id,
                pending_permissions: Arc::new(PendingMap::new()),
                pending_questions: self.pending_questions,
                pending_mode_switches: self.pending_mode_switches,
                routing: self.routing,
            }),
            command_infos: self.command_infos,
            sessions: Mutex::new(HashMap::new()),
        })
    }
}

#[cfg(test)]
mod loop_limits_tests {
    use super::*;

    #[test]
    fn default_has_auto_compact_enabled_at_85_percent_standard_mode() {
        let limits = LoopLimits::default();
        assert!(limits.auto_compact);
        assert_eq!(limits.auto_compact_threshold_percent, 85);
        assert_eq!(limits.compaction_mode, CompactionMode::Standard);
    }

    #[test]
    fn auto_compact_false_is_a_valid_config() {
        let limits = LoopLimits {
            auto_compact: false,
            ..LoopLimits::default()
        };
        assert!(!limits.auto_compact);
        // Threshold and mode fields remain accessible even when compaction is off.
        assert_eq!(limits.auto_compact_threshold_percent, 85);
    }

    #[test]
    fn turn_pair_mode_can_be_set() {
        let limits = LoopLimits {
            compaction_mode: CompactionMode::TurnPair,
            ..LoopLimits::default()
        };
        assert_eq!(limits.compaction_mode, CompactionMode::TurnPair);
    }
}

#[cfg(test)]
mod retry_policy_tests {
    use super::*;

    #[test]
    fn default_schedule_has_ten_total_attempts() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.schedule.len(), 9);
        assert_eq!(policy.max_attempts(), 10);
    }

    #[test]
    fn default_schedule_escalates_as_documented() {
        let policy = RetryPolicy::default();
        let expected = [30, 30, 30, 60, 60, 300, 300, 300, 300].map(Duration::from_secs);
        for (attempt, expected_delay) in (1u32..).zip(expected) {
            assert_eq!(
                policy.delay_for(attempt),
                Some(expected_delay),
                "attempt {attempt} delay mismatch"
            );
        }
    }

    #[test]
    fn delay_for_is_none_once_schedule_is_exhausted() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay_for(10), None);
        assert_eq!(policy.delay_for(11), None);
    }

    #[test]
    fn delay_for_rejects_attempt_zero() {
        // Attempt numbering is 1-indexed (attempt 1 = first retry); 0 has no
        // meaning and must not panic via underflow.
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay_for(0), None);
    }

    #[test]
    fn empty_schedule_is_immediately_exhausted() {
        let policy = RetryPolicy {
            schedule: Vec::new(),
        };
        assert_eq!(policy.max_attempts(), 1);
        assert_eq!(policy.delay_for(1), None);
    }

    #[test]
    fn custom_schedule_overrides_default() {
        let policy = RetryPolicy {
            schedule: vec![Duration::from_millis(10), Duration::from_millis(20)],
        };
        assert_eq!(policy.max_attempts(), 3);
        assert_eq!(policy.delay_for(1), Some(Duration::from_millis(10)));
        assert_eq!(policy.delay_for(2), Some(Duration::from_millis(20)));
        assert_eq!(policy.delay_for(3), None);
    }
}
