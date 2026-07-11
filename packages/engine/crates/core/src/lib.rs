//! Core traits and registries — the contracts every implementation crate
//! programs against.
//!
//! Depends only on `agentloop-contracts` plus async plumbing (tokio sync
//! primitives, futures, async-trait). No I/O lives here: providers own HTTP,
//! delegators own processes, stores own files. This crate defines *what* an
//! [`Agent`], [`Provider`], [`Tool`], [`SessionStore`], and [`Hook`] are, and
//! the shared machinery (event sinks, pending request maps, metric recording)
//! their implementations compose.

pub mod agent;
pub mod event_sink;
pub mod executor;
pub mod hook;
pub mod observe;
pub mod pending;
pub mod plugin;
pub mod provider;
pub mod registry;
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
pub use store::{SessionStore, StoreError};
pub use tool::{
    PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, typed_tool,
};
pub use workspace::{Workspace, WorkspaceError, WorkspaceStatus, Workspaces};

/// Re-export of the contracts crate for convenience.
pub use agentloop_contracts as contracts;
