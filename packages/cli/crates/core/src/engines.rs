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

use agentloop_contracts::SessionId;
use agentloop_core::SessionStore;
use agentloop_delegator_claude_code::{ClaudeCodeConfig, DelegatorProbeStatus, claude_code_agent};
use agentloop_delegator_copilot::{CopilotConfig as CopilotDelegatorConfig, copilot_agent};
use agentloop_engine::{EngineOptions, EngineService, EngineServiceError};
use agentloop_mcp::McpManager;
use agentloop_session::MemoryStore;

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

impl AgentKind {
    /// Every selectable kind, in display order.
    pub const ALL: [Self; 3] = [Self::Native, Self::ClaudeCode, Self::Copilot];

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
        }
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
        let (roles, skipped_roles) = crate::prefs::role_specs(&prefs);
        let mcp_store = crate::mcp_store::McpStore::load();
        let mcp_config = mcp_store.to_bridge_config();
        shutdown_mcp_manager(self.mcp_manager.take());
        let store = self
            .native_store
            .get_or_insert_with(|| Arc::new(MemoryStore::new()) as Arc<dyn SessionStore>)
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
        for (name, reason) in skipped_roles {
            trace.push(format!("skipped role {name}: {reason}"));
        }
        Ok((service, trace))
    }
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
