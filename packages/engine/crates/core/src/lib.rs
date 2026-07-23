pub mod agent;
pub mod event_sink;
pub mod executor;
pub mod hook;
pub mod observe;
pub mod pending;
pub mod plugin;
pub mod provider;
pub mod registry;
pub mod routing;
pub mod store;
pub mod tool;
pub mod workspace;

pub use agent::{Agent, AgentError, EventStream};
pub use event_sink::EventSink;
pub use executor::{
    BackgroundEntry, BackgroundEntrySummary, BackgroundProcess, BackgroundProcessRegistry,
    BackgroundSpawn, BackgroundStatus, ChunkSink, DemoteRegistry, ExecError, ExecOrDemoted,
    ExecOutcome, ExecSpec, ExecStream, Executor, ExecutorHealth, NetworkPolicy,
};
pub use hook::{Hook, HookContext, HookData, HookError, HookOutcome};
pub use pending::PendingMap;
pub use plugin::{Plugin, PluginRegistry, PluginRole, PluginRoleTools};
pub use provider::{
    ChatRequest, Provider, ProviderError, ProviderStream, ProviderStreamEvent, ThinkingConfig,
    ToolChoice, ToolSpec,
};
pub use registry::{ProviderRegistry, ToolFilter, ToolRegistry};
pub use routing::{RoutingOverride, RoutingTable};
pub use store::{SessionStore, StoreError, StoredEvent};
pub use tool::{
    PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, typed_tool,
};
pub use workspace::{Workspace, WorkspaceError, WorkspaceStatus, Workspaces};

pub use agentloop_contracts as contracts;
