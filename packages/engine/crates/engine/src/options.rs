use std::path::PathBuf;
use std::sync::Arc;

use agentloop_contracts::{CompactionMode, IsolationPolicy, ModelRef};
use agentloop_core::{Executor, NetworkPolicy, Plugin, SessionStore, Workspaces};
use agentloop_hooks::{DiagnosticsConfig, FormatterSpec};
use agentloop_loop::RetryPolicy;
use agentloop_loop::roles::RoleSpec;
use agentloop_mcp::{McpBridgeConfig, McpManager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputVerbosity {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Clone)]
pub struct EngineConfig {
    pub cwd: Option<PathBuf>,
    pub date: String,
    pub roles: Vec<RoleSpec>,
    pub mcp: McpBridgeConfig,
    pub mcp_manager: Option<std::sync::Arc<McpManager>>,
    pub session_store: Option<Arc<dyn SessionStore>>,
    pub max_iterations: Option<u32>,
    pub executor: Option<Arc<dyn Executor>>,
    pub network: NetworkPolicy,
    pub injection_scan: bool,
    pub workspace: Option<Arc<dyn Workspaces>>,
    pub isolation_default: IsolationPolicy,
    pub verify_command: Option<String>,
    pub formatters: Vec<FormatterSpec>,
    pub diagnostics: DiagnosticsConfig,
    pub plugins: Vec<Arc<dyn Plugin>>,
    pub verbosity: OutputVerbosity,
    pub default_fallback_models: Vec<ModelRef>,
    pub enable_workflow_tool: bool,
    pub enable_peer_messaging: bool,
    pub enable_switch_mode: bool,
    pub enable_set_routing: bool,
    pub cost_mode: String,
    pub cost_models_low: Vec<String>,
    pub cost_models_medium: Vec<String>,
    pub cost_models_high: Vec<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub auto_compact: bool,
    pub auto_compact_threshold_percent: u8,
    pub compaction_mode: CompactionMode,
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
            default_fallback_models: Vec::new(),
            enable_workflow_tool: false,
            enable_peer_messaging: false,
            enable_switch_mode: false,
            enable_set_routing: false,
            cost_mode: "auto".to_owned(),
            cost_models_low: vec![
                "anthropic/claude-haiku-4-5".to_owned(),
                "openai/gpt-4.1-mini".to_owned(),
                "deepseek/deepseek-v4-flash".to_owned(),
                "gemini/gemini-2.0-flash".to_owned(),
            ],
            cost_models_medium: vec![
                "anthropic/claude-sonnet-4-5".to_owned(),
                "openai/gpt-4.1".to_owned(),
                "deepseek/deepseek-v4-pro".to_owned(),
                "gemini/gemini-2.5-pro".to_owned(),
            ],
            cost_models_high: vec![
                "anthropic/claude-opus-4-5".to_owned(),
                "openai/o3".to_owned(),
                "openai/o1".to_owned(),
            ],
            retry_policy: None,
            auto_compact: true,
            auto_compact_threshold_percent: 85,
            compaction_mode: CompactionMode::Standard,
        }
    }
}
