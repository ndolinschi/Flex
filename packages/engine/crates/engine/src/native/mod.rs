//! Native-loop composition over a prebuilt [`ProviderRegistry`].

mod compose;

use std::sync::Arc;

use agentloop_contracts::{ModelRef, PermissionMode};
use agentloop_core::{PluginRegistry, ProviderRegistry, RoutingTable, SessionStore};
use agentloop_loop::roles::RoleRegistry;
use agentloop_loop::{LoopLimits, NativeAgentBuilder};
use agentloop_session::MemoryStore;
use agentloop_tools::{
    AllowedRouting, BaseTools, GetActiveAgentsTool, GetMessagesTool, PeerMailbox, SendMessageTool,
    SetRoutingTool, SwitchModeTool,
};

use crate::paths::resolve_max_iterations;
use crate::service::EngineService;
use crate::{EngineConfig, EngineResult};

use self::compose::{
    assemble_system_prompt, collect_hooks, discover_commands, plugin_role_to_spec, register_skills,
    resolve_mcp_manager,
};

impl EngineService {
    /// Compose the native loop over a prebuilt [`ProviderRegistry`] and an
    /// optional default [`ModelRef`], plus the engine-scoped [`EngineConfig`].
    ///
    /// Provider *selection and construction* happen outside the engine (the
    /// `providers` facade); this constructor is provider-agnostic. Enabled
    /// plugins from `config.plugins` contribute tools, prompt fragments, and
    /// roles, folded in deterministically. An empty registry is valid — the
    /// service opens with no default model and the failure defers to turn time.
    pub fn native(
        providers: ProviderRegistry,
        default_model: Option<ModelRef>,
        mut config: EngineConfig,
    ) -> EngineResult<Self> {
        let plugins = PluginRegistry::from_plugins(std::mem::take(&mut config.plugins));

        let executor = config.executor.take();
        let executor_id = executor.as_ref().map(|backend| backend.id().to_owned());
        let executor = executor.unwrap_or_else(|| Arc::new(agentloop_executors::LocalExecutor));
        let BaseTools {
            registry: mut tools,
            pending_questions,
            pending_mode_switches,
            background_processes,
            demote_processes,
            ..
        } = if config.cwd.is_some() {
            agentloop_tools::base_tools(executor, config.network)
        } else {
            agentloop_tools::base_tools_read_only()
        };
        for tool in plugins.tools() {
            tools.register(tool);
        }

        let mut roles = config.roles.clone();
        roles.extend(plugins.roles().into_iter().map(plugin_role_to_spec));

        let role_registry = RoleRegistry::with_defaults(roles.clone())?;
        tools.register(agentloop_tools::subagent_tool(&role_registry.spawnable()));
        if config.enable_workflow_tool {
            tools.register(agentloop_tools::workflow_tool(&role_registry.spawnable()));
        }

        let system_prompt = assemble_system_prompt(&config, &plugins)?;
        let commands = discover_commands(&config)?;
        register_skills(&config, &mut tools)?;
        let mcp_manager = resolve_mcp_manager(&mut config)?;

        let store: Arc<dyn SessionStore> = config
            .session_store
            .take()
            .unwrap_or_else(|| Arc::new(MemoryStore::new()));

        // Peer-messaging tools: share one mailbox across all three tools.
        if config.enable_peer_messaging {
            let mailbox = Arc::new(PeerMailbox::new());
            tools.register(Arc::new(GetActiveAgentsTool::new(store.clone())));
            tools.register(Arc::new(SendMessageTool::new(mailbox.clone())));
            tools.register(Arc::new(GetMessagesTool::new(mailbox)));
        }

        // SwitchMode: wire the same pending map the builder will expose via
        // `respond_mode_switch`, so the tool and the reply path share state.
        let active_mode_switches = if config.enable_switch_mode {
            tools.register(Arc::new(SwitchModeTool::new(pending_mode_switches.clone())));
            Some(pending_mode_switches.clone())
        } else {
            None
        };

        // SetRouting: create a shared RoutingTable and hand it to both the
        // tool (so it can write overrides) and the builder (so the loop can
        // read them next iteration).
        let routing = Arc::new(RoutingTable::new());
        if config.enable_set_routing {
            let allowed = Arc::new(AllowedRouting {
                cost_mode: config.cost_mode.clone(),
                low: config.cost_models_low.clone(),
                medium: config.cost_models_medium.clone(),
                high: config.cost_models_high.clone(),
            });
            tools.register(Arc::new(SetRoutingTool::new(routing.clone(), allowed)));
        }

        let limits = LoopLimits {
            max_iterations: resolve_max_iterations(config.max_iterations),
            retry: config.retry_policy.clone().unwrap_or_default(),
            auto_compact: config.auto_compact,
            auto_compact_threshold_percent: config.auto_compact_threshold_percent as u64,
            compaction_mode: config.compaction_mode,
            ..LoopLimits::default()
        };
        let mut builder = NativeAgentBuilder::new(store.clone())
            .providers(providers.clone())
            .tools(tools)
            .questions(pending_questions)
            .mode_switches(pending_mode_switches)
            .routing(routing)
            .system_prompt(system_prompt)
            .commands(commands.infos())
            .roles(roles)
            .limits(limits);
        if let Some(model) = default_model {
            builder = builder.default_model(model);
        }
        if !config.default_fallback_models.is_empty() {
            builder = builder.default_fallback_models(config.default_fallback_models.clone());
        }
        let force_ask_tools = plugins.force_ask_tools();
        if !force_ask_tools.is_empty() {
            builder = builder.policy(
                agentloop_loop::PermissionPolicy::new(PermissionMode::Default)
                    .with_force_ask(force_ask_tools),
            );
        }
        if let Some(id) = executor_id {
            builder = builder.executor_id(id);
        }
        if let Some(manager) = mcp_manager {
            builder = builder.mcp(manager);
        }
        let hooks = collect_hooks(&config, &plugins);
        if !hooks.is_empty() {
            builder = builder.hooks(hooks);
        }
        if let Some(workspace) = &config.workspace {
            builder = builder.workspace(workspace.clone());
        }
        let agent = builder.build();
        let mut service = Self::with_commands(agent, store, commands);
        service.providers = providers;
        service.workspace = config.workspace;
        service.isolation_default = config.isolation_default;
        service.verify_command = config.verify_command;
        service.verbosity = config.verbosity;
        service.background_processes = background_processes;
        service.demote_processes = demote_processes;
        service.pending_mode_switches = active_mode_switches;
        Ok(service)
    }
}
