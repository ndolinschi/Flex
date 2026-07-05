//! Agent selection and service construction — the CLI's composition root.
//!
//! Mirrors the engine runner's resolution logic (probe + human-readable
//! trace) but keeps one lazily-built [`EngineService`] per agent kind so the
//! TUI can switch between the native loop and external-agent delegators at
//! runtime and later switch back to a remembered session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{IsolationPolicy, SessionId};
use agentloop_core::SessionStore;
use agentloop_delegator_claude_code::{ClaudeCodeConfig, DelegatorProbeStatus, claude_code_agent};
use agentloop_delegator_copilot::{CopilotConfig as CopilotDelegatorConfig, copilot_agent};
use agentloop_engine::{EngineOptions, EngineService, EngineServiceError};
use agentloop_mcp::McpManager;
use agentloop_session::{JsonlStore, MemoryStore};
use agentloop_workspace::GitWorktrees;

/// Which agent implementation serves the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AgentKind {
    /// The engine's own loop calling provider APIs directly.
    Native,
    /// The external `claude` CLI driven as a subprocess.
    ClaudeCode,
    /// The external `copilot` CLI driven as a subprocess.
    Copilot,
}

/// Whether delegated (non-native) agents — `claude-code`, `copilot` — are
/// selectable. Off by default: these shell out to external CLIs whose
/// process/auth/protocol issues have shown up as user-facing crashes (exit 1
/// with no stderr, provider `Bad Request`s the delegator can't retry). The
/// native loop's own provider system reaches the same LLMs (e.g.
/// `/provider copilot`) without that fragility. Set
/// `FLEX_ENABLE_DELEGATED_AGENTS=1` to bring them back.
pub fn delegated_agents_enabled() -> bool {
    std::env::var("FLEX_ENABLE_DELEGATED_AGENTS")
        .is_ok_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

impl AgentKind {
    /// Every agent kind that exists, in display order — including ones
    /// currently feature-flagged off. Use [`Self::selectable`] for anything
    /// user-facing (pickers, `/agent` validation).
    pub const ALL: [Self; 3] = [Self::Native, Self::ClaudeCode, Self::Copilot];

    /// Kinds a user can actually pick right now. Native-only unless
    /// [`delegated_agents_enabled`] opts back into the delegators.
    pub fn selectable() -> Vec<Self> {
        if delegated_agents_enabled() {
            Self::ALL.to_vec()
        } else {
            vec![Self::Native]
        }
    }

    /// Stable identifier used in slash commands and flags.
    pub fn id(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::ClaudeCode => "claude-code",
            Self::Copilot => "copilot",
        }
    }

    /// Parse a user-supplied identifier.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "native" => Some(Self::Native),
            "claude-code" => Some(Self::ClaudeCode),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// Why a service could not be built for an agent kind.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HubError {
    /// The engine could not compose a native service (e.g. no credentials).
    #[error(transparent)]
    Engine(#[from] EngineServiceError),
    /// The external CLI backing a delegator is not installed.
    #[error("{agent} is not available: {hint}")]
    NotInstalled {
        /// Agent identifier (e.g. `claude-code`).
        agent: &'static str,
        /// Installation hint from the probe.
        hint: String,
    },
    /// Probing the external CLI failed outright.
    #[error("failed to probe {agent}: {message}")]
    Probe {
        /// Agent identifier.
        agent: &'static str,
        /// Probe failure detail.
        message: String,
    },
}

/// Lazily-built [`EngineService`]s, one per [`AgentKind`], plus the
/// resolution trace and last-used session for each.
pub struct EngineHub {
    cwd: PathBuf,
    /// Preferred provider for the native service (None = auto priority).
    provider: Option<String>,
    /// Default model for the native service (None = provider default).
    model: Option<String>,
    services: HashMap<AgentKind, EngineService>,
    traces: HashMap<AgentKind, Vec<String>>,
    last_session: HashMap<AgentKind, SessionId>,
    /// Native session store shared across MCP reload / invalidate rebuilds.
    native_store: Option<Arc<dyn SessionStore>>,
    /// Live MCP connections for the native service; shut down before rebuild.
    mcp_manager: Option<Arc<McpManager>>,
    /// Run-level isolation posture for new native root sessions.
    isolation: IsolationPolicy,
    /// Command run inside an isolated workspace before integrating it back.
    verify_command: Option<String>,
}

impl EngineHub {
    /// A hub rooted at `cwd` with optional native provider/model preferences
    /// (typically from `--provider`/`--model` flags).
    pub fn new(cwd: PathBuf, provider: Option<String>, model: Option<String>) -> Self {
        Self {
            cwd,
            provider,
            model,
            services: HashMap::new(),
            traces: HashMap::new(),
            last_session: HashMap::new(),
            native_store: None,
            mcp_manager: None,
            isolation: IsolationPolicy::Never,
            verify_command: None,
        }
    }

    /// Set the run-level isolation posture and the verify command used before
    /// integrating an isolated workspace. Takes effect on the next native
    /// (re)build. `Never` (the default) keeps sessions unisolated.
    pub fn set_isolation(&mut self, policy: IsolationPolicy, verify_command: Option<String>) {
        self.isolation = policy;
        self.verify_command = verify_command;
    }

    /// Change only the isolation policy (keeping the verify command), for a
    /// runtime `/isolation on|off` toggle. Takes effect on the next native
    /// (re)build.
    pub fn set_isolation_policy(&mut self, policy: IsolationPolicy) {
        self.isolation = policy;
    }

    /// The current run-level isolation policy.
    pub fn isolation(&self) -> IsolationPolicy {
        self.isolation
    }

    /// The working directory sessions run in.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The service for `kind`, building (and caching) it on first use.
    /// Returns a clone — [`EngineService`] is cheaply cloneable.
    pub async fn service(&mut self, kind: AgentKind) -> Result<EngineService, HubError> {
        if let Some(service) = self.services.get(&kind) {
            return Ok(service.clone());
        }
        let (service, trace) = match kind {
            AgentKind::Native => self.build_native()?,
            AgentKind::ClaudeCode => build_claude_code(&self.cwd).await?,
            AgentKind::Copilot => build_copilot_delegator(&self.cwd).await?,
        };
        for line in &trace {
            tracing::info!(target: "resolution", agent = kind.id(), "{line}");
        }
        self.traces.insert(kind, trace);
        self.services.insert(kind, service.clone());
        Ok(service)
    }

    /// How the service for `kind` was resolved (empty if never built).
    pub fn trace(&self, kind: AgentKind) -> &[String] {
        self.traces.get(&kind).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Drop a cached service so the next [`Self::service`] call rebuilds it —
    /// used after a login changes which providers resolve.
    pub fn invalidate(&mut self, kind: AgentKind) {
        if kind == AgentKind::Native {
            shutdown_mcp_manager(self.mcp_manager.take());
        }
        self.services.remove(&kind);
        self.traces.remove(&kind);
        self.last_session.remove(&kind);
    }

    /// The live MCP manager for the native service, if any.
    pub fn mcp_manager(&self) -> Option<Arc<McpManager>> {
        self.mcp_manager.clone()
    }

    /// Shut down MCP connections without invalidating the cached native service.
    pub fn shutdown_mcp(&mut self) {
        shutdown_mcp_manager(self.mcp_manager.take());
    }

    /// Remember the session in use for `kind` so switching back can resume.
    pub fn remember_session(&mut self, kind: AgentKind, id: SessionId) {
        self.last_session.insert(kind, id);
    }

    /// The last session used with `kind`, if any.
    pub fn last_session(&self, kind: AgentKind) -> Option<&SessionId> {
        self.last_session.get(&kind)
    }

    fn build_native(&mut self) -> Result<(EngineService, Vec<String>), HubError> {
        // Re-read prefs on every (re)build so `/connect` followed by an
        // invalidate picks up new custom providers without extra plumbing.
        let prefs = crate::prefs::CliPrefs::load();
        let (custom, skipped) = crate::prefs::custom_specs(&prefs);
        let (provider_keys, skipped_keys) = crate::prefs::provider_keys(&prefs);
        let (mut roles, skipped_roles) = crate::prefs::role_specs(&prefs);
        // Auto-apply the DeepSeek orchestration split — research subagents
        // (searcher) on the fast v4-flash, implementation (worker) on the
        // strong v4-pro, mirroring Claude Code's Haiku-search / Sonnet-work
        // default — when the active provider/model is DeepSeek and the user
        // hasn't customized these roles. In-memory only (not written to
        // config); `/roles preset deepseek` is the explicit, persistent form.
        if deepseek_is_active(&prefs, self.provider.as_deref(), self.model.as_deref()) {
            let missing: std::collections::BTreeMap<String, crate::prefs::RoleConfig> =
                crate::prefs::deepseek_roles_preset()
                    .into_iter()
                    .filter(|(name, _)| !prefs.roles.contains_key(name))
                    .collect();
            if !missing.is_empty() {
                let preset_prefs = crate::prefs::CliPrefs {
                    roles: missing,
                    ..Default::default()
                };
                let (preset_specs, _) = crate::prefs::role_specs(&preset_prefs);
                roles.extend(preset_specs);
            }
        }
        let mcp_store = crate::mcp_store::McpStore::load();
        let mcp_config = mcp_store.to_bridge_config();
        shutdown_mcp_manager(self.mcp_manager.take());
        let store = self
            .native_store
            .get_or_insert_with(open_native_store)
            .clone();
        let mcp_manager = if mcp_config.servers.iter().any(|server| server.enabled) {
            let manager = Arc::new(
                McpManager::from_config_blocking_default(mcp_config.clone())
                    .map_err(EngineServiceError::from)?,
            );
            self.mcp_manager = Some(manager.clone());
            Some(manager)
        } else {
            self.mcp_manager = None;
            None
        };
        // Provision isolated workspaces under the state dir when isolation is
        // on; falls back to no backend if the state dir can't be resolved
        // (isolation then degrades or fails per policy).
        let workspace = crate::sessions::worktrees_dir()
            .map(|root| Arc::new(GitWorktrees::new(root)) as Arc<dyn agentloop_core::Workspaces>);
        let service = EngineService::native_all(EngineOptions {
            provider: self.provider.clone(),
            model: self.model.clone(),
            cwd: self.cwd.clone(),
            date: today(),
            custom,
            roles,
            mcp: mcp_config,
            mcp_manager,
            session_store: Some(store),
            max_iterations: prefs.max_iterations,
            workspace,
            isolation_default: self.isolation,
            verify_command: self.verify_command.clone(),
            formatters: Vec::new(),
            diagnostics: agentloop_engine::DiagnosticsConfig::default(),
            provider_keys,
        })?;
        let mut trace = vec!["selected native loop".to_owned()];
        let ids = service
            .provider_registry()
            .ids()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        trace.push(format!("registered providers: {}", ids.join(", ")));
        let enabled_mcps = mcp_store.enabled_count();
        if enabled_mcps > 0 {
            trace.push(format!("enabled MCP servers: {enabled_mcps}"));
        }
        for (id, reason) in skipped {
            trace.push(format!("skipped custom provider {id}: {reason}"));
        }
        for (id, reason) in skipped_keys {
            trace.push(format!("skipped provider key {id}: {reason}"));
        }
        for (name, reason) in skipped_roles {
            trace.push(format!("skipped role {name}: {reason}"));
        }
        Ok((service, trace))
    }
}

/// Whether the effective native provider/model is DeepSeek — used to decide
/// the automatic role split. Checks the `--provider`/`--model` overrides and
/// the persisted `last_model` (`deepseek/…`).
/// Open the on-disk session store so a session survives closing the
/// terminal and can be resumed later (see [`crate::sessions`]). Falls back
/// to an in-memory store — degraded but still functional for the current
/// process — if the state directory can't be resolved or opened (e.g. a
/// read-only home directory), rather than failing startup over history.
fn open_native_store() -> Arc<dyn SessionStore> {
    let Some(dir) = crate::sessions::sessions_dir() else {
        tracing::warn!(
            target: "sessions",
            "could not resolve a session state directory (HOME/XDG_STATE_HOME unset); \
             sessions will not survive restarting flex"
        );
        return Arc::new(MemoryStore::new());
    };
    match JsonlStore::open(&dir) {
        Ok(store) => Arc::new(store),
        Err(err) => {
            tracing::warn!(
                target: "sessions",
                path = %dir.display(),
                "could not open the session store, falling back to in-memory: {err}"
            );
            Arc::new(MemoryStore::new())
        }
    }
}

fn deepseek_is_active(
    prefs: &crate::prefs::CliPrefs,
    provider: Option<&str>,
    model: Option<&str>,
) -> bool {
    let is_deepseek_model = |m: &str| m.starts_with("deepseek/");
    provider == Some("deepseek")
        || model.is_some_and(is_deepseek_model)
        || prefs.last_model.as_deref().is_some_and(is_deepseek_model)
}

fn shutdown_mcp_manager(manager: Option<Arc<McpManager>>) {
    let Some(manager) = manager else {
        return;
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(manager.shutdown()));
    } else if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        runtime.block_on(manager.shutdown());
    }
}

async fn build_claude_code(cwd: &Path) -> Result<(EngineService, Vec<String>), HubError> {
    let config = ClaudeCodeConfig {
        cwd: Some(cwd.to_path_buf()),
        ..ClaudeCodeConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(claude_code_agent(config, store.clone()));
    let mut trace = Vec::new();
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `claude`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator claude-code".to_owned());
            Ok((EngineService::new(agent, store), trace))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => Err(HubError::NotInstalled {
            agent: "claude-code",
            hint,
        }),
        Err(err) => Err(HubError::Probe {
            agent: "claude-code",
            message: err.to_string(),
        }),
    }
}

async fn build_copilot_delegator(cwd: &Path) -> Result<(EngineService, Vec<String>), HubError> {
    let config = CopilotDelegatorConfig {
        cwd: Some(cwd.to_path_buf()),
        ..CopilotDelegatorConfig::default()
    };
    let store = Arc::new(MemoryStore::new());
    let agent = Arc::new(copilot_agent(config, store.clone()));
    let mut trace = Vec::new();
    match agent.probe(CancellationToken::new()).await {
        Ok(DelegatorProbeStatus::Installed { version }) => {
            trace.push(format!(
                "probed `copilot`: installed ({})",
                version.as_deref().unwrap_or("version unknown")
            ));
            trace.push("selected delegator copilot".to_owned());
            Ok((EngineService::new(agent, store), trace))
        }
        Ok(DelegatorProbeStatus::NotInstalled { hint }) => Err(HubError::NotInstalled {
            agent: "copilot",
            hint,
        }),
        Err(err) => Err(HubError::Probe {
            agent: "copilot",
            message: err.to_string(),
        }),
    }
}

/// Coarse ISO date from the epoch — the system prompt only needs day
/// resolution and this crate deliberately has no chrono dependency (mirrors
/// the engine runner).
fn today() -> String {
    let days = agentloop_contracts::now_ms() / 86_400_000;
    let mut year = 1970u64;
    let mut remaining = days;
    loop {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let len = if leap { 366 } else { 365 };
        if remaining < len {
            break;
        }
        remaining -= len;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for len in month_lengths {
        if remaining < len {
            break;
        }
        remaining -= len;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02}", remaining + 1)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn agent_kind_round_trips_ids() {
        for kind in AgentKind::ALL {
            assert_eq!(AgentKind::parse(kind.id()), Some(kind));
        }
        assert_eq!(AgentKind::parse("nope"), None);
    }

    #[test]
    fn deepseek_is_active_detects_provider_model_and_last_model() {
        let mut prefs = crate::prefs::CliPrefs::default();
        assert!(!deepseek_is_active(&prefs, None, None));
        assert!(deepseek_is_active(&prefs, Some("deepseek"), None));
        assert!(deepseek_is_active(
            &prefs,
            None,
            Some("deepseek/deepseek-v4-pro")
        ));
        assert!(!deepseek_is_active(
            &prefs,
            Some("anthropic"),
            Some("anthropic/claude-sonnet-5")
        ));
        prefs.last_model = Some("deepseek/deepseek-v4-flash".to_owned());
        assert!(deepseek_is_active(&prefs, None, None));
    }

    #[test]
    fn delegated_agents_disabled_by_default() {
        temp_env::with_var_unset("FLEX_ENABLE_DELEGATED_AGENTS", || {
            assert!(!delegated_agents_enabled());
            assert_eq!(AgentKind::selectable(), vec![AgentKind::Native]);
        });
    }

    #[test]
    fn delegated_agents_enabled_via_env_flag() {
        temp_env::with_var("FLEX_ENABLE_DELEGATED_AGENTS", Some("1"), || {
            assert!(delegated_agents_enabled());
            assert_eq!(AgentKind::selectable().len(), AgentKind::ALL.len());
        });
    }

    #[test]
    fn today_is_iso_shaped() {
        let date = today();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn invalidate_does_not_drop_native_store() {
        let mut hub = EngineHub::new(PathBuf::from("."), None, None);
        let store: Arc<dyn SessionStore> = Arc::new(MemoryStore::new());
        let ptr = Arc::as_ptr(&store);
        hub.native_store = Some(store);
        hub.invalidate(AgentKind::Native);
        let kept = hub
            .native_store
            .as_ref()
            .expect("native store survives invalidate");
        assert_eq!(Arc::as_ptr(kept), ptr);
    }
}
