//! Composition inputs for the native engine service.
//!
//! These are the engine-scoped knobs only. Provider selection and construction
//! live outside the engine (in the `providers` facade), which resolves a
//! [`ProviderRegistry`] and default model and hands them to
//! [`EngineService::native`](crate::EngineService::native) together with an
//! [`EngineConfig`].

use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::IsolationPolicy;
use agentloop_core::{Executor, NetworkPolicy, Plugin, SessionStore, Workspaces};
use agentloop_hooks::{DiagnosticsConfig, FormatterSpec};
use agentloop_loop::roles::RoleSpec;
use agentloop_mcp::{McpBridgeConfig, McpManager};

/// Control the verbosity of NDJSON event output in the stdio transport.
///
/// At [`Low`](OutputVerbosity::Low), only materialized turn-level events are
/// emitted; streaming deltas and intermediate noisy events are suppressed.
/// At [`Medium`](OutputVerbosity::Medium) (the default), all events are
/// emitted except for streaming deltas. At [`High`](OutputVerbosity::High),
/// every event passes through — including per-token streaming deltas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputVerbosity {
    /// Only materialized turn-level events: no streaming deltas or intermediate
    /// internal events.
    Low,
    /// All events except streaming deltas (default).
    #[default]
    Medium,
    /// All events, including per-token streaming deltas.
    High,
}

#[derive(Clone)]
pub struct EngineConfig {
    pub cwd: Option<PathBuf>,
    pub date: String,
    /// Role definitions for multi-agent orchestration (built-ins always present).
    pub roles: Vec<RoleSpec>,
    /// MCP servers bridged into the native tool registry.
    pub mcp: McpBridgeConfig,
    /// Pre-built MCP manager; when set, the engine reuses it instead of creating a new one.
    pub mcp_manager: Option<std::sync::Arc<McpManager>>,
    /// Reuse an existing session store across native service rebuilds.
    pub session_store: Option<Arc<dyn SessionStore>>,
    /// Model-call iterations per turn; `None` keeps the engine default
    /// (currently 500 — a backstop against a runaway loop, not a budget for
    /// normal work).
    pub max_iterations: Option<u32>,
    /// Command-execution backend for shell tools. `None` = run directly on
    /// the host (`/bin/sh -lc`, the historical behavior); set a container or
    /// remote backend to sandbox command execution.
    pub executor: Option<Arc<dyn Executor>>,
    /// Network posture for shell commands. `Denied` is enforced best-effort
    /// by the executor backend (container backends drop the network; the
    /// local backend rejects the call).
    pub network: NetworkPolicy,
    /// Scan tool results with prompt-injection heuristics and fence flagged
    /// content in an explicit warning before the model reads it. Off by
    /// default.
    pub injection_scan: bool,
    /// Isolation backend. When set, root sessions can run in an isolated
    /// workspace; when `None`, isolation requests degrade or fail per policy.
    pub workspace: Option<Arc<dyn Workspaces>>,
    /// Run-level default isolation applied to a root session that doesn't
    /// request its own (and whose role doesn't force one). `Never` = off.
    pub isolation_default: IsolationPolicy,
    /// Command run inside an isolated workspace before integrating it back
    /// (e.g. `"cargo test"`). `None` skips verification.
    pub verify_command: Option<String>,
    /// Formatters run after `Write`/`Edit` (format-on-edit). Empty = off.
    /// Each spec is availability-gated on its command resolving on `$PATH`.
    pub formatters: Vec<FormatterSpec>,
    /// Diagnostics feedback run after `Write`/`Edit`. Disabled by default;
    /// availability-gated on the check command resolving on `$PATH`.
    pub diagnostics: DiagnosticsConfig,
    /// Enabled plugins, each contributing tools, prompt fragments, and roles at
    /// composition time. Empty = the base tool set only (byte-identical to the
    /// pre-plugin engine).
    pub plugins: Vec<Arc<dyn Plugin>>,
    /// NDJSON event verbosity for the stdio transport. Defaults to
    /// [`OutputVerbosity::Medium`].
    pub verbosity: OutputVerbosity,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            cwd: None,
            date: String::new(),
            roles: Vec::new(),
            mcp: McpBridgeConfig::default(),
            mcp_manager: None,
            session_store: None,
            max_iterations: None,
            executor: None,
            network: NetworkPolicy::Allowed,
            injection_scan: false,
            workspace: None,
            isolation_default: IsolationPolicy::Never,
            verify_command: None,
            formatters: Vec::new(),
            diagnostics: DiagnosticsConfig::default(),
            plugins: Vec::new(),
            verbosity: OutputVerbosity::default(),
        }
    }
}
