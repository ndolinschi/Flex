//! Sole `AgentBuilder` call site — the desktop composition root.

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_sdk::{AgentBuilder, EngineService};
use agentloop_session::JsonlStore;
use agentloop_workspace::GitWorktrees;

use crate::config::{ProviderConfig, sessions_dir, worktrees_dir};
use crate::error::{DesktopError, DesktopResult};

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

/// Build (or rebuild) an [`EngineService`] from keychain-backed config.
///
/// Always goes through [`AgentBuilder`] — never `EngineService::native` or
/// `providers::resolve_*`. No connectors/delegators.
pub fn build_service(
    cfg: &ProviderConfig,
    store: Arc<JsonlStore>,
) -> DesktopResult<EngineService> {
    let preferred = cfg
        .prefs
        .preferred_provider
        .as_deref()
        .ok_or(DesktopError::NotConfigured)?;

    let cwd = cfg
        .prefs
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let base_url = cfg.prefs.base_url.as_deref();
    let fallbacks = cfg.prefs.fallback_models.clone();
    let needs_all = fallbacks.iter().any(|m| {
        provider_of_model(m)
            .map(|p| p != preferred)
            .unwrap_or(false)
    });

    with_base_url_env(preferred, base_url, || {
        let mut builder = AgentBuilder::new().cwd(cwd.clone()).provider(preferred);

        if let Some(model) = &cfg.prefs.default_model {
            builder = builder.model(model.clone());
        }

        for (id, key) in &cfg.keys {
            builder = builder.provider_key(id.clone(), key.clone());
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
            let worktrees = worktrees_dir().unwrap_or_else(|_| {
                std::env::temp_dir().join("agentloop-desktop-worktrees")
            });
            config.workspace = Some(Arc::new(GitWorktrees::new(worktrees)));
        }

        builder.build().map_err(DesktopError::from)
    })
}

pub fn open_session_store() -> DesktopResult<Arc<JsonlStore>> {
    let dir = sessions_dir()?;
    JsonlStore::open(dir)
        .map(Arc::new)
        .map_err(|e| DesktopError::Store(e.to_string()))
}
