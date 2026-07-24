
use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{CompactionMode, IsolationPolicy};
use agentloop_engine::{RoleSpec, RoleToolProfile};
use agentloop_sdk::mcp_store::default_mcp_dir;
use agentloop_sdk::{
    AgentBuilder, ArtifactsPlugin, EngineService, IndexPlugin, LearningPlugin, McpBridgeConfig,
    McpServerConfig,
};
use agentloop_session::JsonlStore;
use agentloop_workspace::GitWorktrees;
use tauri::AppHandle;

use crate::config::{sessions_dir, worktrees_dir, ProviderConfig};
use crate::error::{DesktopError, DesktopResult};
use crate::plugins::{BrowserPlugin, ComputerPlugin};

fn load_enabled_mcp_servers() -> McpBridgeConfig {
    let Some(dir) = default_mcp_dir() else {
        return McpBridgeConfig::default();
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return McpBridgeConfig::default();
        }
        Err(err) => {
            tracing::warn!(error = %err, "could not read MCP server directory");
            return McpBridgeConfig::default();
        }
    };

    let mut servers = Vec::new();
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(err) => {
                tracing::warn!(error = %err, "could not read MCP server directory entry");
                continue;
            }
        };
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "could not read MCP server config");
                continue;
            }
        };
        match toml::from_str::<McpServerConfig>(&content) {
            Ok(server) if server.enabled => {
                servers.push(resolve_mcp_server_secrets(server));
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "could not parse MCP server config");
            }
        }
    }

    let config = McpBridgeConfig { servers };
    if let Err(err) = config.validate() {
        tracing::warn!(error = %err, "MCP server configuration is invalid; no MCP servers loaded");
        return McpBridgeConfig::default();
    }
    config
}

pub(crate) fn resolve_mcp_server_secrets(mut config: McpServerConfig) -> McpServerConfig {
    let (secret_env, args_suffix) = match crate::config::load_mcp_server_secrets(&config.name) {
        Ok(pair) => pair,
        Err(err) => {
            tracing::warn!(
                server = %config.name,
                error = %err,
                "failed to load MCP secrets; starting without them"
            );
            return config;
        }
    };
    if secret_env.is_empty() && args_suffix.is_empty() {
        return config;
    }
    match &mut config.transport {
        agentloop_sdk::McpServerTransport::Stdio(stdio) => {
            for (k, v) in secret_env {
                stdio.env.insert(k, v);
            }
            stdio.args.extend(args_suffix);
        }
        agentloop_sdk::McpServerTransport::StreamableHttp(http)
        | agentloop_sdk::McpServerTransport::Sse(http) => {
            for (k, v) in secret_env {
                http.headers.insert(k, v);
            }
        }
        _ => {}
    }
    config
}

fn base_url_env(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" | "deepseek" => Some("OPENAI_BASE_URL"),
        "anthropic" => Some("ANTHROPIC_BASE_URL"),
        "gemini" => Some("GEMINI_BASE_URL"),
        "ollama" => Some("OLLAMA_HOST"),
        "openrouter" => Some("OPENROUTER_BASE_URL"),
        "groq" => Some("GROQ_BASE_URL"),
        "mistral" => Some("MISTRAL_BASE_URL"),
        "xai" => Some("XAI_BASE_URL"),
        _ => None,
    }
}

fn with_base_url_env<T>(provider: &str, base_url: Option<&str>, f: impl FnOnce() -> T) -> T {
    let env_key = base_url_env(provider);
    let previous = env_key.and_then(|k| std::env::var(k).ok());
    if let (Some(key), Some(url)) = (env_key, base_url.filter(|s| !s.is_empty())) {
        unsafe { std::env::set_var(key, url) };
    }
    let out = f();
    if let Some(key) = env_key {
        match previous {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
    out
}

fn provider_of_model(model: &str) -> Option<&str> {
    model.split_once('/').map(|(p, _)| p)
}

fn with_bedrock_env<T>(key: Option<&str>, region: Option<&str>, f: impl FnOnce() -> T) -> T {
    let key = key.filter(|k| !k.is_empty());
    let Some(key) = key else {
        return f();
    };
    let prev_token = std::env::var("AWS_BEARER_TOKEN_BEDROCK").ok();
    let prev_region = std::env::var("BEDROCK_REGION").ok();
    unsafe { std::env::set_var("AWS_BEARER_TOKEN_BEDROCK", key) };
    if let Some(region) = region.filter(|r| !r.is_empty()) {
        unsafe { std::env::set_var("BEDROCK_REGION", region) };
    }
    let out = f();
    match prev_token {
        Some(v) => unsafe { std::env::set_var("AWS_BEARER_TOKEN_BEDROCK", v) },
        None => unsafe { std::env::remove_var("AWS_BEARER_TOKEN_BEDROCK") },
    }
    match prev_region {
        Some(v) => unsafe { std::env::set_var("BEDROCK_REGION", v) },
        None => unsafe { std::env::remove_var("BEDROCK_REGION") },
    }
    out
}

struct ActiveConnection<'a> {
    provider: &'a str,
    base_url: Option<&'a str>,
    region: Option<&'a str>,
    default_model: Option<&'a str>,
    fallback_models: Vec<String>,
    api_key: Option<&'a str>,
}

fn resolve_active_connection(cfg: &ProviderConfig) -> DesktopResult<ActiveConnection<'_>> {
    if let Some(profile) = cfg.active_profile() {
        return Ok(ActiveConnection {
            provider: profile.provider.as_str(),
            base_url: profile.base_url.as_deref(),
            region: profile.region.as_deref(),
            default_model: profile.default_model.as_deref(),
            fallback_models: profile
                .fallback_models
                .as_deref()
                .map(|s| {
                    s.split(',')
                        .map(|m| m.trim().to_owned())
                        .filter(|m| !m.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            api_key: cfg.active_profile_key().map(String::as_str),
        });
    }
    let preferred = cfg
        .prefs
        .preferred_provider
        .as_deref()
        .ok_or(DesktopError::NotConfigured)?;
    Ok(ActiveConnection {
        provider: preferred,
        base_url: cfg.prefs.base_url.as_deref(),
        region: cfg.prefs.region.as_deref(),
        default_model: cfg.prefs.default_model.as_deref(),
        fallback_models: cfg.prefs.fallback_models.clone(),
        api_key: cfg.keys.get(preferred).map(String::as_str),
    })
}

/// Build the engine. When `include_mcp` is false, MCP servers are skipped so
/// cold launch can paint the UI before slow stdio/HTTP tool discovery.
pub fn build_service(
    cfg: &ProviderConfig,
    store: Arc<JsonlStore>,
    app: AppHandle,
) -> DesktopResult<EngineService> {
    build_service_opts(cfg, store, app, true)
}

/// Fast path used at app launch: same as [`build_service`] without blocking MCP connects.
pub fn build_service_fast(
    cfg: &ProviderConfig,
    store: Arc<JsonlStore>,
    app: AppHandle,
) -> DesktopResult<EngineService> {
    build_service_opts(cfg, store, app, false)
}

/// True when at least one enabled MCP server is configured (disk only — no connect).
pub fn has_enabled_mcp_servers() -> bool {
    !load_enabled_mcp_servers().servers.is_empty()
}

fn build_service_opts(
    cfg: &ProviderConfig,
    store: Arc<JsonlStore>,
    app: AppHandle,
    include_mcp: bool,
) -> DesktopResult<EngineService> {
    let conn = resolve_active_connection(cfg)?;
    let preferred = conn.provider;

    let cwd = cfg
        .prefs
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let base_url = conn.base_url;
    let fallbacks = conn.fallback_models.clone();
    let inline_provider = cfg
        .prefs
        .inline_completion
        .provider_id
        .as_deref()
        .filter(|p| !p.is_empty());
    let needs_all = fallbacks.iter().any(|m| {
        provider_of_model(m)
            .map(|p| p != preferred)
            .unwrap_or(false)
    }) || inline_provider.is_some_and(|p| p != preferred);
    let bedrock_key = (preferred == "bedrock").then_some(conn.api_key).flatten();
    let bedrock_region = conn.region;

    with_bedrock_env(bedrock_key, bedrock_region, || {
        with_base_url_env(preferred, base_url, || {
            let mut builder = AgentBuilder::new().cwd(cwd.clone()).provider(preferred);

            if let Some(model) = conn.default_model {
                builder = builder.model(model.to_owned());
            }

            if let Some(key) = conn.api_key {
                builder = builder.provider_key(preferred.to_owned(), key.to_owned());
            }
            if preferred == "bedrock" {
                if let Some(region) = conn.region.filter(|r| !r.is_empty()) {
                    builder = builder.provider_region("bedrock", region.to_owned());
                }
            }

            if !fallbacks.is_empty() {
                builder = builder.fallback_models(fallbacks);
            }
            if needs_all {
                builder = builder.all_providers(true);
            }

            if cfg.prefs.plugins.search {
                builder = builder.enable_plugin("search");
            }
            if cfg.prefs.plugins.index {
                builder = builder.plugin(
                    IndexPlugin::new()
                        .with_auto_context(cfg.prefs.plugins.auto_context)
                        .with_auto_update(cfg.prefs.plugins.auto_update_index),
                );
            }
            if cfg.prefs.plugins.learning {
                if let Some(plugin) = LearningPlugin::with_default_dir() {
                    let plugin = plugin
                        .require_human_approval(cfg.prefs.plugins.learning_require_human_approval)
                        .require_verified_memory(
                            cfg.prefs.plugins.learning_require_verified_memory,
                        );
                    builder = builder.plugin(plugin);
                } else {
                    tracing::warn!("learning plugin enabled but home dir unresolved; skipping");
                }
            }
            if cfg.prefs.plugins.verifier || cfg.prefs.plugins.council {
                builder = builder.enable_plugin("verifier");
            }
            if cfg.prefs.plugins.messaging {
                builder = builder.enable_plugin("messaging");
            }
            if cfg.prefs.plugins.browser {
                builder = builder.plugin(BrowserPlugin::new(app.clone()));
            }
            if cfg.prefs.plugins.computer {
                builder = builder.plugin(ComputerPlugin::new(app.clone()));
            }
            if cfg.prefs.plugins.artifacts {
                builder = builder.plugin(ArtifactsPlugin::default());
            }

            {
                let config = builder.config_mut();
                config.session_store = Some(store.clone());
                config.verbosity = agentloop_sdk::OutputVerbosity::High;
                config.auto_compact = cfg.prefs.plugins.auto_compact;
                config.auto_compact_threshold_percent =
                    cfg.prefs.plugins.auto_compact_threshold_percent;
                config.compaction_mode = match cfg.prefs.plugins.compaction_mode.as_str() {
                    "turn_pair" => CompactionMode::TurnPair,
                    _ => CompactionMode::Standard,
                };
                if cfg.prefs.plugins.auto_mode {
                    config.enable_set_routing = true;
                    config.enable_switch_mode = true;
                }
                config.cost_mode = cfg.prefs.plugins.cost_mode.clone();
                config.cost_models_low = cfg.prefs.plugins.cost_models_low.clone();
                config.cost_models_medium = cfg.prefs.plugins.cost_models_medium.clone();
                config.cost_models_high = cfg.prefs.plugins.cost_models_high.clone();
                let worktrees = worktrees_dir()
                    .unwrap_or_else(|_| std::env::temp_dir().join("agentloop-desktop-worktrees"));
                let mut backend = GitWorktrees::new(worktrees);
                if let Some(cap) = cfg.prefs.max_workspaces_per_project {
                    backend = backend.with_max_per_base(cap.max(1) as usize);
                }
                config.workspace = Some(Arc::new(backend));
                config.mcp = if include_mcp {
                    load_enabled_mcp_servers()
                } else {
                    McpBridgeConfig::default()
                };

                config.roles.extend(flex_composer_roles());
            }

            builder = builder.enable_workflow_tool(true);

            builder.build().map_err(DesktopError::from)
        })
    })
}

fn flex_composer_roles() -> Vec<RoleSpec> {
    let planner = RoleSpec {
        prompt: Some(
            "You are a planning specialist. Produce a complete implementation \
             plan for the given task: explore the codebase first (spawn \
             read-only subagents for broad sweeps when helpful), then emit a \
             structured plan — goals, ordered steps with concrete file paths, \
             risks, and verification — as your final message. Never mutate \
             files; planning only."
                .to_owned(),
        ),
        max_depth: 2,
        ..RoleSpec::new("planner")
    };
    let reviewer = RoleSpec {
        prompt: Some(
            "You are an independent plan reviewer. You receive ONLY a task \
             statement and a plan; you have no other context by design. \
             Adversarially review the plan: completeness against the task, \
             correctness of file references (verify them by reading), risks, \
             and missing verification. Your final message must start with a \
             single verdict line — APPROVED or REJECTED — followed by \
             numbered, actionable objections ordered by severity. Do not \
             rewrite the plan yourself."
                .to_owned(),
        ),
        max_depth: 0,
        ..RoleSpec::new("plan-reviewer")
    };
    let worker = RoleSpec {
        tools: RoleToolProfile::Full,
        prompt: Some(
            "You are an implementation worker. Implement EXACTLY your \
             assigned step from the plan; stay in scope. Run the project's \
             verification commands relevant to your change. Your final \
             message: what changed (files), verification results, and \
             anything remaining."
                .to_owned(),
        ),
        max_depth: 1,
        isolation: IsolationPolicy::Required,
        ..RoleSpec::new("flex-worker")
    };
    vec![planner, reviewer, worker]
}

pub fn open_session_store() -> DesktopResult<Arc<JsonlStore>> {
    let dir = sessions_dir()?;
    JsonlStore::open(dir)
        .map(Arc::new)
        .map_err(|e| DesktopError::Store(e.to_string()))
}
