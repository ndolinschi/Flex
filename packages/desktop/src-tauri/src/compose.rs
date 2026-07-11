//! Sole `AgentBuilder` call site — the desktop composition root.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::IsolationPolicy;
use agentloop_engine::{RoleSpec, RoleToolProfile};
use agentloop_sdk::mcp_store::default_mcp_dir;
use agentloop_sdk::{AgentBuilder, EngineService, McpBridgeConfig, McpServerConfig};
use agentloop_session::JsonlStore;
use agentloop_workspace::GitWorktrees;

use crate::config::{ProviderConfig, sessions_dir, worktrees_dir};
use crate::error::{DesktopError, DesktopResult};

/// Read every enabled MCP server spec from `~/.config/agentloop/mcp/*.toml`
/// (blocking `std::fs`, since `build_service` itself is sync and runs both
/// from `setup()` and from `async fn` command bodies — the same file layout
/// `FileMcpStore` reads asynchronously, but composition needs a plain
/// synchronous read here). A missing directory or an unreadable/malformed
/// file is non-fatal: MCP servers are additive, so a bad entry just logs a
/// warning and is skipped rather than blocking the whole engine build.
fn load_enabled_mcp_servers() -> McpBridgeConfig {
    let Some(dir) = default_mcp_dir() else {
        return McpBridgeConfig::default();
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return McpBridgeConfig::default(),
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
            Ok(server) if server.enabled => servers.push(server),
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

/// Map a provider id to the env var its client reads for host/base URL.
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

/// Apply an optional host override the same way the CLI does (env), then
/// clear it after build so we don't leak into other processes' expectations
/// within this app lifetime beyond the current service.
fn with_base_url_env<T>(provider: &str, base_url: Option<&str>, f: impl FnOnce() -> T) -> T {
    let env_key = base_url_env(provider);
    let previous = env_key.and_then(|k| std::env::var(k).ok());
    if let (Some(key), Some(url)) = (env_key, base_url.filter(|s| !s.is_empty())) {
        // SAFETY: single-threaded at composition time in the desktop shell;
        // we restore the previous value immediately after `f`.
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

/// Bug fix (form-supplied Bedrock key ignored by Validate): `resolve_real_providers`
/// (`packages/providers/crates/providers/src/resolve.rs`, read-only from here — see
/// AGENTS.md) resolves its *initial* Bedrock provider purely from
/// `BedrockProvider::from_env()` and returns `AuthMissing` immediately if
/// `AWS_BEARER_TOKEN_BEDROCK`/SigV4 env vars are unset, **before**
/// `native()`'s follow-up `connect_bedrock(&opts.provider_keys, ...)` call
/// (which *does* honor a client-supplied key) ever runs. A freshly pasted
/// form key never reaches env, so that first call always fails and the whole
/// `AgentBuilder::build()` errors out with "authentication missing for
/// bedrock" even though a perfectly good key was passed via `.provider_key`.
///
/// Desktop-layer workaround (mirrors `with_base_url_env`): scope
/// `AWS_BEARER_TOKEN_BEDROCK` (and `BEDROCK_REGION`, if given) to the
/// duration of `f`, just enough to satisfy the early `has_credentials()`
/// check. `connect_bedrock` still runs afterward and re-registers Bedrock
/// with the exact key/region passed in `provider_keys`/`provider_regions`
/// (`ProviderRegistry::register` replaces by id), so the final registered
/// provider is correct regardless of what this scoped env var held.
fn with_bedrock_env<T>(key: Option<&str>, region: Option<&str>, f: impl FnOnce() -> T) -> T {
    let key = key.filter(|k| !k.is_empty());
    let Some(key) = key else {
        return f();
    };
    let prev_token = std::env::var("AWS_BEARER_TOKEN_BEDROCK").ok();
    let prev_region = std::env::var("BEDROCK_REGION").ok();
    // SAFETY: single-threaded at composition time in the desktop shell; both
    // vars are restored immediately after `f`.
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

/// The provider/model/key inputs `build_service` needs, resolved from either
/// the active profile or (fallback) the legacy top-level prefs fields — see
/// `ProviderConfig::active_profile`.
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
    // Legacy fallback: no profile migrated yet (shouldn't normally happen —
    // `load_config` always migrates — but keeps this function correct if
    // called with a hand-built `ProviderConfig`, e.g. in tests).
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

/// Build (or rebuild) an [`EngineService`] from keychain-backed config.
///
/// Always goes through [`AgentBuilder`] — never `EngineService::native` or
/// `providers::resolve_*`. No connectors/delegators. Reads the active
/// profile (see `ProviderConfig::active_profile`) as its single source for
/// provider/key/region/model/fallbacks/isolation.
pub fn build_service(
    cfg: &ProviderConfig,
    store: Arc<JsonlStore>,
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
    let needs_all = fallbacks.iter().any(|m| {
        provider_of_model(m)
            .map(|p| p != preferred)
            .unwrap_or(false)
    });
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
            // Bedrock is the only region-scoped built-in today; the settings
            // form's "Region" field (repurposed per-provider, like `base_url`)
            // carries it through to `BedrockConfig` instead of the AWS env vars.
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
            if cfg.prefs.plugins.learning {
                builder = builder.enable_plugin("learning");
            }
            if cfg.prefs.plugins.verifier {
                builder = builder.enable_plugin("verifier");
            }

            {
                let config = builder.config_mut();
                config.session_store = Some(store.clone());
                config.verbosity = agentloop_sdk::OutputVerbosity::High;
                // Opt-in isolation per session; backend always available for undo snapshots.
                let worktrees = worktrees_dir()
                    .unwrap_or_else(|_| std::env::temp_dir().join("agentloop-desktop-worktrees"));
                config.workspace = Some(Arc::new(GitWorktrees::new(worktrees)));
                // User-configured MCP servers (`~/.config/agentloop/mcp/*.toml`,
                // managed by the "MCP Servers" section of the Customize page).
                // Loading them here means a saved add/remove/toggle only takes
                // effect once the service is rebuilt (`save_provider_config` or
                // the MCP commands below) — no hot-reload of a running session.
                config.mcp = load_enabled_mcp_servers();

                // Flex composer-mode roles: orchestrated planning -> independent
                // review -> isolated implementation. These back the Flex
                // composer mode (a later wave wires an orchestrator prompt and a
                // "composer" turn mode that instructs the model to use them);
                // registering them here is inert on its own — nothing in today's
                // Agent-mode system prompt or turn options references these role
                // names, so existing sessions are unaffected until a future wave
                // tells the model to spawn them via `Agent`/`RunWorkflow`.
                //
                // `enable_workflow_tool` is turned on alongside them: the
                // composer mode's orchestrator needs `RunWorkflow` to run a
                // multi-step plan (`planner` -> `plan-reviewer` -> one or more
                // `flex-worker`s) without waiting on its own next turn between
                // steps. This does add one more tool to every session's tool
                // list (not just composer-mode ones) — but `workflow_tool`'s
                // description (`agentloop_tools::workflow.rs`) is purely
                // descriptive of whatever roles are registered and says to
                // "prefer `Agent` for a single task"; it does not claim
                // composer-specific behavior, so a normal Agent-mode session
                // reading it is not misled, just offered a pipeline tool it can
                // reasonably choose to ignore.
                config.roles.extend(flex_composer_roles());
            }

            builder = builder.enable_workflow_tool(true);

            builder.build().map_err(DesktopError::from)
        })
    })
}

/// Roles backing the Flex composer mode: orchestrated planning, an
/// independent plan review, and isolated implementation workers. All three
/// leave `models` empty (inherit / per-call override) so tier routing stays a
/// composer-side decision. Inert until a turn's system prompt instructs the
/// model to spawn them.
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
