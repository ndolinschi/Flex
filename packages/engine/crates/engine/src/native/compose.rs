//! Helpers that assemble prompts, skills, and hooks for native composition.

use std::sync::Arc;

use agentloop_core::{Hook, PluginRegistry, PluginRole, PluginRoleTools, ToolRegistry};
use agentloop_hooks::{DiagnosticsHook, FormatOnEditHook};
use agentloop_loop::roles::{RoleSpec, RoleToolProfile};
use agentloop_mcp::McpManager;
use agentloop_prompts::{
    CommandDiscoveryConfig, CommandRegistry, SkillDiscoveryConfig, SkillRegistry,
    SystemPromptAssembler, SystemPromptConfig, Vars,
};

use crate::paths::{default_user_command_dir, default_user_memory_dir, default_user_skill_dir};
use crate::{EngineConfig, EngineResult};

/// Map a loop-independent [`PluginRole`] onto the loop's [`RoleSpec`].
pub(super) fn plugin_role_to_spec(role: PluginRole) -> RoleSpec {
    let tools = match role.tools {
        PluginRoleTools::ReadOnly => RoleToolProfile::ReadOnly,
        PluginRoleTools::Full => RoleToolProfile::Full,
        PluginRoleTools::Allow(list) => RoleToolProfile::Allow(list),
    };
    RoleSpec {
        models: role.models,
        tools,
        prompt: role.prompt,
        isolation: role.isolation,
        ..RoleSpec::new(role.name)
    }
}

pub(super) fn assemble_system_prompt(
    config: &EngineConfig,
    plugins: &PluginRegistry,
) -> EngineResult<String> {
    let cwd_display = config
        .cwd
        .as_deref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "headless".to_string());
    let mut appends = plugins.prompt_fragments();
    if let Some(memory) = agentloop_prompts::load_memory_section(&agentloop_prompts::MemoryConfig {
        dir: default_user_memory_dir(),
        budget_chars: 0,
    }) {
        appends.push(memory);
    }
    Ok(SystemPromptAssembler::new(SystemPromptConfig {
        appends,
        ..SystemPromptConfig::default()
    })
    .assemble(&Vars {
        cwd: cwd_display,
        date: config.date.clone(),
    })?)
}

pub(super) fn discover_commands(config: &EngineConfig) -> EngineResult<CommandRegistry> {
    let project_cmd_dir = config
        .cwd
        .as_ref()
        .map(|cwd| cwd.join(".agent").join("commands"));
    Ok(CommandRegistry::discover(CommandDiscoveryConfig {
        user_dir: default_user_command_dir(),
        project_dir: project_cmd_dir,
    })?)
}

pub(super) fn register_skills(config: &EngineConfig, tools: &mut ToolRegistry) -> EngineResult<()> {
    if let Some(user_skill_dir) = default_user_skill_dir() {
        match agentloop_prompts::install_bundled_skills(&user_skill_dir) {
            Ok(installed) if !installed.is_empty() => {
                tracing::debug!(?installed, "seeded bundled skills into user skill dir");
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(%err, "failed to seed bundled skills; continuing without them");
            }
        }
    }

    let project_skill_dir = config
        .cwd
        .as_ref()
        .map(|cwd| cwd.join(".agent").join("skills"));
    let skills = Arc::new(SkillRegistry::discover(SkillDiscoveryConfig {
        learned_dir: default_user_skill_dir().map(|dir| dir.join("learned")),
        user_dir: default_user_skill_dir(),
        project_dir: project_skill_dir,
    })?);
    if let Some(tool) = agentloop_tools::skill_tool(&skills.model_visible(), {
        let skills = skills.clone();
        Arc::new(move |name: &str| skills.load_body(name).ok())
    }) {
        tools.register(tool);
    }
    Ok(())
}

pub(super) fn resolve_mcp_manager(
    config: &mut EngineConfig,
) -> EngineResult<Option<Arc<McpManager>>> {
    Ok(match config.mcp_manager.take() {
        Some(manager) => Some(manager),
        None if config.mcp.servers.is_empty() => None,
        None => Some(Arc::new(McpManager::from_config_blocking_default(
            config.mcp.clone(),
        )?)),
    })
}

pub(super) fn collect_hooks(config: &EngineConfig, plugins: &PluginRegistry) -> Vec<Arc<dyn Hook>> {
    let mut hooks: Vec<Arc<dyn Hook>> = Vec::new();
    let formatter = FormatOnEditHook::new(config.formatters.clone());
    if formatter.is_active() {
        hooks.push(Arc::new(formatter));
    }
    let diagnostics = DiagnosticsHook::new(config.diagnostics.clone());
    if diagnostics.is_active() {
        hooks.push(Arc::new(diagnostics));
    }
    if config.injection_scan {
        hooks.insert(0, Arc::new(agentloop_hooks::InjectionScanHook::new()));
    }
    hooks.extend(plugins.hooks());
    hooks
}
